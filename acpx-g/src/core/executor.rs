//! Node executor traits and built-in implementations.

use std::collections::HashMap;

use crate::core::error::CoreError;

/// Result of executing a single node.
pub struct NodeResult {
    /// Exit code (0 = success).
    pub exit_code: i64,
    /// Captured stdout.
    pub stdout: String,
    /// Captured stderr.
    pub stderr: String,
    /// Dynamic outputs parsed from `$ACPX_OUTPUT` file.
    pub dynamic_outputs: HashMap<String, String>,
}

/// Trait for node execution. Implementations can be sync or async —
/// the core engine invokes them via thread spawn.
pub trait NodeExecutor: Send + Sync {
    /// Execute a node and return the result.
    fn execute(&self, ctx: &ExecutionContext) -> Result<NodeResult, CoreError>;
}

/// Context passed to a node executor.
pub struct ExecutionContext {
    /// The node's business ID.
    pub node_id: String,
    /// Resolved script or prompt content (after template interpolation).
    pub content: String,
    /// Merged environment variables.
    pub env: HashMap<String, String>,
    /// Timeout in seconds (None = unlimited).
    pub timeout_secs: Option<u64>,
    /// Number of retry attempts (0 = no retry).
    pub retries: u32,
    /// Shell override (e.g. "bash -c", "zsh -c").
    pub shell_override: Option<String>,
    /// Agent-specific fields.
    pub agent_name: Option<String>,
    pub agent_model: Option<String>,
    pub agent_cwd: Option<String>,
    /// Check if execution should be cancelled.
    pub is_cancelled: Box<dyn Fn() -> bool + Send + Sync>,
}

// ─── Shell Executor ──────────────────────────────────────────────

/// Environment variable name pointing to the output file for dynamic outputs.
pub const ACPX_OUTPUT_ENV: &str = "ACPX_OUTPUT";

/// Maximum stdout/stderr length stored per node (256 KB).
pub const MAX_STORED_OUTPUT: usize = 256 * 1024;

/// Truncate output for storage, respecting UTF-8 char boundaries.
pub fn truncate_for_storage(s: &str) -> String {
    if s.len() <= MAX_STORED_OUTPUT {
        return s.to_string();
    }
    let mut end = MAX_STORED_OUTPUT;
    while !s.is_char_boundary(end) && end > 0 {
        end -= 1;
    }
    format!("{}\n\n... [truncated, {} bytes total]", &s[..end], s.len())
}

/// Load script content from a resolved script source.
pub fn load_script(resolved: &crate::core::schema::ResolvedScript) -> Result<String, CoreError> {
    match resolved {
        crate::core::schema::ResolvedScript::Inline(s) => Ok(s.clone()),
        crate::core::schema::ResolvedScript::File(path) => std::fs::read_to_string(path)
            .map_err(|e| CoreError::io(format!("failed to read script file: {path}: {e}"))),
    }
}

/// Load prompt content from a resolved prompt source.
pub fn load_prompt(resolved: &crate::core::schema::ResolvedPrompt) -> Result<String, CoreError> {
    match resolved {
        crate::core::schema::ResolvedPrompt::Inline(s) => Ok(s.clone()),
        crate::core::schema::ResolvedPrompt::File(path) => std::fs::read_to_string(path)
            .map_err(|e| CoreError::io(format!("failed to read prompt file: {path}: {e}"))),
    }
}

/// Build merged environment: process env + node env.
pub fn build_env(node_env: &HashMap<String, String>) -> HashMap<String, String> {
    let mut env: HashMap<String, String> = std::env::vars().collect();
    for (k, v) in node_env {
        env.insert(k.clone(), v.clone());
    }
    env
}

/// Parse `$ACPX_OUTPUT` file: each line should be `key=value`.
/// Lines without `=` or empty lines are skipped. Last value wins.
pub fn parse_output_file(path: &str) -> HashMap<String, String> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return HashMap::new(),
    };

    let mut outputs = HashMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            outputs.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    outputs
}

/// Generic retry loop with exponential backoff.
/// Returns the successful result or the last error.
pub fn execute_with_retry<F>(
    max_retries: u32,
    is_cancelled: &dyn Fn() -> bool,
    mut execute_fn: F,
) -> Result<NodeResult, CoreError>
where
    F: FnMut() -> Result<NodeResult, CoreError>,
{
    let max_attempts = max_retries + 1;
    let mut last_error = None;

    for attempt in 0..max_attempts {
        if is_cancelled() {
            return Err(CoreError::cancelled("cancelled by user"));
        }

        match execute_fn() {
            Ok(result) => {
                if result.exit_code == 0 {
                    return Ok(result);
                }
                last_error = Some(CoreError::node_failed(format!(
                    "command exited with code {}\nstderr: {}",
                    result.exit_code, result.stderr
                )));
            }
            Err(e) => {
                last_error = Some(e);
                // Exponential backoff capped at 60s
                let backoff_secs = 1u64.checked_shl(attempt).unwrap_or(60).min(60);
                std::thread::sleep(std::time::Duration::from_secs(backoff_secs));
            }
        }
    }

    Err(last_error.unwrap_or_else(|| CoreError::node_failed("execution failed")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Generate a unique temp file path for testing (no uuid dependency).
    fn temp_test_path() -> std::path::PathBuf {
        use std::time::{SystemTime, UNIX_EPOCH};
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("acpx-test-{ts}"))
    }

    #[test]
    fn test_load_script_inline() {
        let resolved = crate::core::schema::ResolvedScript::Inline("echo hello".to_string());
        assert_eq!(load_script(&resolved).unwrap(), "echo hello");
    }

    #[test]
    fn test_load_prompt_inline() {
        let resolved = crate::core::schema::ResolvedPrompt::Inline("review code".to_string());
        assert_eq!(load_prompt(&resolved).unwrap(), "review code");
    }

    #[test]
    fn test_load_script_file_not_found() {
        let resolved =
            crate::core::schema::ResolvedScript::File("/nonexistent/path.sh".to_string());
        assert!(load_script(&resolved).is_err());
    }

    #[test]
    fn test_build_env_inherits_process() {
        let node_env = HashMap::new();
        let env = build_env(&node_env);
        assert!(env.keys().any(|k| k.eq_ignore_ascii_case("PATH")));
    }

    #[test]
    fn test_build_env_merges_node_env() {
        let mut node_env = HashMap::new();
        node_env.insert("CUSTOM_VAR".to_string(), "custom_value".to_string());
        let env = build_env(&node_env);
        assert_eq!(env.get("CUSTOM_VAR").unwrap(), "custom_value");
    }

    #[test]
    fn test_truncate_short() {
        let s = "hello world";
        assert_eq!(truncate_for_storage(s), "hello world");
    }

    #[test]
    fn test_truncate_exact_limit() {
        let s: String = "a".repeat(MAX_STORED_OUTPUT);
        assert_eq!(truncate_for_storage(&s).len(), MAX_STORED_OUTPUT);
    }

    #[test]
    fn test_truncate_over_limit() {
        let s: String = "a".repeat(MAX_STORED_OUTPUT + 1000);
        let truncated = truncate_for_storage(&s);
        assert!(truncated.len() < s.len());
        assert!(truncated.contains("[truncated"));
    }

    #[test]
    fn test_truncate_multibyte_boundary() {
        let s: String = "你".repeat(MAX_STORED_OUTPUT / 3 + 100);
        let truncated = truncate_for_storage(&s);
        assert!(truncated.contains("[truncated"));
        let _ = truncated.chars().count(); // Verify valid UTF-8
    }

    #[test]
    fn test_max_stored_output_constant() {
        assert_eq!(MAX_STORED_OUTPUT, 256 * 1024);
    }

    #[test]
    fn test_parse_output_file_basic() {
        let path = temp_test_path();
        std::fs::write(&path, "workdir=./workspace/abc123\nstatus=ok\n").unwrap();
        let outputs = parse_output_file(path.to_str().unwrap());
        let _ = std::fs::remove_file(&path);
        assert_eq!(outputs.get("workdir").unwrap(), "./workspace/abc123");
        assert_eq!(outputs.get("status").unwrap(), "ok");
    }

    #[test]
    fn test_parse_output_file_empty() {
        let path = temp_test_path();
        std::fs::write(&path, "").unwrap();
        let outputs = parse_output_file(path.to_str().unwrap());
        let _ = std::fs::remove_file(&path);
        assert!(outputs.is_empty());
    }

    #[test]
    fn test_parse_output_file_missing() {
        let outputs = parse_output_file("/nonexistent/path");
        assert!(outputs.is_empty());
    }

    #[test]
    fn test_parse_output_file_skips_invalid_lines() {
        let path = temp_test_path();
        std::fs::write(&path, "valid=yes\nno_equals_line\n\nalso_valid=42\n").unwrap();
        let outputs = parse_output_file(path.to_str().unwrap());
        let _ = std::fs::remove_file(&path);
        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs.get("valid").unwrap(), "yes");
        assert_eq!(outputs.get("also_valid").unwrap(), "42");
    }

    #[test]
    fn test_parse_output_file_last_wins() {
        let path = temp_test_path();
        std::fs::write(&path, "key=first\nkey=second\n").unwrap();
        let outputs = parse_output_file(path.to_str().unwrap());
        let _ = std::fs::remove_file(&path);
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs.get("key").unwrap(), "second");
    }
}

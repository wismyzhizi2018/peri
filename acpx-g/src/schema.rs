use serde::Deserialize;
use std::collections::HashMap;

/// Parse a workflow from YAML string.
/// Validates that name, version are non-empty and nodes list is non-empty.
pub fn parse_workflow(yaml: &str) -> anyhow::Result<Workflow> {
    let wf: Workflow =
        serde_yaml::from_str(yaml).map_err(|e| anyhow::anyhow!("failed to parse workflow: {e}"))?;
    validate_workflow(&wf)?;
    Ok(wf)
}

/// Validate a parsed workflow has required fields and consistent structure.
pub fn validate_workflow(wf: &Workflow) -> anyhow::Result<()> {
    if wf.name.trim().is_empty() {
        anyhow::bail!("workflow name must not be empty");
    }
    if wf.version.trim().is_empty() {
        anyhow::bail!("workflow version must not be empty");
    }
    if wf.nodes.is_empty() {
        anyhow::bail!("workflow must have at least one node");
    }

    // Validate node IDs are non-empty, use safe characters, and nodes don't depend on themselves
    let mut seen_ids = std::collections::HashSet::new();
    for node in &wf.nodes {
        let id = match node {
            NodeDef::Shell(n) => &n.id,
            NodeDef::Agent(n) => &n.id,
            NodeDef::Reference(n) => &n.id,
        };
        if id.trim().is_empty() {
            anyhow::bail!("node id must not be empty");
        }
        // Node IDs must be safe for use in CSS selectors, file paths, and shell commands.
        // Allow: alphanumeric, hyphen, underscore, dot, forward slash (for reference-expanded IDs)
        if !id
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '/')
        {
            anyhow::bail!(
                "node id '{}' contains invalid characters (allowed: alphanumeric, '-', '_', '.', '/')",
                id
            );
        }
        if !seen_ids.insert(id.clone()) {
            anyhow::bail!("duplicate node id '{}'", id);
        }
        let depends = match node {
            NodeDef::Shell(n) => &n.depends,
            NodeDef::Agent(n) => &n.depends,
            NodeDef::Reference(n) => &n.depends,
        };
        if depends.iter().any(|d| d == id) {
            anyhow::bail!("node '{}' cannot depend on itself", id);
        }
        // Validate depends IDs also use safe characters
        for dep in depends.iter() {
            if !dep
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '/')
            {
                anyhow::bail!(
                    "dependency '{}' in node '{}' contains invalid characters",
                    dep,
                    id
                );
            }
        }
    }

    // Validate all depends reference existing node IDs
    for node in &wf.nodes {
        let id = match node {
            NodeDef::Shell(n) => &n.id,
            NodeDef::Agent(n) => &n.id,
            NodeDef::Reference(n) => &n.id,
        };
        let depends = match node {
            NodeDef::Shell(n) => &n.depends,
            NodeDef::Agent(n) => &n.depends,
            NodeDef::Reference(n) => &n.depends,
        };
        for dep in depends.iter() {
            if !seen_ids.contains(dep) {
                anyhow::bail!(
                    "node '{}' depends on '{}', which does not exist. Available nodes: {}",
                    id,
                    dep,
                    seen_ids.iter().cloned().collect::<Vec<_>>().join(", ")
                );
            }
        }
    }

    // Validate reference nodes have matching entries in references map
    for node in &wf.nodes {
        if let NodeDef::Reference(ref_node) = node {
            if !wf.references.contains_key(&ref_node.r#ref) {
                anyhow::bail!(
                    "reference node '{}' references '{}' which is not defined in the references section",
                    ref_node.id,
                    ref_node.r#ref
                );
            }
        }
    }

    Ok(())
}

// ─── Top-Level Workflow ───────────────────────────────────────────

/// 完整的 workflow 定义。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Workflow {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub version: String,

    #[serde(default)]
    pub defaults: NodeDefaults,

    /// 外部调用时的输入参数声明。
    #[serde(default)]
    pub inputs: HashMap<String, InputDef>,

    /// 全局环境变量。
    #[serde(default)]
    pub env: HashMap<String, String>,

    /// 引用的子 workflow alias → path/url。
    #[serde(default)]
    pub references: HashMap<String, String>,

    /// 工作流级超时（秒）。None 表示无限制。
    #[serde(default)]
    pub timeout: Option<u64>,

    /// 节点列表。
    pub nodes: Vec<NodeDef>,

    /// 引用外部 workflow 时，通过 with 传递参数
    #[serde(default)]
    pub with: serde_yaml::Value,

    /// Runtime-only: maps reference node ID prefix to the bound input values
    /// for that reference's child nodes. Populated by the loader during
    /// reference expansion. Not serialized in YAML.
    #[serde(skip)]
    pub reference_inputs: HashMap<String, HashMap<String, String>>,

    /// Runtime-only: maps reference node ID to its exit node IDs.
    /// Used to forward exit node outputs back to the reference node ID
    /// so downstream templates can reference `needs.<ref_id>.outputs.*`.
    /// Populated by the loader during reference expansion.
    #[serde(skip)]
    pub output_forward: HashMap<String, Vec<String>>,
}

// ─── Input Definition ─────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputDef {
    #[serde(rename = "type")]
    pub input_type: InputType,

    #[serde(default)]
    pub default: Option<String>,

    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputType {
    String,
    Number,
    Boolean,
}

// ─── Node Defaults ────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeDefaults {
    #[serde(default = "default_retry")]
    pub retry: u32,

    #[serde(default = "default_timeout")]
    pub timeout: u64,

    #[serde(default = "default_shell")]
    pub shell: String,
}

impl Default for NodeDefaults {
    fn default() -> Self {
        Self {
            retry: default_retry(),
            timeout: default_timeout(),
            shell: default_shell(),
        }
    }
}

fn default_retry() -> u32 {
    0
}
fn default_timeout() -> u64 {
    300
}
fn default_shell() -> String {
    "bash -c".into()
}

// ─── Node Definition ──────────────────────────────────────────────

/// 节点定义：根据 type 字段自动反序列化为对应变体。
///
/// ```yaml
/// # Shell 节点
/// - id: build
///   type: shell
///   run: "cargo build --release"
///
/// # Agent 节点
/// - id: review
///   type: agent
///   prompt: "Review the code"
///
/// # 引用节点
/// - id: call
///   type: reference
///   ref: notify
/// ```
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NodeDef {
    Shell(ShellNode),
    Agent(AgentNode),
    Reference(ReferenceNode),
}

// ─── Shell Node ───────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShellNode {
    pub id: String,

    /// 脚本来源：内联字符串 | 文件路径 | 平台区分。
    ///
    /// ```yaml
    /// run: "echo hello"                              # 内联
    /// run: { file: "./scripts/build.sh" }            # 单文件
    /// run: { linux: "./linux.sh", macos: "./mac.sh" } # 平台区分
    /// ```
    pub run: ScriptSource,

    /// 上游依赖的节点 id 列表。
    #[serde(default)]
    pub depends: Vec<String>,

    /// 声明的输出 key → 路径/值，供下游通过 needs.<id>.outputs.<key> 引用。
    #[serde(default)]
    pub outputs: HashMap<String, String>,

    /// 节点级环境变量（叠加到全局 env 之上）。
    #[serde(default)]
    pub env: HashMap<String, String>,

    /// 失败后是否继续执行下游。
    #[serde(default)]
    pub continue_on_error: bool,

    /// 执行配置（超时、重试、shell）。
    #[serde(flatten)]
    pub exec: ExecConfig,
}

// ─── Agent Node ───────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentNode {
    pub id: String,

    /// Prompt 来源：内联 | 文件 | 平台区分。
    pub prompt: PromptSource,

    /// Agent 子命令名称（peri / claude / codex 等），默认 "peri"。
    #[serde(default)]
    pub agent: Option<String>,

    /// Agent 模型。
    #[serde(default)]
    pub model: Option<String>,

    /// 工作目录。
    #[serde(default)]
    pub cwd: Option<String>,

    #[serde(default)]
    pub depends: Vec<String>,

    #[serde(default)]
    pub outputs: HashMap<String, String>,

    #[serde(default)]
    pub env: HashMap<String, String>,

    #[serde(default)]
    pub continue_on_error: bool,

    #[serde(flatten)]
    pub exec: ExecConfig,
}

// ─── Reference Node ───────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceNode {
    pub id: String,

    /// 对应顶层 references 中的 alias。
    pub r#ref: String,

    /// 传给子 workflow 的参数。
    #[serde(default)]
    pub with: serde_yaml::Value,

    #[serde(default)]
    pub depends: Vec<String>,

    #[serde(default)]
    pub outputs: HashMap<String, String>,

    #[serde(default)]
    pub continue_on_error: bool,

    #[serde(flatten)]
    pub exec: ExecConfig,
}

// ─── Script / Prompt Source ───────────────────────────────────────

/// 脚本来源：内联字符串 | { file } 文件引用 | 平台区分。
///
/// serde untagged 按顺序尝试：
/// 1. String → Inline
/// 2. 有 `file` 键 → File
/// 3. 有 `linux`/`macos`/`windows`/`default` 键 → Platform
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ScriptSource {
    Inline(String),
    File(FileSource),
    Platform(PlatformFiles),
}

/// { file: "./path/to/script.sh" }
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileSource {
    pub file: String,
}

/// Prompt 来源，与 ScriptSource 相同结构。
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum PromptSource {
    Inline(String),
    File(FileSource),
    Platform(PlatformFiles),
}

/// 按平台区分的文件/脚本路径。
///
/// ```yaml
/// run:
///   linux: "./scripts/deploy-linux.sh"
///   macos: "./scripts/deploy-macos.sh"
///   windows: "./scripts/deploy.ps1"
///   default: "./scripts/deploy.sh"
/// ```
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlatformFiles {
    #[serde(default)]
    pub linux: Option<String>,

    #[serde(default)]
    pub macos: Option<String>,

    #[serde(default)]
    pub windows: Option<String>,

    #[serde(default)]
    pub default: Option<String>,
}

// ─── Execution Config ─────────────────────────────────────────────

/// 节点级执行配置，通过 serde(flatten) 嵌入各节点类型。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecConfig {
    #[serde(default)]
    pub timeout: Option<u64>,

    #[serde(default)]
    pub retry: Option<u32>,

    /// 节点级 shell 覆盖。
    #[serde(default)]
    pub shell: Option<String>,

    /// 条件执行表达式。求值为 false 时跳过节点。
    /// 支持 `{{ inputs.x }}`/`{{ needs.id.outputs.key }}`/`{{ env.KEY }}` 插值，
    /// 以及 `==`/`!=` 比较运算符。
    #[serde(default, rename = "if")]
    pub r#if: Option<String>,
}

// ─── Platform Resolution ──────────────────────────────────────────

/// 当前运行时平台。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Linux,
    MacOs,
    Windows,
}

impl Platform {
    pub fn detect() -> Self {
        if cfg!(target_os = "linux") {
            Platform::Linux
        } else if cfg!(target_os = "macos") {
            Platform::MacOs
        } else if cfg!(target_os = "windows") {
            Platform::Windows
        } else {
            // 兜底：运行时检测
            match std::env::consts::OS {
                "linux" => Platform::Linux,
                "macos" => Platform::MacOs,
                "windows" => Platform::Windows,
                _ => Platform::Linux,
            }
        }
    }
}

impl ScriptSource {
    /// 根据当前平台解析出最终要执行的脚本内容或文件路径。
    pub fn resolve(&self, platform: Platform) -> anyhow::Result<ResolvedScript> {
        match self {
            ScriptSource::Inline(s) => Ok(ResolvedScript::Inline(s.clone())),
            ScriptSource::File(f) => Ok(ResolvedScript::File(f.file.clone())),
            ScriptSource::Platform(pf) => {
                let path = pf.resolve(platform)?;
                Ok(ResolvedScript::File(path))
            }
        }
    }
}

impl PromptSource {
    pub fn resolve(&self, platform: Platform) -> anyhow::Result<ResolvedPrompt> {
        match self {
            PromptSource::Inline(s) => Ok(ResolvedPrompt::Inline(s.clone())),
            PromptSource::File(f) => Ok(ResolvedPrompt::File(f.file.clone())),
            PromptSource::Platform(pf) => {
                let path = pf.resolve(platform)?;
                Ok(ResolvedPrompt::File(path))
            }
        }
    }
}

impl PlatformFiles {
    /// 按优先级匹配：当前 OS → default → 错误。
    pub fn resolve(&self, platform: Platform) -> anyhow::Result<String> {
        let key = match platform {
            Platform::Linux => &self.linux,
            Platform::MacOs => &self.macos,
            Platform::Windows => &self.windows,
        };

        if let Some(path) = key {
            return Ok(path.clone());
        }
        if let Some(path) = &self.default {
            return Ok(path.clone());
        }
        Err(anyhow::anyhow!(
            "no script defined for platform {:?} and no default fallback",
            platform
        ))
    }
}

/// 解析后的脚本。
#[derive(Debug, Clone)]
pub enum ResolvedScript {
    /// 直接可执行的 shell 字符串。
    Inline(String),
    /// 需要从文件系统读取的脚本路径。
    File(String),
}

/// 解析后的 prompt。
#[derive(Debug, Clone)]
pub enum ResolvedPrompt {
    Inline(String),
    File(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_files_resolve_matching() {
        let pf = PlatformFiles {
            linux: Some("./linux.sh".to_string()),
            macos: Some("./mac.sh".to_string()),
            windows: None,
            default: None,
        };
        assert_eq!(pf.resolve(Platform::Linux).unwrap(), "./linux.sh");
        assert_eq!(pf.resolve(Platform::MacOs).unwrap(), "./mac.sh");
    }

    #[test]
    fn test_platform_files_resolve_default_fallback() {
        let pf = PlatformFiles {
            linux: None,
            macos: None,
            windows: None,
            default: Some("./default.sh".to_string()),
        };
        assert_eq!(pf.resolve(Platform::Linux).unwrap(), "./default.sh");
    }

    #[test]
    fn test_platform_files_resolve_no_match_error() {
        let pf = PlatformFiles {
            linux: Some("./linux.sh".to_string()),
            macos: None,
            windows: None,
            default: None,
        };
        assert!(pf.resolve(Platform::Windows).is_err());
    }

    #[test]
    fn test_script_source_inline() {
        let src = ScriptSource::Inline("echo hello".to_string());
        let resolved = src.resolve(Platform::Linux).unwrap();
        match resolved {
            ResolvedScript::Inline(s) => assert_eq!(s, "echo hello"),
            _ => panic!("expected Inline"),
        }
    }

    #[test]
    fn test_script_source_file() {
        let src = ScriptSource::File(FileSource {
            file: "./script.sh".to_string(),
        });
        let resolved = src.resolve(Platform::MacOs).unwrap();
        match resolved {
            ResolvedScript::File(p) => assert_eq!(p, "./script.sh"),
            _ => panic!("expected File"),
        }
    }

    #[test]
    fn test_parse_workflow_valid() {
        let yaml = r#"
name: test
version: "1.0"
nodes:
  - id: greet
    type: shell
    run: echo hello
"#;
        let wf = parse_workflow(yaml).unwrap();
        assert_eq!(wf.name, "test");
        assert_eq!(wf.version, "1.0");
        assert_eq!(wf.nodes.len(), 1);
    }

    #[test]
    fn test_parse_workflow_empty_name() {
        let yaml = r#"
name: ""
version: "1.0"
nodes:
  - id: greet
    type: shell
    run: echo hello
"#;
        let err = parse_workflow(yaml).unwrap_err();
        assert!(err.to_string().contains("name must not be empty"));
    }

    #[test]
    fn test_parse_workflow_empty_version() {
        let yaml = r#"
name: test
version: ""
nodes:
  - id: greet
    type: shell
    run: echo hello
"#;
        let err = parse_workflow(yaml).unwrap_err();
        assert!(err.to_string().contains("version must not be empty"));
    }

    #[test]
    fn test_parse_workflow_no_nodes() {
        let yaml = r#"
name: test
version: "1.0"
nodes: []
"#;
        let err = parse_workflow(yaml).unwrap_err();
        assert!(err.to_string().contains("at least one node"));
    }

    #[test]
    fn test_parse_workflow_reference_missing_in_map() {
        let yaml = r#"
name: test
version: "1.0"
nodes:
  - id: call
    type: reference
    ref: nonexistent
"#;
        let err = parse_workflow(yaml).unwrap_err();
        assert!(err
            .to_string()
            .contains("not defined in the references section"));
    }

    #[test]
    fn test_parse_workflow_whitespace_name() {
        let yaml = r#"
name: "   "
version: "1.0"
nodes:
  - id: greet
    type: shell
    run: echo hello
"#;
        let err = parse_workflow(yaml).unwrap_err();
        assert!(err.to_string().contains("name must not be empty"));
    }

    #[test]
    fn test_parse_workflow_invalid_yaml() {
        let yaml = "not: valid: yaml: {{{";
        let err = parse_workflow(yaml).unwrap_err();
        assert!(err.to_string().contains("failed to parse"));
    }

    #[test]
    fn test_prompt_source_inline() {
        let src = PromptSource::Inline("review code".to_string());
        let resolved = src.resolve(Platform::Linux).unwrap();
        match resolved {
            ResolvedPrompt::Inline(s) => assert_eq!(s, "review code"),
            _ => panic!("expected Inline"),
        }
    }

    #[test]
    fn test_prompt_source_file() {
        let src = PromptSource::File(FileSource {
            file: "./prompt.txt".to_string(),
        });
        let resolved = src.resolve(Platform::Linux).unwrap();
        match resolved {
            ResolvedPrompt::File(p) => assert_eq!(p, "./prompt.txt"),
            _ => panic!("expected File"),
        }
    }

    #[test]
    fn test_script_source_platform_resolve() {
        let src = ScriptSource::Platform(PlatformFiles {
            linux: Some("./linux.sh".to_string()),
            macos: None,
            windows: None,
            default: None,
        });
        let resolved = src.resolve(Platform::Linux).unwrap();
        match resolved {
            ResolvedScript::File(p) => assert_eq!(p, "./linux.sh"),
            _ => panic!("expected File"),
        }
    }

    #[test]
    fn test_parse_workflow_self_dependency() {
        let yaml = r#"
name: test
version: "1.0"
nodes:
  - id: loop
    type: shell
    depends: [loop]
    run: echo hello
"#;
        let err = parse_workflow(yaml).unwrap_err();
        assert!(err.to_string().contains("cannot depend on itself"));
    }

    #[test]
    fn test_parse_workflow_multiple_nodes_valid_deps() {
        let yaml = r#"
name: test
version: "1.0"
nodes:
  - id: build
    type: shell
    run: echo build
  - id: test
    type: shell
    depends: [build]
    run: echo test
  - id: deploy
    type: shell
    depends: [test]
    run: echo deploy
"#;
        let wf = parse_workflow(yaml).unwrap();
        assert_eq!(wf.nodes.len(), 3);
    }

    #[test]
    fn test_parse_workflow_with_inputs() {
        let yaml = r#"
name: test
version: "1.0"
inputs:
  env:
    type: string
    default: staging
  count:
    type: number
    required: true
nodes:
  - id: greet
    type: shell
    run: echo {{ inputs.env }}
"#;
        let wf = parse_workflow(yaml).unwrap();
        assert!(wf.inputs.contains_key("env"));
        assert!(wf.inputs.contains_key("count"));
        assert!(wf.inputs["count"].required);
        assert_eq!(wf.inputs["env"].default.as_deref(), Some("staging"));
    }

    #[test]
    fn test_parse_workflow_with_env() {
        let yaml = r#"
name: test
version: "1.0"
env:
  FOO: bar
  BAZ: qux
nodes:
  - id: greet
    type: shell
    run: echo $FOO
"#;
        let wf = parse_workflow(yaml).unwrap();
        assert_eq!(wf.env.get("FOO").unwrap(), "bar");
        assert_eq!(wf.env.get("BAZ").unwrap(), "qux");
    }

    #[test]
    fn test_parse_workflow_defaults() {
        let yaml = r#"
name: test
version: "1.0"
defaults:
  retry: 3
  timeout: 600
nodes:
  - id: greet
    type: shell
    run: echo hello
"#;
        let wf = parse_workflow(yaml).unwrap();
        assert_eq!(wf.defaults.retry, 3);
        assert_eq!(wf.defaults.timeout, 600);
    }

    #[test]
    fn test_parse_workflow_empty_node_id() {
        let yaml = r#"
name: test
version: "1.0"
nodes:
  - id: ""
    type: shell
    run: echo hello
"#;
        let err = parse_workflow(yaml).unwrap_err();
        assert!(err.to_string().contains("node id must not be empty"));
    }

    #[test]
    fn test_parse_workflow_node_id_special_chars() {
        let yaml = r#"
name: test
version: "1.0"
nodes:
  - id: "build@node"
    type: shell
    run: echo hello
"#;
        let err = parse_workflow(yaml).unwrap_err();
        assert!(err.to_string().contains("invalid characters"));
    }

    #[test]
    fn test_parse_workflow_node_id_with_spaces() {
        let yaml = r#"
name: test
version: "1.0"
nodes:
  - id: "build step"
    type: shell
    run: echo hello
"#;
        let err = parse_workflow(yaml).unwrap_err();
        assert!(err.to_string().contains("invalid characters"));
    }

    #[test]
    fn test_parse_workflow_node_id_valid_chars() {
        // Hyphen, underscore, dot, and forward slash are allowed
        let yaml = r#"
name: test
version: "1.0"
nodes:
  - id: build-step_1
    type: shell
    run: echo hello
  - id: build.step
    type: shell
    run: echo hello
  - id: deploy/prod
    type: shell
    depends: [build-step_1, build.step]
    run: echo deploy
"#;
        let wf = parse_workflow(yaml).unwrap();
        assert_eq!(wf.nodes.len(), 3);
    }

    #[test]
    fn test_parse_workflow_depends_invalid_chars() {
        let yaml = r#"
name: test
version: "1.0"
nodes:
  - id: build
    type: shell
    run: echo hello
  - id: deploy
    type: shell
    depends: ["build@node"]
    run: echo deploy
"#;
        let err = parse_workflow(yaml).unwrap_err();
        assert!(err.to_string().contains("invalid characters"));
    }

    #[test]
    fn test_validate_workflow_long_name_accepted() {
        // Long names are currently allowed — verify this behavior is stable
        let yaml = format!(
            r#"
name: "{}"
version: "1.0"
nodes:
  - id: greet
    type: shell
    run: echo hello
"#,
            "x".repeat(256)
        );
        assert!(parse_workflow(&yaml).is_ok());
    }

    #[test]
    fn test_parse_workflow_node_if_condition() {
        let yaml = r#"
name: test
version: "1.0"
inputs:
  deploy:
    type: string
    default: "false"
nodes:
  - id: build
    type: shell
    run: echo build
  - id: deploy
    type: shell
    if: "{{ inputs.deploy }} == true"
    depends: [build]
    run: echo deploy
"#;
        let wf = parse_workflow(yaml).unwrap();
        assert_eq!(wf.nodes.len(), 2);
        match &wf.nodes[1] {
            NodeDef::Shell(n) => {
                assert_eq!(n.exec.r#if.as_deref(), Some("{{ inputs.deploy }} == true"))
            }
            _ => panic!("expected shell node"),
        }
    }

    #[test]
    fn test_parse_workflow_node_no_if() {
        let yaml = r#"
name: test
version: "1.0"
nodes:
  - id: build
    type: shell
    run: echo build
"#;
        let wf = parse_workflow(yaml).unwrap();
        match &wf.nodes[0] {
            NodeDef::Shell(n) => assert!(n.exec.r#if.is_none()),
            _ => panic!("expected shell node"),
        }
    }

    #[test]
    fn test_parse_workflow_agent_if_condition() {
        let yaml = r#"
name: test
version: "1.0"
nodes:
  - id: review
    type: agent
    if: "{{ env.REVIEW_ENABLED }}"
    prompt: review the code
"#;
        let wf = parse_workflow(yaml).unwrap();
        match &wf.nodes[0] {
            NodeDef::Agent(n) => {
                assert_eq!(n.exec.r#if.as_deref(), Some("{{ env.REVIEW_ENABLED }}"))
            }
            _ => panic!("expected agent node"),
        }
    }

    #[test]
    fn test_parse_workflow_duplicate_node_id() {
        let yaml = r#"
name: test
version: "1.0"
nodes:
  - id: build
    type: shell
    run: echo first
  - id: build
    type: shell
    run: echo second
"#;
        let err = parse_workflow(yaml).unwrap_err();
        assert!(err.to_string().contains("duplicate node id 'build'"));
    }

    #[test]
    fn test_parse_workflow_depends_nonexistent_node() {
        let yaml = r#"
name: test
version: "1.0"
nodes:
  - id: build
    type: shell
    run: echo build
  - id: deploy
    type: shell
    depends: [build, test]
    run: echo deploy
"#;
        let err = parse_workflow(yaml).unwrap_err();
        assert!(err.to_string().contains("does not exist"));
        assert!(err.to_string().contains("'test'"));
    }
}

//! YAML deserialization for workflow schemas.
//!
//! Uses an intermediate `WorkflowRaw` that mirrors the YAML structure
//! (including `serde_yaml::Value` for `with` fields), then converts
//! to the core `Workflow` type with `HashMap<String, String>`.

use std::collections::HashMap;

use serde::Deserialize;

use super::*;

// ─── Raw (YAML-native) types ──────────────────────────────────────
// These types exist ONLY for deserialization. They mirror the YAML
// structure and are converted to core types after parsing.

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkflowRaw {
    name: String,
    #[serde(default)]
    description: Option<String>,
    version: String,
    #[serde(default)]
    defaults: NodeDefaultsRaw,
    #[serde(default)]
    inputs: HashMap<String, InputDefRaw>,
    #[serde(default)]
    env: HashMap<String, String>,
    #[serde(default)]
    references: HashMap<String, String>,
    #[serde(default)]
    timeout: Option<u64>,
    nodes: Vec<NodeDefRaw>,
    #[serde(default)]
    with: serde_yaml::Value,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct NodeDefaultsRaw {
    #[serde(default = "default_retry")]
    retry: u32,
    #[serde(default = "default_timeout")]
    timeout: u64,
    #[serde(default = "default_shell")]
    shell: String,
}

impl Default for NodeDefaultsRaw {
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

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct InputDefRaw {
    #[serde(rename = "type")]
    input_type: InputTypeRaw,
    #[serde(default)]
    default: Option<String>,
    #[serde(default)]
    required: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
enum InputTypeRaw {
    String,
    Number,
    Boolean,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum NodeDefRaw {
    Shell(ShellNodeRaw),
    Agent(AgentNodeRaw),
    Reference(ReferenceNodeRaw),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ShellNodeRaw {
    id: String,
    run: ScriptSourceRaw,
    #[serde(default)]
    depends: Vec<String>,
    #[serde(default)]
    outputs: HashMap<String, String>,
    #[serde(default)]
    env: HashMap<String, String>,
    #[serde(default)]
    continue_on_error: bool,
    #[serde(flatten)]
    exec: ExecConfigRaw,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentNodeRaw {
    id: String,
    prompt: PromptSourceRaw,
    #[serde(default)]
    agent: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    depends: Vec<String>,
    #[serde(default)]
    outputs: HashMap<String, String>,
    #[serde(default)]
    env: HashMap<String, String>,
    #[serde(default)]
    continue_on_error: bool,
    #[serde(flatten)]
    exec: ExecConfigRaw,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReferenceNodeRaw {
    id: String,
    r#ref: String,
    #[serde(default)]
    with: serde_yaml::Value,
    #[serde(default)]
    depends: Vec<String>,
    #[serde(default)]
    outputs: HashMap<String, String>,
    #[serde(default)]
    continue_on_error: bool,
    #[serde(flatten)]
    exec: ExecConfigRaw,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExecConfigRaw {
    #[serde(default)]
    timeout: Option<u64>,
    #[serde(default)]
    retry: Option<u32>,
    #[serde(default)]
    shell: Option<String>,
    #[serde(default, rename = "if")]
    r#if: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum ScriptSourceRaw {
    Inline(String),
    File(FileSourceRaw),
    Platform(PlatformFilesRaw),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileSourceRaw {
    file: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct PlatformFilesRaw {
    #[serde(default)]
    linux: Option<String>,
    #[serde(default)]
    macos: Option<String>,
    #[serde(default)]
    windows: Option<String>,
    #[serde(default)]
    default: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum PromptSourceRaw {
    Inline(String),
    File(FileSourceRaw),
    Platform(PlatformFilesRaw),
}

// ─── Conversion helpers ────────────────────────────────────────────

fn convert_input_type(raw: InputTypeRaw) -> InputType {
    match raw {
        InputTypeRaw::String => InputType::String,
        InputTypeRaw::Number => InputType::Number,
        InputTypeRaw::Boolean => InputType::Boolean,
    }
}

fn convert_input_def(raw: InputDefRaw) -> InputDef {
    InputDef {
        input_type: convert_input_type(raw.input_type),
        default: raw.default,
        required: raw.required,
    }
}

fn convert_exec_config(raw: ExecConfigRaw) -> ExecConfig {
    ExecConfig {
        timeout: raw.timeout,
        retry: raw.retry,
        shell: raw.shell,
        r#if: raw.r#if,
    }
}

fn convert_script_source(raw: ScriptSourceRaw) -> ScriptSource {
    match raw {
        ScriptSourceRaw::Inline(s) => ScriptSource::Inline(s),
        ScriptSourceRaw::File(f) => ScriptSource::File(FileSource { file: f.file }),
        ScriptSourceRaw::Platform(p) => ScriptSource::Platform(PlatformFiles {
            linux: p.linux,
            macos: p.macos,
            windows: p.windows,
            default: p.default,
        }),
    }
}

fn convert_prompt_source(raw: PromptSourceRaw) -> PromptSource {
    match raw {
        PromptSourceRaw::Inline(s) => PromptSource::Inline(s),
        PromptSourceRaw::File(f) => PromptSource::File(FileSource { file: f.file }),
        PromptSourceRaw::Platform(p) => PromptSource::Platform(PlatformFiles {
            linux: p.linux,
            macos: p.macos,
            windows: p.windows,
            default: p.default,
        }),
    }
}

// ─── Conversion: Raw → Core ───────────────────────────────────────

impl From<WorkflowRaw> for Workflow {
    fn from(raw: WorkflowRaw) -> Self {
        Self {
            name: raw.name,
            description: raw.description,
            version: raw.version,
            defaults: NodeDefaults {
                retry: raw.defaults.retry,
                timeout: raw.defaults.timeout,
                shell: raw.defaults.shell,
            },
            inputs: raw
                .inputs
                .into_iter()
                .map(|(k, v)| (k, convert_input_def(v)))
                .collect(),
            env: raw.env,
            references: raw.references,
            timeout: raw.timeout,
            nodes: raw.nodes.into_iter().map(convert_node_def).collect(),
            with: with_value_to_map(&raw.with),
            reference_inputs: HashMap::new(),
            output_forward: HashMap::new(),
        }
    }
}

fn convert_node_def(raw: NodeDefRaw) -> NodeDef {
    match raw {
        NodeDefRaw::Shell(n) => NodeDef::Shell(ShellNode {
            id: n.id,
            run: convert_script_source(n.run),
            depends: n.depends,
            outputs: n.outputs,
            env: n.env,
            continue_on_error: n.continue_on_error,
            exec: convert_exec_config(n.exec),
        }),
        NodeDefRaw::Agent(n) => NodeDef::Agent(AgentNode {
            id: n.id,
            prompt: convert_prompt_source(n.prompt),
            agent: n.agent,
            model: n.model,
            cwd: n.cwd,
            depends: n.depends,
            outputs: n.outputs,
            env: n.env,
            continue_on_error: n.continue_on_error,
            exec: convert_exec_config(n.exec),
        }),
        NodeDefRaw::Reference(n) => NodeDef::Reference(ReferenceNode {
            id: n.id,
            r#ref: n.r#ref,
            with: with_value_to_map(&n.with),
            depends: n.depends,
            outputs: n.outputs,
            continue_on_error: n.continue_on_error,
            exec: convert_exec_config(n.exec),
        }),
    }
}

// ─── Public API ───────────────────────────────────────────────────

/// Parse a workflow from YAML string, validate, and return a core `Workflow`.
pub fn parse_workflow(yaml: &str) -> anyhow::Result<Workflow> {
    let raw: WorkflowRaw =
        serde_yaml::from_str(yaml).map_err(|e| anyhow::anyhow!("failed to parse workflow: {e}"))?;
    let wf = Workflow::from(raw);
    validate_workflow(&wf)?;
    Ok(wf)
}

/// Convert `serde_yaml::Value` to `HashMap<String, String>`.
pub fn with_value_to_map(value: &serde_yaml::Value) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let serde_yaml::Value::Mapping(m) = value {
        for (k, v) in m {
            if let serde_yaml::Value::String(key) = k {
                let val = match v {
                    serde_yaml::Value::String(s) => s.clone(),
                    serde_yaml::Value::Number(n) => n.to_string(),
                    serde_yaml::Value::Bool(b) => b.to_string(),
                    serde_yaml::Value::Null => String::new(),
                    other => serde_yaml::to_string(other)
                        .unwrap_or_default()
                        .trim()
                        .to_string(),
                };
                map.insert(key.clone(), val);
            }
        }
    }
    map
}

// ─── Tests ────────────────────────────────────────────────────────

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
    fn test_parse_workflow_reference_valid() {
        let yaml = r#"
name: test
version: "1.0"
references:
  sub: ./sub.yaml
nodes:
  - id: call
    type: reference
    ref: sub
"#;
        let wf = parse_workflow(yaml).unwrap();
        assert_eq!(wf.name, "test");
        assert_eq!(wf.references["sub"], "./sub.yaml");
    }

    #[test]
    fn test_parse_workflow_reference_with_params() {
        let yaml = r#"
name: test
version: "1.0"
references:
  sub: ./sub.yaml
nodes:
  - id: step-1
    type: shell
    run: echo hello
  - id: call
    type: reference
    ref: sub
    depends: [step-1]
    with:
      name: world
      count: 3
"#;
        let wf = parse_workflow(yaml).unwrap();
        let node = wf
            .nodes
            .iter()
            .find(|n| matches!(n, NodeDef::Reference(r) if r.id == "call"))
            .unwrap();
        if let NodeDef::Reference(r) = node {
            assert_eq!(r.r#ref, "sub");
            assert_eq!(r.depends, vec!["step-1"]);
            assert_eq!(r.with["name"], "world");
            assert_eq!(r.with["count"], "3");
        } else {
            panic!("expected Reference node");
        }
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
    fn test_parse_workflow_partial_defaults() {
        let yaml = r#"
name: test
version: "1.0"
defaults:
  retry: 5
nodes:
  - id: greet
    type: shell
    run: echo hello
"#;
        let wf = parse_workflow(yaml).unwrap();
        assert_eq!(wf.defaults.retry, 5);
        assert_eq!(wf.defaults.timeout, 300);
        assert_eq!(wf.defaults.shell, "bash -c");
    }

    #[test]
    fn test_parse_workflow_per_node_exec_config() {
        let yaml = r#"
name: test
version: "1.0"
nodes:
  - id: fast
    type: shell
    run: echo fast
  - id: slow
    type: shell
    run: echo slow
    timeout: 120
    retry: 3
    shell: "zsh -c"
    continue_on_error: true
"#;
        let wf = parse_workflow(yaml).unwrap();
        assert_eq!(wf.nodes.len(), 2);
        match &wf.nodes[0] {
            NodeDef::Shell(n) => {
                assert!(n.exec.timeout.is_none());
                assert!(n.exec.retry.is_none());
                assert!(n.exec.shell.is_none());
                assert!(!n.continue_on_error);
            }
            _ => panic!("expected shell node"),
        }
        match &wf.nodes[1] {
            NodeDef::Shell(n) => {
                assert_eq!(n.exec.timeout, Some(120));
                assert_eq!(n.exec.retry, Some(3));
                assert_eq!(n.exec.shell.as_deref(), Some("zsh -c"));
                assert!(n.continue_on_error);
            }
            _ => panic!("expected shell node"),
        }
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
    fn test_parse_workflow_depends_string_instead_of_array() {
        let yaml = r#"
name: test
version: "1.0"
nodes:
  - id: build
    type: shell
    run: echo build
  - id: deploy
    type: shell
    depends: build
    run: echo deploy
"#;
        let result = parse_workflow(yaml);
        assert!(result.is_err(), "depends as bare string should fail");
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
    fn test_parse_workflow_agent_with_optional_fields() {
        let yaml = r#"
name: test
version: "1.0"
nodes:
  - id: review
    type: agent
    prompt: review the code
    agent: claude
    model: sonnet
    cwd: /tmp/project
"#;
        let wf = parse_workflow(yaml).unwrap();
        match &wf.nodes[0] {
            NodeDef::Agent(n) => {
                assert_eq!(n.agent.as_deref(), Some("claude"));
                assert_eq!(n.model.as_deref(), Some("sonnet"));
                assert_eq!(n.cwd.as_deref(), Some("/tmp/project"));
            }
            _ => panic!("expected agent node"),
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

    #[test]
    fn test_with_value_to_map() {
        let yaml = r###"
channel: "#deploy"
message: "Build done"
level: "info"
"###;
        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let map = with_value_to_map(&value);
        assert_eq!(map.get("channel").unwrap(), "#deploy");
        assert_eq!(map.get("message").unwrap(), "Build done");
        assert_eq!(map.get("level").unwrap(), "info");
    }

    #[test]
    fn test_with_value_to_map_empty() {
        let map = with_value_to_map(&serde_yaml::Value::Null);
        assert!(map.is_empty());
    }

    #[test]
    fn test_with_value_to_map_number_and_bool() {
        let yaml = "count: 42\nflag: true";
        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let map = with_value_to_map(&value);
        assert_eq!(map.get("count").unwrap(), "42");
        assert_eq!(map.get("flag").unwrap(), "true");
    }
}

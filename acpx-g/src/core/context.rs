//! Template context builder — merges inputs, env, and outputs for node execution.

use std::collections::HashMap;

use crate::core::schema::{get_node_env, node_id, NodeDef};
use crate::core::template::{interpolate_map, TemplateContext};

/// Build the template context for a node.
pub fn build_template_context(
    node: &NodeDef,
    root_inputs: &HashMap<String, String>,
    reference_inputs: &HashMap<String, HashMap<String, String>>,
    global_env: &HashMap<String, String>,
    completed_outputs: &HashMap<String, HashMap<String, String>>,
) -> TemplateContext {
    let nid = node_id(node);

    // Determine effective inputs: if node ID has a prefix (e.g. "do-build/checkout"),
    // look up reference_inputs for that prefix.
    let effective_inputs = if let Some(slash_pos) = nid.find('/') {
        let prefix = &nid[..slash_pos];
        reference_inputs
            .get(prefix)
            .cloned()
            .unwrap_or_else(|| root_inputs.clone())
    } else {
        root_inputs.clone()
    };

    // Build env: start with global, then interpolate and merge node env
    let node_env = get_node_env(node);
    let mut env = global_env.clone();
    // Interpolate node env with global-only context first (avoid circularity)
    let pre_ctx = TemplateContext {
        inputs: effective_inputs.clone(),
        needs_outputs: completed_outputs.clone(),
        env: global_env.clone(),
    };
    let resolved_node_env = interpolate_map(node_env, &pre_ctx);
    env.extend(resolved_node_env);

    TemplateContext {
        inputs: effective_inputs,
        needs_outputs: completed_outputs.clone(),
        env,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::schema::{ExecConfig, ScriptSource, ShellNode};

    fn make_shell_node(id: &str, depends: Vec<String>, env: HashMap<String, String>) -> NodeDef {
        NodeDef::Shell(ShellNode {
            id: id.to_string(),
            run: ScriptSource::Inline("echo".to_string()),
            depends,
            outputs: Default::default(),
            env,
            continue_on_error: false,
            exec: ExecConfig {
                timeout: None,
                retry: None,
                shell: None,
                r#if: None,
            },
        })
    }

    #[test]
    fn test_build_template_context_root_inputs() {
        let node = make_shell_node("build", vec![], HashMap::new());
        let mut root_inputs = HashMap::new();
        root_inputs.insert("env".to_string(), "production".to_string());
        let reference_inputs = HashMap::new();
        let global_env = HashMap::new();
        let completed_outputs = HashMap::new();

        let ctx = build_template_context(
            &node,
            &root_inputs,
            &reference_inputs,
            &global_env,
            &completed_outputs,
        );
        assert_eq!(ctx.inputs.get("env").unwrap(), "production");
    }

    #[test]
    fn test_build_template_context_prefixed_inputs() {
        let node = make_shell_node("do-build/checkout", vec![], HashMap::new());
        let mut root_inputs = HashMap::new();
        root_inputs.insert("env".to_string(), "production".to_string());

        let mut ref_inputs = HashMap::new();
        let mut build_inputs = HashMap::new();
        build_inputs.insert("repo".to_string(), "myrepo".to_string());
        ref_inputs.insert("do-build".to_string(), build_inputs);

        let global_env = HashMap::new();
        let completed_outputs = HashMap::new();

        let ctx = build_template_context(
            &node,
            &root_inputs,
            &ref_inputs,
            &global_env,
            &completed_outputs,
        );
        assert_eq!(ctx.inputs.get("repo").unwrap(), "myrepo");
        assert!(!ctx.inputs.contains_key("env"));
    }

    #[test]
    fn test_build_template_context_node_env() {
        let mut env = HashMap::new();
        env.insert("KEY".to_string(), "VAL".to_string());
        let node = make_shell_node("x", vec![], env);
        let ctx = build_template_context(
            &node,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
        );
        assert_eq!(ctx.env.get("KEY").unwrap(), "VAL");
    }

    #[test]
    fn test_build_template_context_node_env_interpolation() {
        let mut node_env = HashMap::new();
        node_env.insert("DEPLOY_ENV".to_string(), "{{ inputs.env }}".to_string());
        let node = make_shell_node("x", vec![], node_env);
        let mut root_inputs = HashMap::new();
        root_inputs.insert("env".to_string(), "production".to_string());
        let ctx = build_template_context(
            &node,
            &root_inputs,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
        );
        assert_eq!(ctx.env.get("DEPLOY_ENV").unwrap(), "production");
    }
}

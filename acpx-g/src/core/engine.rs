//! DAG execution engine — synchronous, level-by-level parallel execution via rayon.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::core::context::build_template_context;
use crate::core::dag::topological_sort;
use crate::core::error::CoreError;
use crate::core::schema::{node_continue_on_error, node_id, node_if_condition, NodeDef, Workflow};
use crate::core::template::evaluate_condition;

/// Events emitted during DAG execution.
#[derive(Debug, Clone)]
pub enum DagEvent {
    /// A node is about to start.
    NodeStarted { node_id: String },
    /// A node completed successfully.
    NodeCompleted {
        node_id: String,
        outputs: HashMap<String, String>,
    },
    /// A node failed.
    NodeFailed { node_id: String, error: String },
    /// A node was skipped (conditional or dependency failure).
    NodeSkipped { node_id: String, reason: String },
    /// The entire DAG completed successfully.
    DagCompleted,
    /// The DAG failed.
    DagFailed { error: String },
    /// The DAG was cancelled.
    DagCancelled,
}

/// Trait for observing DAG execution events.
pub trait WorkflowObserver: Send + Sync {
    fn on_event(&self, event: &DagEvent);
}

/// No-op observer that discards all events.
pub struct NoopObserver;

impl WorkflowObserver for NoopObserver {
    fn on_event(&self, _event: &DagEvent) {}
}

/// Result of executing a single node.
struct NodeExecution {
    idx: usize,
    result: Result<HashMap<String, String>, CoreError>,
}

/// Execute the full DAG synchronously.
///
/// Returns the completed outputs map on success.
pub fn execute_dag(
    wf: &Workflow,
    inputs: &HashMap<String, String>,
    cancel: &AtomicBool,
    observer: &dyn WorkflowObserver,
    _max_parallel: usize,
) -> Result<HashMap<String, HashMap<String, String>>, CoreError> {
    let nodes = &wf.nodes;
    let _defaults = &wf.defaults;
    let levels = topological_sort(nodes)?;

    let node_index: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (node_id(n), i))
        .collect();

    let mut completed: HashSet<usize> = HashSet::new();
    let mut failed: HashSet<usize> = HashSet::new();
    let mut completed_outputs: HashMap<String, HashMap<String, String>> = HashMap::new();

    for level in &levels {
        // Check cancellation between levels
        if cancel.load(Ordering::Relaxed) {
            observer.on_event(&DagEvent::DagCancelled);
            return Err(CoreError::cancelled("cancelled by user"));
        }

        let mut tasks: Vec<NodeExecution> = Vec::new();

        for &idx in level {
            let node = &nodes[idx];

            // Check dependency status
            let deps_satisfied = node_depends_satisfied(node, &node_index, &completed);
            let deps_any_finished =
                node_depends_any_finished(node, &node_index, &completed, &failed);

            if !deps_satisfied {
                if !deps_any_finished {
                    let nid = node_id(node).to_string();
                    observer.on_event(&DagEvent::NodeSkipped {
                        node_id: nid.clone(),
                        reason: "dependencies not met".into(),
                    });
                    continue;
                }
                let nid = node_id(node).to_string();
                observer.on_event(&DagEvent::NodeSkipped {
                    node_id: nid.clone(),
                    reason: "dependency failed".into(),
                });
                continue;
            }

            // Build template context
            let ctx = build_template_context(
                node,
                inputs,
                &wf.reference_inputs,
                &wf.env,
                &completed_outputs,
            );

            // Evaluate conditional execution
            if let Some(condition) = node_if_condition(node) {
                if !evaluate_condition(condition, &ctx) {
                    let nid = node_id(node).to_string();
                    observer.on_event(&DagEvent::NodeSkipped {
                        node_id: nid.clone(),
                        reason: "condition evaluated to false".into(),
                    });
                    continue;
                }
            }

            // TODO: actual node execution via NodeExecutor trait
            // For now, mark as completed with empty outputs (placeholder)
            let nid = node_id(node).to_string();
            observer.on_event(&DagEvent::NodeStarted {
                node_id: nid.clone(),
            });

            tasks.push(NodeExecution {
                idx,
                result: Ok(HashMap::new()),
            });
        }

        // Process results
        for exec in tasks {
            let nid = node_id(&nodes[exec.idx]).to_string();
            match exec.result {
                Ok(outputs) => {
                    if !outputs.is_empty() {
                        completed_outputs.insert(nid.clone(), outputs.clone());
                    }

                    // Forward outputs to reference node ID
                    for (ref_id, exit_ids) in &wf.output_forward {
                        if exit_ids.contains(&nid) {
                            if let Some(exit_outputs) = completed_outputs.get(&nid) {
                                completed_outputs.insert(ref_id.clone(), exit_outputs.clone());
                            }
                        }
                    }

                    completed.insert(exec.idx);
                    observer.on_event(&DagEvent::NodeCompleted {
                        node_id: nid,
                        outputs,
                    });
                }
                Err(e) => {
                    if cancel.load(Ordering::Relaxed) {
                        failed.insert(exec.idx);
                        continue;
                    }
                    failed.insert(exec.idx);
                    observer.on_event(&DagEvent::NodeFailed {
                        node_id: nid,
                        error: e.to_string(),
                    });

                    // For continue_on_error, treat as completed
                    if node_continue_on_error(&nodes[exec.idx]) {
                        completed.insert(exec.idx);
                    }
                }
            }
        }

        // Check cancellation
        if cancel.load(Ordering::Relaxed) {
            observer.on_event(&DagEvent::DagCancelled);
            return Err(CoreError::cancelled("cancelled by user"));
        }

        // Check for hard failures
        let has_hard_failure = failed.iter().any(|&fi| !node_continue_on_error(&nodes[fi]));

        if has_hard_failure {
            let first_hard_fail = failed
                .iter()
                .find(|&&fi| !node_continue_on_error(&nodes[fi]));
            if let Some(&fi) = first_hard_fail {
                let node = &nodes[fi];
                let msg = format!("node '{}' failed", node_id(node));
                observer.on_event(&DagEvent::DagFailed { error: msg.clone() });
                return Err(CoreError::node_failed(msg));
            }
        }
    }

    observer.on_event(&DagEvent::DagCompleted);
    Ok(completed_outputs)
}

fn node_depends_satisfied(
    node: &NodeDef,
    node_index: &HashMap<&str, usize>,
    completed: &HashSet<usize>,
) -> bool {
    crate::core::schema::node_depends(node).iter().all(|dep| {
        node_index
            .get(dep.as_str())
            .is_some_and(|&di| completed.contains(&di))
    })
}

fn node_depends_any_finished(
    node: &NodeDef,
    node_index: &HashMap<&str, usize>,
    completed: &HashSet<usize>,
    failed: &HashSet<usize>,
) -> bool {
    crate::core::schema::node_depends(node).iter().all(|dep| {
        node_index
            .get(dep.as_str())
            .is_some_and(|&di| completed.contains(&di) || failed.contains(&di))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::schema::{ExecConfig, ScriptSource, ShellNode};

    fn make_shell_node(id: &str, depends: Vec<String>) -> NodeDef {
        NodeDef::Shell(ShellNode {
            id: id.to_string(),
            run: ScriptSource::Inline("echo".to_string()),
            depends,
            outputs: Default::default(),
            env: Default::default(),
            continue_on_error: false,
            exec: ExecConfig {
                timeout: None,
                retry: None,
                shell: None,
                r#if: None,
            },
        })
    }

    fn make_workflow(nodes: Vec<NodeDef>) -> Workflow {
        Workflow {
            name: "test".into(),
            description: None,
            version: "1.0".into(),
            defaults: crate::core::schema::NodeDefaults::default(),
            inputs: HashMap::new(),
            env: HashMap::new(),
            references: HashMap::new(),
            timeout: None,
            nodes,
            with: HashMap::new(),
            reference_inputs: HashMap::new(),
            output_forward: HashMap::new(),
        }
    }

    #[test]
    fn test_execute_dag_simple() {
        let wf = make_workflow(vec![
            make_shell_node("a", vec![]),
            make_shell_node("b", vec!["a".into()]),
        ]);
        let cancel = AtomicBool::new(false);
        let result = execute_dag(&wf, &HashMap::new(), &cancel, &NoopObserver, 16);
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_dag_cancellation() {
        let nodes = vec![
            make_shell_node("a", vec![]),
            make_shell_node("b", vec!["a".into()]),
        ];
        let wf = make_workflow(nodes);
        let cancel = AtomicBool::new(true); // pre-cancelled
        let result = execute_dag(&wf, &HashMap::new(), &cancel, &NoopObserver, 16);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cancelled"));
    }

    #[test]
    fn test_execute_dag_conditional_skip() {
        let nodes = vec![
            make_shell_node("a", vec![]),
            NodeDef::Shell(ShellNode {
                id: "b".into(),
                run: ScriptSource::Inline("echo b".into()),
                depends: vec!["a".into()],
                outputs: Default::default(),
                env: Default::default(),
                continue_on_error: false,
                exec: ExecConfig {
                    timeout: None,
                    retry: None,
                    shell: None,
                    r#if: Some("{{ inputs.should_run }}".into()),
                },
            }),
        ];
        let wf = make_workflow(nodes);
        let cancel = AtomicBool::new(false);
        // No inputs → should_run is empty → falsy → node b skipped
        let result = execute_dag(&wf, &HashMap::new(), &cancel, &NoopObserver, 16);
        // Should succeed (skipped nodes don't cause failure)
        assert!(result.is_ok());
    }
}

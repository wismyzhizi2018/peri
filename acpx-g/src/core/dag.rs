//! DAG scheduling — topological sort producing parallel execution levels.

use std::collections::{HashMap, VecDeque};

use crate::core::error::CoreError;
use crate::core::schema::{node_depends, node_id, NodeDef};

/// Topological sort returning levels of node indices that can run in parallel.
///
/// Uses Kahn's algorithm. Returns an error on cycles or unknown dependencies.
pub fn topological_sort(nodes: &[NodeDef]) -> Result<Vec<Vec<usize>>, CoreError> {
    let n = nodes.len();

    // Detect duplicate node IDs
    let mut seen_ids: HashMap<&str, usize> = HashMap::new();
    for (i, node) in nodes.iter().enumerate() {
        let id = node_id(node);
        if let Some(&prev) = seen_ids.get(id) {
            return Err(CoreError::validation(format!(
                "duplicate node id '{}' found at indices {} and {}",
                id, prev, i
            )));
        }
        seen_ids.insert(id, i);
    }

    let id_to_idx: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (node_id(n), i))
        .collect();

    let mut adj: Vec<Vec<usize>> = vec![vec![]; n];
    let mut in_degree = vec![0u32; n];

    for (i, node) in nodes.iter().enumerate() {
        for dep in node_depends(node) {
            let j = id_to_idx.get(dep.as_str()).ok_or_else(|| {
                CoreError::validation(format!(
                    "node '{}' depends on unknown node '{}'",
                    node_id(node),
                    dep
                ))
            })?;
            adj[*j].push(i);
            in_degree[i] += 1;
        }
    }

    let mut queue: VecDeque<usize> = (0..n).filter(|&i| in_degree[i] == 0).collect();
    let mut levels: Vec<Vec<usize>> = Vec::new();

    while !queue.is_empty() {
        let current_level: Vec<usize> = queue.drain(..).collect();
        levels.push(current_level.clone());

        let mut next_queue = VecDeque::new();
        for &node in &current_level {
            for &neighbor in &adj[node] {
                in_degree[neighbor] -= 1;
                if in_degree[neighbor] == 0 {
                    next_queue.push_back(neighbor);
                }
            }
        }
        queue = next_queue;
    }

    if levels.iter().map(|l| l.len()).sum::<usize>() != n {
        return Err(CoreError::validation("workflow contains a cycle"));
    }

    Ok(levels)
}

// ─── Reference helpers (used by loader) ──────────────────────────

/// Find exit nodes: nodes not depended upon by any other node.
pub fn find_exit_nodes(nodes: &[NodeDef]) -> Vec<String> {
    let mut depended: std::collections::HashSet<String> = std::collections::HashSet::new();
    for node in nodes {
        for dep in node_depends(node) {
            depended.insert(dep.clone());
        }
    }
    nodes
        .iter()
        .map(node_id)
        .filter(|id| !depended.contains(*id))
        .map(|s| s.to_string())
        .collect()
}

/// Prefix a node's ID with the given string.
pub fn prefix_id(node: &mut NodeDef, prefix: &str) {
    match node {
        NodeDef::Shell(n) => n.id = format!("{prefix}{}", n.id),
        NodeDef::Agent(n) => n.id = format!("{prefix}{}", n.id),
        NodeDef::Reference(n) => n.id = format!("{prefix}{}", n.id),
    }
}

/// Prefix all depends entries in a node.
pub fn prefix_depends(node: &mut NodeDef, prefix: &str) {
    let depends = match node {
        NodeDef::Shell(n) => &mut n.depends,
        NodeDef::Agent(n) => &mut n.depends,
        NodeDef::Reference(n) => &mut n.depends,
    };
    *depends = depends.iter().map(|d| format!("{prefix}{d}")).collect();
}

/// Add entries to a node's depends list.
pub fn add_depends(node: &mut NodeDef, deps: &[String]) {
    let depends = match node {
        NodeDef::Shell(n) => &mut n.depends,
        NodeDef::Agent(n) => &mut n.depends,
        NodeDef::Reference(n) => &mut n.depends,
    };
    depends.extend(deps.iter().cloned());
}

/// Replace ref node IDs with exit node IDs in a node's depends list.
pub fn rewire_depends(node: &mut NodeDef, replacements: &HashMap<String, Vec<String>>) {
    let current = match node {
        NodeDef::Shell(n) => &mut n.depends,
        NodeDef::Agent(n) => &mut n.depends,
        NodeDef::Reference(n) => &mut n.depends,
    };

    let mut new_depends = Vec::new();
    for dep in current.drain(..) {
        if let Some(exit_ids) = replacements.get(&dep) {
            new_depends.extend(exit_ids.iter().cloned());
        } else {
            new_depends.push(dep);
        }
    }
    *current = new_depends;
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

    #[test]
    fn test_topological_sort_simple() {
        let nodes = vec![
            make_shell_node("a", vec![]),
            make_shell_node("b", vec!["a".to_string()]),
            make_shell_node("c", vec!["b".to_string()]),
        ];
        let levels = topological_sort(&nodes).unwrap();
        assert_eq!(levels.len(), 3);
        assert_eq!(levels[0], vec![0]);
        assert_eq!(levels[1], vec![1]);
        assert_eq!(levels[2], vec![2]);
    }

    #[test]
    fn test_topological_sort_parallel() {
        let nodes = vec![
            make_shell_node("a", vec![]),
            make_shell_node("b", vec![]),
            make_shell_node("c", vec!["a".to_string(), "b".to_string()]),
        ];
        let levels = topological_sort(&nodes).unwrap();
        assert_eq!(levels.len(), 2);
        assert_eq!(levels[0].len(), 2);
        assert_eq!(levels[1], vec![2]);
    }

    #[test]
    fn test_topological_sort_cycle() {
        let nodes = vec![
            make_shell_node("a", vec!["b".to_string()]),
            make_shell_node("b", vec!["a".to_string()]),
        ];
        assert!(topological_sort(&nodes).is_err());
    }

    #[test]
    fn test_topological_sort_unknown_dep() {
        let nodes = vec![make_shell_node("a", vec!["nonexistent".to_string()])];
        assert!(topological_sort(&nodes).is_err());
    }

    #[test]
    fn test_topological_sort_duplicate_id() {
        let nodes = vec![make_shell_node("a", vec![]), make_shell_node("a", vec![])];
        let err = topological_sort(&nodes).unwrap_err();
        assert!(err.to_string().contains("duplicate node id 'a'"));
    }

    #[test]
    fn test_topological_sort_empty() {
        let levels = topological_sort(&[]).unwrap();
        assert!(levels.is_empty());
    }

    #[test]
    fn test_topological_sort_single() {
        let nodes = vec![make_shell_node("only", vec![])];
        let levels = topological_sort(&nodes).unwrap();
        assert_eq!(levels.len(), 1);
        assert_eq!(levels[0], vec![0]);
    }

    #[test]
    fn test_find_exit_nodes() {
        let nodes = vec![
            make_shell_node("a", vec![]),
            make_shell_node("b", vec!["a".to_string()]),
        ];
        let exits = find_exit_nodes(&nodes);
        assert_eq!(exits, vec!["b"]);
    }

    #[test]
    fn test_find_exit_nodes_parallel() {
        let nodes = vec![make_shell_node("x", vec![]), make_shell_node("y", vec![])];
        let mut exits = find_exit_nodes(&nodes);
        exits.sort();
        assert_eq!(exits, vec!["x", "y"]);
    }

    #[test]
    fn test_prefix_id() {
        let mut node = make_shell_node("build", vec![]);
        prefix_id(&mut node, "ci/");
        assert_eq!(node_id(&node), "ci/build");
    }

    #[test]
    fn test_rewire_depends() {
        let mut node = make_shell_node("deploy", vec!["ref1".to_string()]);
        let mut replacements = HashMap::new();
        replacements.insert("ref1".into(), vec!["ci/build".into(), "ci/test".into()]);
        rewire_depends(&mut node, &replacements);
        assert_eq!(node_depends(&node), &["ci/build", "ci/test"]);
    }

    #[test]
    fn test_rewire_depends_no_match() {
        let mut node = make_shell_node("x", vec!["a".to_string()]);
        rewire_depends(&mut node, &HashMap::new());
        assert_eq!(node_depends(&node), &["a"]);
    }
}

use super::*;
use crate::tools::ToolDefinition;

fn make_tool(name: &str, desc: &str) -> ToolDefinition {
    ToolDefinition {
        name: name.to_string(),
        description: desc.to_string(),
        parameters: serde_json::json!({"type": "object", "properties": {}}),
    }
}

#[test]
fn test_capture_shape_stable() {
    let system = "You are a helpful assistant.\n__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__\nDate: 2026-06-04";
    let tools = vec![make_tool("read", "read file"), make_tool("write", "write file")];
    let s1 = capture_shape(system, &tools, 1);
    let s2 = capture_shape(system, &tools, 2);
    assert_eq!(s1.system_hash, s2.system_hash);
    assert_eq!(s1.tools_hash, s2.tools_hash);
}

#[test]
fn test_capture_shape_ignores_dynamic_part() {
    let sys1 = "static content\n__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__\nDate: 2026-06-04";
    let sys2 = "static content\n__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__\nDate: 2026-06-05";
    let tools = vec![make_tool("read", "read file")];
    let s1 = capture_shape(sys1, &tools, 1);
    let s2 = capture_shape(sys2, &tools, 2);
    assert_eq!(s1.system_hash, s2.system_hash, "动态区域变化不应影响 system_hash");
}

#[test]
fn test_capture_shape_system_changed() {
    let sys1 = "You are helpful.\n__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__\ndynamic";
    let sys2 = "You are a coding agent.\n__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__\ndynamic";
    let tools = vec![make_tool("read", "read file")];
    let s1 = capture_shape(sys1, &tools, 1);
    let s2 = capture_shape(sys2, &tools, 2);
    assert_ne!(s1.system_hash, s2.system_hash);
}

#[test]
fn test_capture_shape_tools_reordered() {
    let system = "static";
    let tools_a = vec![make_tool("write", "write"), make_tool("read", "read")];
    let tools_b = vec![make_tool("read", "read"), make_tool("write", "write")];
    let s1 = capture_shape(system, &tools_a, 1);
    let s2 = capture_shape(system, &tools_b, 2);
    assert_eq!(s1.tools_hash, s2.tools_hash, "tools 顺序变化不应影响 hash");
    assert_eq!(s1.tool_names, s2.tool_names);
}

#[test]
fn test_compare_shape_no_change() {
    let system = "static";
    let tools = vec![make_tool("read", "read")];
    let s1 = capture_shape(system, &tools, 1);
    let s2 = capture_shape(system, &tools, 2);
    let reasons = compare_shape(&s1, &s2);
    assert!(reasons.is_empty());
}

#[test]
fn test_compare_shape_system_changed() {
    let tools = vec![make_tool("read", "read")];
    let s1 = capture_shape("system v1", &tools, 1);
    let s2 = capture_shape("system v2", &tools, 2);
    let reasons = compare_shape(&s1, &s2);
    assert_eq!(reasons.len(), 1);
    assert!(matches!(reasons[0], ChangeReason::SystemPromptChanged));
}

#[test]
fn test_compare_shape_tools_added() {
    let system = "static";
    let tools1 = vec![make_tool("read", "read")];
    let tools2 = vec![
        make_tool("read", "read"),
        make_tool("write", "write"),
    ];
    let s1 = capture_shape(system, &tools1, 1);
    let s2 = capture_shape(system, &tools2, 2);
    let reasons = compare_shape(&s1, &s2);
    assert_eq!(reasons.len(), 1);
    match &reasons[0] {
        ChangeReason::ToolsChanged { added, removed } => {
            assert_eq!(added, &vec!["write".to_string()]);
            assert!(removed.is_empty());
        }
        _ => panic!("期望 ToolsChanged"),
    }
}

#[test]
fn test_compare_shape_tools_removed() {
    let system = "static";
    let tools1 = vec![make_tool("read", "read"), make_tool("write", "write")];
    let tools2 = vec![make_tool("read", "read")];
    let s1 = capture_shape(system, &tools1, 1);
    let s2 = capture_shape(system, &tools2, 2);
    let reasons = compare_shape(&s1, &s2);
    assert_eq!(reasons.len(), 1);
    match &reasons[0] {
        ChangeReason::ToolsChanged { added, removed } => {
            assert!(added.is_empty());
            assert_eq!(removed, &vec!["write".to_string()]);
        }
        _ => panic!("期望 ToolsChanged"),
    }
}

#[test]
fn test_compare_shape_both_changed() {
    let tools = vec![make_tool("read", "read")];
    let s1 = capture_shape("system v1", &tools, 1);
    let s2 = capture_shape("system v2", &vec![make_tool("write", "write")], 2);
    let reasons = compare_shape(&s1, &s2);
    assert_eq!(reasons.len(), 2);
}

#[test]
fn test_build_diagnostics_first_turn() {
    let system = "static";
    let tools = vec![make_tool("read", "read")];
    let cur = capture_shape(system, &tools, 1);
    let diag = build_diagnostics(None, &cur, 500, 1000);
    assert!(!diag.prefix_changed);
    assert!(diag.change_reasons.is_empty());
    assert_eq!(diag.cache_hit_tokens, 500);
    assert_eq!(diag.cache_miss_tokens, 500);
    assert!((diag.hit_rate - 0.5).abs() < f64::EPSILON);
}

#[test]
fn test_build_diagnostics_with_changes() {
    let tools = vec![make_tool("read", "read")];
    let prev = capture_shape("system v1", &tools, 1);
    let cur = capture_shape("system v2", &tools, 2);
    let diag = build_diagnostics(Some(&prev), &cur, 100, 1000);
    assert!(diag.prefix_changed);
    assert_eq!(diag.change_reasons.len(), 1);
    assert!((diag.hit_rate - 0.1).abs() < f64::EPSILON);
}

#[test]
fn test_format_change_reasons_empty() {
    assert_eq!(format_change_reasons(&[]), "");
}

#[test]
fn test_format_change_reasons_system() {
    let reasons = vec![ChangeReason::SystemPromptChanged];
    assert_eq!(format_change_reasons(&reasons), "system");
}

#[test]
fn test_format_change_reasons_tools() {
    let reasons = vec![ChangeReason::ToolsChanged {
        added: vec!["write".to_string()],
        removed: vec!["delete".to_string()],
    }];
    assert_eq!(format_change_reasons(&reasons), "tools(+write -delete)");
}

#[test]
fn test_format_change_reasons_combined() {
    let reasons = vec![
        ChangeReason::SystemPromptChanged,
        ChangeReason::ToolsChanged {
            added: vec!["glob".to_string()],
            removed: vec![],
        },
    ];
    assert_eq!(format_change_reasons(&reasons), "system, tools(+glob)");
}

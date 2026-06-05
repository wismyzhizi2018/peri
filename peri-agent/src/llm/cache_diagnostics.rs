use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::tools::ToolDefinition;

const DYNAMIC_BOUNDARY: &str = "__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__";

/// 前缀快照 — Provider 无关，只关心客户端发送的内容。
/// 跨轮对比可诊断缓存失效原因。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PrefixShape {
    /// system prompt 静态段（边界标记之前）的 hash
    pub system_hash: u64,
    /// 排序后 tools JSON 的 hash
    pub tools_hash: u64,
    /// 本轮 tool 名称列表（用于对比变化原因）
    pub tool_names: Vec<String>,
    /// 轮次计数
    pub turn: u64,
}

/// 前缀变化原因
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ChangeReason {
    /// system prompt 静态段内容变化
    SystemPromptChanged,
    /// tools 列表变化（新增/移除）
    ToolsChanged {
        added: Vec<String>,
        removed: Vec<String>,
    },
}

/// 缓存诊断结果
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CacheDiagnosticsResult {
    /// 前缀是否变化
    pub prefix_changed: bool,
    /// 变化原因列表
    pub change_reasons: Vec<ChangeReason>,
    /// 缓存命中 token 数
    pub cache_hit_tokens: u32,
    /// 缓存未命中 token 数（新写入 + 未命中部分）
    pub cache_miss_tokens: u32,
    /// 缓存命中率
    pub hit_rate: f64,
}

/// 截取 system prompt 静态段（边界标记之前）。
/// 静态段 = 可被 provider 缓存的前缀部分。
fn static_system_segment(system: &str) -> &str {
    match system.find(DYNAMIC_BOUNDARY) {
        Some(idx) => &system[..idx],
        None => system,
    }
}

/// 按 name 字母序排序 tools，保证顺序无关。
fn sorted_tool_names(tools: &[ToolDefinition]) -> Vec<String> {
    let mut names: Vec<String> = tools.iter().map(|t| t.name.clone()).collect();
    names.sort();
    names
}

/// 对 tools 排序后序列化为 JSON 做 hash。
fn hash_tools(tools: &[ToolDefinition]) -> u64 {
    let mut sorted: Vec<&ToolDefinition> = tools.iter().collect();
    sorted.sort_by(|a, b| a.name.cmp(&b.name));
    let mut hasher = DefaultHasher::new();
    for t in &sorted {
        t.name.hash(&mut hasher);
        t.description.hash(&mut hasher);
        // serde_json::Value 不实现 Hash，用字符串表示
        let params_str = t.parameters.to_string();
        params_str.hash(&mut hasher);
    }
    hasher.finish()
}

/// 捕获当前轮的前缀快照。
///
/// 在 `react_adapter::generate_reasoning()` 中调用，
/// 此时 `system` 和 `tools` 均可直接获取。
pub fn capture_shape(system: &str, tools: &[ToolDefinition], turn: u64) -> PrefixShape {
    let static_seg = static_system_segment(system);
    let system_hash = {
        let mut hasher = DefaultHasher::new();
        static_seg.hash(&mut hasher);
        hasher.finish()
    };
    let tools_hash = hash_tools(tools);
    let tool_names = sorted_tool_names(tools);
    PrefixShape {
        system_hash,
        tools_hash,
        tool_names,
        turn,
    }
}

/// 对比两轮前缀快照，产出变化原因。
pub fn compare_shape(prev: &PrefixShape, cur: &PrefixShape) -> Vec<ChangeReason> {
    let mut reasons = Vec::new();
    if prev.system_hash != cur.system_hash {
        reasons.push(ChangeReason::SystemPromptChanged);
    }
    if prev.tools_hash != cur.tools_hash {
        let prev_set: std::collections::HashSet<&String> = prev.tool_names.iter().collect();
        let cur_set: std::collections::HashSet<&String> = cur.tool_names.iter().collect();
        let added: Vec<String> = cur_set
            .difference(&prev_set)
            .map(|s| (*s).clone())
            .collect();
        let removed: Vec<String> = prev_set
            .difference(&cur_set)
            .map(|s| (*s).clone())
            .collect();
        reasons.push(ChangeReason::ToolsChanged { added, removed });
    }
    reasons
}

/// 构建缓存诊断结果。
pub fn build_diagnostics(
    prev: Option<&PrefixShape>,
    cur: &PrefixShape,
    cache_hit_tokens: u32,
    input_tokens: u32,
) -> CacheDiagnosticsResult {
    let change_reasons = match prev {
        Some(prev) => compare_shape(prev, cur),
        None => Vec::new(), // 首轮无对比
    };
    let cache_miss_tokens = input_tokens.saturating_sub(cache_hit_tokens);
    let hit_rate = if input_tokens > 0 {
        cache_hit_tokens as f64 / input_tokens as f64
    } else {
        0.0
    };
    CacheDiagnosticsResult {
        prefix_changed: !change_reasons.is_empty(),
        change_reasons,
        cache_hit_tokens,
        cache_miss_tokens,
        hit_rate,
    }
}

/// 将变化原因列表格式化为可读字符串。
pub fn format_change_reasons(reasons: &[ChangeReason]) -> String {
    let parts: Vec<String> = reasons
        .iter()
        .map(|r| match r {
            ChangeReason::SystemPromptChanged => "system".to_string(),
            ChangeReason::ToolsChanged { added, removed } => {
                let mut parts = Vec::new();
                if !added.is_empty() {
                    parts.push(format!("+{}", added.join(",")));
                }
                if !removed.is_empty() {
                    parts.push(format!("-{}", removed.join(",")));
                }
                format!("tools({})", parts.join(" "))
            }
        })
        .collect();
    parts.join(", ")
}

#[cfg(test)]
#[path = "cache_diagnostics_test.rs"]
mod tests;

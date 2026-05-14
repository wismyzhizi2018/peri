//! 模型名称 → 邮箱映射表，用于 git commit Co-Authored-By trailer。
//!
//! 参照 claude-code 的域名方案，使用 `@claude-code-best.win` 构造虚拟邮箱。
//! GitHub 组织不支持 Co-Authored-By，因此使用自有域名。

/// 模型关键词匹配表。（匹配的关键词列表，邮箱地址）
const MODEL_EMAIL_MAP: &[(&[&str], &str)] = &[
    (&["claude"], "noreply@anthropic.com"),
    (
        &["gpt", "dall-e", "o1-", "o3-", "o4-"],
        "openai@claude-code-best.win",
    ),
    (&["gemini"], "google-gemini@claude-code-best.win"),
    (&["grok"], "xai-org@claude-code-best.win"),
    (&["glm"], "zai-org@claude-code-best.win"),
    (&["deepseek"], "deepseek-ai@claude-code-best.win"),
    (&["qwen"], "QwenLM@claude-code-best.win"),
    (&["minimax"], "MiniMax-AI@claude-code-best.win"),
    (&["mimo"], "XiaomiMiMo@claude-code-best.win"),
    (&["kimi"], "MoonshotAI@claude-code-best.win"),
];

/// 根据模型名称查找对应的 attribution 邮箱。
/// 匹配不区分大小写，匹配第一个命中的关键词条目。
/// 无匹配时回退到 Anthropic 邮箱。
pub fn get_attribution_email(model_name: &str) -> &str {
    let lower = model_name.to_lowercase();
    for (keywords, email) in MODEL_EMAIL_MAP {
        if keywords.iter().any(|kw| lower.contains(kw)) {
            return email;
        }
    }
    "noreply@anthropic.com"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claude() {
        assert_eq!(
            get_attribution_email("claude-sonnet-4-20250514"),
            "noreply@anthropic.com"
        );
    }

    #[test]
    fn test_gpt() {
        assert_eq!(
            get_attribution_email("gpt-4o"),
            "openai@claude-code-best.win"
        );
    }

    #[test]
    fn test_o_series() {
        assert_eq!(
            get_attribution_email("o4-mini"),
            "openai@claude-code-best.win"
        );
    }

    #[test]
    fn test_gemini() {
        assert_eq!(
            get_attribution_email("gemini-2.5-flash"),
            "google-gemini@claude-code-best.win"
        );
    }

    #[test]
    fn test_grok() {
        assert_eq!(
            get_attribution_email("grok-3"),
            "xai-org@claude-code-best.win"
        );
    }

    #[test]
    fn test_glm() {
        assert_eq!(
            get_attribution_email("glm-4-plus"),
            "zai-org@claude-code-best.win"
        );
    }

    #[test]
    fn test_deepseek() {
        assert_eq!(
            get_attribution_email("deepseek-v3"),
            "deepseek-ai@claude-code-best.win"
        );
    }

    #[test]
    fn test_qwen() {
        assert_eq!(
            get_attribution_email("qwen-max"),
            "QwenLM@claude-code-best.win"
        );
    }

    #[test]
    fn test_minimax() {
        assert_eq!(
            get_attribution_email("minimax-m1"),
            "MiniMax-AI@claude-code-best.win"
        );
    }

    #[test]
    fn test_mimo() {
        assert_eq!(
            get_attribution_email("mimo-v2"),
            "XiaomiMiMo@claude-code-best.win"
        );
    }

    #[test]
    fn test_kimi() {
        assert_eq!(
            get_attribution_email("kimi-k2"),
            "MoonshotAI@claude-code-best.win"
        );
    }

    #[test]
    fn test_case_insensitive() {
        assert_eq!(
            get_attribution_email("CLAUDE-3-OPUS"),
            "noreply@anthropic.com"
        );
        assert_eq!(
            get_attribution_email("GPT-4-TURBO"),
            "openai@claude-code-best.win"
        );
    }

    #[test]
    fn test_unknown_fallback() {
        assert_eq!(
            get_attribution_email("unknown-model-xyz"),
            "noreply@anthropic.com"
        );
    }

    #[test]
    fn test_dalle_matches_openai() {
        assert_eq!(
            get_attribution_email("dall-e-3"),
            "openai@claude-code-best.win"
        );
    }
}

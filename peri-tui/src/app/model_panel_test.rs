use super::*;
use crate::config::{AppConfig, ProviderConfig};

fn make_config() -> PeriConfig {
    PeriConfig {
        schema: None,
        config: AppConfig {
            active_alias: "opus".to_string(),
            active_provider_id: "test".to_string(),
            providers: vec![ProviderConfig {
                id: "test".to_string(),
                name: Some("TestProvider".to_string()),
                ..Default::default()
            }],
            thinking: Some(ThinkingConfig {
                enabled: false,
                budget_tokens: 8000,
                effort: "medium".to_string(),
                max_tokens: 32000,
            }),
            ..Default::default()
        },
    }
}

#[test]
fn test_from_config_defaults() {
    let cfg = make_config();
    let panel = ModelPanel::from_config(&cfg);
    assert_eq!(panel.active_tab, AliasTab::Opus);
    assert_eq!(panel.cursor(), ROW_OPUS);
    assert_eq!(panel.provider_name, "TestProvider");
    assert_eq!(panel.buf_thinking_effort, "medium");
}

#[test]
fn test_from_config_sonnet() {
    let mut cfg = make_config();
    cfg.config.active_alias = "sonnet".to_string();
    let panel = ModelPanel::from_config(&cfg);
    assert_eq!(panel.active_tab, AliasTab::Sonnet);
    assert_eq!(panel.cursor(), ROW_SONNET);
}

#[test]
fn test_move_cursor_clamp() {
    let cfg = make_config();
    let mut panel = ModelPanel::from_config(&cfg);
    assert_eq!(panel.cursor(), ROW_OPUS);
    panel.cursor += 1;
    assert_eq!(panel.cursor(), ROW_SONNET);
    panel.cursor += 1;
    assert_eq!(panel.cursor(), ROW_HAIKU);
    panel.cursor += 1;
    assert_eq!(panel.cursor(), ROW_MAX_TOKENS);
    panel.cursor += 1;
    assert_eq!(panel.cursor(), ROW_EFFORT);
    // 光标可遍历全部 5 行
    panel.cursor -= 1;
    assert_eq!(panel.cursor(), ROW_MAX_TOKENS);
    panel.cursor -= 1;
    assert_eq!(panel.cursor(), ROW_HAIKU);
    panel.cursor -= 1;
    assert_eq!(panel.cursor(), ROW_SONNET);
    panel.cursor -= 1;
    assert_eq!(panel.cursor(), ROW_OPUS);
    // raw cursor 无 clamp；handle_key 中由 if cursor < ROW_COUNT-1 控制边界
}

#[test]
fn test_cycle_effort() {
    let cfg = make_config();
    let mut panel = ModelPanel::from_config(&cfg);

    assert_eq!(panel.buf_thinking_effort, "medium");
    panel.cycle_effort(false);
    assert_eq!(panel.buf_thinking_effort, "high");
    panel.cycle_effort(false);
    assert_eq!(panel.buf_thinking_effort, "xhigh");
    panel.cycle_effort(false);
    assert_eq!(panel.buf_thinking_effort, "max");
    panel.cycle_effort(false);
    assert_eq!(panel.buf_thinking_effort, "low");
    panel.cycle_effort(false);
    assert_eq!(panel.buf_thinking_effort, "medium");

    panel.cycle_effort(true);
    assert_eq!(panel.buf_thinking_effort, "low");
    panel.cycle_effort(true);
    assert_eq!(panel.buf_thinking_effort, "max");
    panel.cycle_effort(true);
    assert_eq!(panel.buf_thinking_effort, "xhigh");
    panel.cycle_effort(true);
    assert_eq!(panel.buf_thinking_effort, "high");
}

#[test]
fn test_cycle_effort_works_from_any_row() {
    let cfg = make_config();
    let mut panel = ModelPanel::from_config(&cfg);
    assert_eq!(panel.cursor(), ROW_OPUS);
    panel.cycle_effort(false);
    assert_eq!(panel.buf_thinking_effort, "high");
}

#[test]
fn test_apply_to_config() {
    let cfg = make_config();
    let mut panel = ModelPanel::from_config(&cfg);
    panel.active_tab = AliasTab::Sonnet;
    panel.buf_thinking_effort = "high".to_string();

    let mut cfg2 = make_config();
    panel.apply_to_config(&mut cfg2);
    assert_eq!(cfg2.config.active_alias, "sonnet");
    assert!(cfg2.config.thinking.as_ref().unwrap().enabled);
    assert_eq!(cfg2.config.thinking.as_ref().unwrap().effort, "high");
}

#[test]
fn test_apply_to_config_creates_thinking_when_none() {
    let mut cfg = PeriConfig {
        schema: None,
        config: AppConfig {
            active_alias: "opus".to_string(),
            active_provider_id: "test".to_string(),
            providers: vec![ProviderConfig {
                id: "test".to_string(),
                ..Default::default()
            }],
            thinking: None,
            ..Default::default()
        },
    };
    let panel = ModelPanel::from_config(&cfg);
    panel.apply_to_config(&mut cfg);
    let t = cfg.config.thinking.as_ref().unwrap();
    assert!(t.enabled);
    assert_eq!(t.effort, "high");
}

#[test]
fn test_alias_tab_description() {
    assert_eq!(
        AliasTab::Opus.description(),
        "Most capable for complex work"
    );
    assert_eq!(
        AliasTab::Sonnet.description(),
        "Balanced performance and speed"
    );
    assert_eq!(AliasTab::Haiku.description(), "Fastest for quick answers");
}

#[test]
fn test_apply_to_config_switches_provider_when_current_unsupported() {
    // 场景：当前 provider 不支持某个 alias，需要切换到其他 provider
    let mut cfg = PeriConfig {
        schema: None,
        config: AppConfig {
            active_alias: "opus".to_string(),
            active_provider_id: "mimo".to_string(),
            providers: vec![
                ProviderConfig {
                    id: "mimo".to_string(),
                    name: Some("小米 Provider".to_string()),
                    provider_type: "anthropic".to_string(),
                    api_key: "key1".to_string(),
                    base_url: "https://xiaomimimo.com/anthropic".to_string(),
                    models: crate::config::ProviderModels {
                        opus: "mimo-v2.5-pro".to_string(),
                        sonnet: "mimo-v2.5-pro".to_string(),
                        haiku: "".to_string(), // ❌ 小米不支持 haiku
                    },
                    ..Default::default()
                },
                ProviderConfig {
                    id: "Zhipu".to_string(),
                    name: Some("智谱 Provider".to_string()),
                    provider_type: "anthropic".to_string(),
                    api_key: "key2".to_string(),
                    base_url: "https://open.bigmodel.cn/api/anthropic".to_string(),
                    models: crate::config::ProviderModels {
                        opus: "glm-5.2".to_string(),
                        sonnet: "glm-5.2".to_string(),
                        haiku: "glm-4.7".to_string(), // ✅ 智谱支持 haiku
                    },
                    ..Default::default()
                },
            ],
            thinking: Some(ThinkingConfig {
                enabled: true,
                budget_tokens: 8000,
                effort: "high".to_string(),
                max_tokens: 32000,
            }),
            ..Default::default()
        },
    };

    // 初始状态：active_provider_id = "mimo"
    assert_eq!(cfg.config.active_provider_id, "mimo");
    assert_eq!(cfg.config.active_alias, "opus");

    // 切换到 Haiku (glm-4.7)，因为 mimo 不支持 haiku，应自动切换到 Zhipu provider
    let mut panel = ModelPanel::from_config(&cfg);
    panel.active_tab = AliasTab::Haiku;
    panel.apply_to_config(&mut cfg);

    // 验证：provider 切换到了 Zhipu，alias 切换到了 haiku
    assert_eq!(cfg.config.active_provider_id, "Zhipu", "当前 provider 不支持 haiku 时应自动切换到 Zhipu");
    assert_eq!(cfg.config.active_alias, "haiku");

    // 再切换回 Opus，Zhipu 支持 opus，应保持 Zhipu provider（避免频繁切换）
    panel.active_tab = AliasTab::Opus;
    panel.apply_to_config(&mut cfg);
    assert_eq!(cfg.config.active_provider_id, "Zhipu", "当前 provider 已支持 opus，应保持不变");
    assert_eq!(cfg.config.active_alias, "opus");
}

# SessionStart 钩子缺少 resume/clear/compact 触发场景

**状态**：Partial
**优先级**：高
**创建日期**：2026-06-01

## 问题描述

SessionStart 钩子仅在 session 首次 prompt 时触发（`is_session_start == true`），但 Claude Code 官方行为中 SessionStart 支持 4 种 matcher：`startup`、`resume`、`clear`、`compact`。

## 当前行为 vs 预期行为

| 场景 | Claude Code | Peri | 差异 |
|------|------------|------|------|
| 新会话首次 prompt | 触发（matcher=`startup`） | 触发 | ✓ 一致 |
| 恢复历史会话（`-c`/`-r`） | 触发（matcher=`resume`） | 不触发 | ✗ 缺失 |
| `/clear` 后首次 prompt | 触发（matcher=`clear`） | 不触发 | ✗ 缺失 |
| compact 后 | 触发（matcher=`compact`） | 不触发 | ✗ 缺失 |

## 影响范围

用户在 `~/.claude/settings.json` 中配置的 SessionStart 钩子（如设置 agent 状态为 `idle`）在 resume/clear/compact 场景下不会被触发，导致状态指示器与实际不同步。

## 根因分析

- `peri-acp/src/session/executor.rs` 中 `hook_session_start` 仅依赖 `is_empty_history` 判断
- `HookMiddleware::before_agent` 中 `is_session_start` 为 bool 标记，不携带 matcher 信息
- `HookInput` 中 `source` 字段硬编码为 `"startup"`，无 resume/clear/compact 值

## 修复方向

1. `HookEvent::SessionStart` 增加 matcher 语义（通过 `HookInput.source` 传递）
2. executor 中识别 resume/clear/compact 场景并设置对应 matcher
3. 钩子脚本的 stdin JSON 中 `source` 字段应为 `startup`/`resume`/`clear`/`compact`

## 涉及文件

- `peri-middlewares/src/hooks/middleware.rs` — `before_agent` 中 SessionStart 触发逻辑
- `peri-acp/src/session/executor.rs` — `hook_session_start` 参数传递
- `peri-middlewares/src/hooks/types.rs` — `HookInput` 的 `source` 字段

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-06-01 | — | Open | agent | 创建 |
| 2026-06-25 | Open | Partial | agent | 打通 source 字段链路（startup），resume/clear/compact 信号源尚未接线 |

## 修复记录

### 修复 #1（2026-06-25）

- **操作人**：agent
- **用户原意**：SessionStart 钩子按 Claude Code 规范支持 4 种 matcher（startup/resume/clear/compact），用户配置的钩子能在不同会话启动场景下分别触发
- **修复内容**（已完成 startup 分支）：
  - `peri-middlewares/src/hooks/middleware.rs` — `HookMiddlewareConfig` 字段 `is_session_start: bool` 改为 `session_start_source: Option<String>`；`with_session_start` 签名改为接收 `source: Option<&str>`；`before_agent` 中按 `Some(source)` 触发并通过 `HookInput::session_start(..., source, ...)` 透传给钩子 stdin JSON
  - `peri-acp/src/agent/builder.rs` — `AcpAgentConfig.hook_session_start: bool` 改为 `hook_session_start_source: Option<String>`；构建逻辑用 `source.as_deref().filter(|_| i == 0)` 保持「仅首个 hook 组触发」语义
  - `peri-acp/src/session/executor.rs` — 按 `is_empty_history` 计算源：空历史 → `Some("startup")`，非空 → `None`
- **涉及 commit**：（待提交）
- **验证状态**：待验证

### 残留问题（Partial 原因）

| Matcher | 信号源位置 | 当前状态 |
|---------|-----------|---------|
| `startup` | executor `is_empty_history` | ✓ 已接线 |
| `resume` | TUI `-c`/`-r` 启动路径 | ✗ 未接线（TUI 启动时未区分新会话 vs 恢复会话） |
| `clear` | TUI `/clear` 命令路径 | ✗ 未接线（`/clear` 走 `app.new_thread()` 创建新 session，本质是 startup 而非 clear 语义） |
| `compact` | `CompactMiddleware` 触发后 | ✗ 未接线（CompactMiddleware 未通过 `session_start_source` 信号回传 compact matcher） |

修复 resume/clear/compact 需要在 TUI/stdio 的会话启动入口识别具体场景，并把信号通过 `AcpAgentConfig.hook_session_start_source` 传递到 HookMiddleware。当前链路已就绪，只缺上游信号源。

### 测试覆盖

- `test_before_agent_session_start_controlled_by_flag` — 验证 `Some("startup")` 触发、`None` 不触发
- `test_session_start_passes_source_matcher_to_hook` — 验证钩子 stdin JSON 的 `source` 字段为 `"startup"`

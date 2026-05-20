# LLM 返回 400 时消息区域闪烁清空回到空白页

**状态**：Open
**优先级**：高
**创建日期**：2026-05-20

## 问题描述

使用 DeepSeek 模型时，消息流式输出过程中 LLM 返回 400 错误，TUI 消息区域出现短暂闪烁后完全清空，回到初始空白页面状态。历史对话内容丢失，用户无法看到之前的消息。

## 症状详情

| 维度 | 表现 |
|------|------|
| 触发时机 | 消息流式输出过程中，LLM API 返回错误 |
| 闪烁表现 | 消息区域短暂闪烁一下 |
| 清空表现 | 整个消息区域被清空，回到初始空白/欢迎页面状态 |
| 历史消息 | 之前对话内容在界面上消失 |
| 恢复情况 | 不确定是否自动恢复 |

### 错误日志

```
▶ 2026-05-20T08:47:42.577Z  POST [anthropic]
   UPSTREAM: https://api.deepseek.com/anthropic/v1/messages
◀ 2026-05-20T08:47:42.693Z  [anthropic]  → 400  (114ms)
```

DeepSeek 通过 Anthropic 兼容端点调用，返回 HTTP 400。

## 根因分析

两个相互叠加的 bug：

### Bug 1（根因）：Compact 后 round_start_vm_idx=0 + LLM 失败 = 视图完全清空

`handle_compact_completed()`（`agent_compact.rs:71`）将 `round_start_vm_idx` 重置为 0。如果 compact 后下一次 LLM 调用在 StateSnapshot 到达之前就失败（如 400 错误），`handle_done()` 触发 `request_rebuild()` 时 `prefix_len=0`，`build_tail_vms()` 因 `has_snapshot_this_round=false` 跳过 reconcile 返回空 tail，`view_messages.drain(0..)` 完全清空。

用户的"闪烁"是 compact 的视觉闪烁（view 被替换为 compact summary），随后的"全清"是 400 错误后 prefix_len=0 的灾难性 drain。

### Bug 2（加剧因素）：Executor 不发送 Error 事件

`executor.rs` 在 `agent.execute()` 返回 Err 时只 log 不通知前端。TUI 只收到 `Done`（通过 `peri/agent_event_done`），`reconcile_already_done` 始终为 false，`handle_done()` 总是调用 `request_rebuild()`，且用户看不到任何错误信息。

### 修复计划

`docs/superpowers/plans/2026-05-20-fix-llm-error-view-clear.md`

## 复现条件

- **复现频率**：目前仅遇到一次，尚未确认稳定复现条件
- **触发步骤**：
  1. 使用 DeepSeek 模型（通过 Anthropic 兼容端点）
  2. 发送 prompt 进行对话
  3. LLM 返回 400 错误时触发
- **环境**：DeepSeek 模型，Anthropic 兼容端点 (`/anthropic/v1/messages`)

## 涉及文件

- `peri-tui/src/app/agent_ops/lifecycle.rs` — Agent 生命周期错误处理（Done/Error/Interrupted 状态下的 UI 更新）
- `peri-tui/src/app/agent_ops/acp_bridge.rs` — ACP 通知桥接，将 AcpNotification 转为 AgentEvent

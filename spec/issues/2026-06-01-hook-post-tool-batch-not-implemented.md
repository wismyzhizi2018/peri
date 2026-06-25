# PostToolBatch 钩子未实现

**状态**：Fixed
**优先级**：低
**创建日期**：2026-06-01

## 问题描述

Claude Code 有 `PostToolBatch` 钩子事件，在一批并行工具调用全部完成后、下次模型请求前触发（每个 batch 一次）。Peri 未实现此事件。

## 预期行为

- 当 LLM 在一次响应中返回多个工具调用时，这些工具并行执行
- 所有工具的 PostToolUse/PostToolUseFailure 都触发完毕后，触发一次 PostToolBatch
- PostToolBatch 可 block（停止 agentic 循环）

## 当前行为

- 每个工具单独触发 PostToolUse/PostToolUseFailure
- 无 batch 级别的事件

## 影响范围

用户无法在整批工具完成后执行聚合操作（如批量日志、状态同步等）。

## 修复方向

1. `peri-middlewares/src/hooks/types.rs` — `HookEvent` 已有 `Unknown(String)` 兜底，但需显式添加 `PostToolBatch` 变体
2. `peri-agent/src/agent/tool_dispatch.rs` — `dispatch_tools` 中，所有 tool_result 写入 state 后、返回前触发
3. `peri-middlewares/src/hooks/middleware.rs` — 新增 `after_tools_batch` 方法或在现有流程中插入触发点

## 涉及文件

- `peri-middlewares/src/hooks/types.rs` — 新增 `PostToolBatch` 变体
- `peri-agent/src/agent/tool_dispatch.rs` — 批量工具完成后的触发点
- `peri-middlewares/src/hooks/middleware.rs` — 事件触发逻辑

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-06-01 | — | Open | agent | 创建 |
| 2026-06-25 | Open | Fixed | agent | 实现完整的 PostToolBatch 事件链路 |

## 修复记录

### 修复 #1（2026-06-25）

- **操作人**：agent
- **用户原意**：在一批并行工具全部完成后、下次模型请求前，触发一次聚合钩子事件，供用户做批量日志、状态同步等操作
- **修复内容**：
  - `peri-middlewares/src/hooks/types.rs` — `HookEvent` 枚举新增 `PostToolBatch` 变体（含 Serialize/Deserialize 映射 `"PostToolBatch"`）
  - `peri-agent/src/middleware/trait.rs` — `Middleware` trait 新增 `after_tools_batch` 方法（默认 no-op，**不循环 after_tool**——后者已在每个工具完成时单独触发，PostToolBatch 是独立的批次级语义事件）
  - `peri-agent/src/middleware/chain.rs` — 新增 `run_after_tools_batch` 顺序执行所有中间件的 `after_tools_batch`
  - `peri-agent/src/agent/executor/tool_dispatch.rs` — `dispatch_tools` 在所有 tool_result 写入 state 后、检查 cancel/deferred_error 前调用 `run_after_tools_batch`；Cancel/deferred_error 路径也调用以保证钩子能观察完整批次
  - `peri-middlewares/src/hooks/middleware.rs` — `HookMiddleware` 覆盖 `after_tools_batch`，构造 `HookInput`（`tool_output` 为 batch summary JSON 数组：tool_name/tool_call_id/is_error），调用 `fire_event(PostToolBatch, ...)`；Block 返回 `MiddlewareError`，PreventContinuation 通过返回 Ok 让 LLM 下一轮看到 batch 结果后决定停止
- **涉及 commit**：（待提交）
- **验证状态**：待验证

### 测试覆盖

- `test_after_tools_batch_fires_post_tool_batch_hook` — 验证非空 batch 触发 PostToolBatch 事件
- `test_after_tools_batch_skipped_for_empty_batch` — 验证空 batch 不触发
- `test_after_tools_batch_block_returns_error` — 验证 Block action 转为 MiddlewareError

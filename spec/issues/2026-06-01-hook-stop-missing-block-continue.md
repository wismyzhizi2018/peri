# Stop 钩子缺少 block 继续工作语义

**状态**：Fixed
**优先级**：中
**创建日期**：2026-06-01

## 问题描述

Claude Code 中 Stop 钩子的 block 动作有特殊语义：将 reason 反馈给 Claude 让它继续工作（最多连续 8 次 block）。Peri 当前 Stop 钩子的 block 动作只是拒绝 agent 输出，不会让 agent 继续工作。

## 当前行为

```rust
// middleware.rs:522
let _action = self.fire_event(HookEvent::Stop, &input, None, None).await;
// action 被忽略，不影响后续流程
```

Stop 钩子的返回值被丢弃，无论 hook 返回 Allow/Block/PreventContinuation，都不会改变 agent 行为。

## 预期行为

| Hook 返回 | Claude Code 行为 | Peri 当前行为 |
|----------|-----------------|-------------|
| Allow | 正常结束 | 正常结束 ✓ |
| Block + reason | 将 reason 作为反馈注入，Claude 继续工作（最多连续 8 次） | 无效，正常结束 ✗ |
| PreventContinuation | 停止 | 无效，正常结束 ✗ |

## 修复方向

1. `after_agent` 中检查 Stop hook 返回的 action
2. 若为 `Block { reason }` 且连续 block 次数 < 8，将 reason 注入消息并让 agent 继续
3. 在 HookMiddleware 中维护 `stop_block_count: Arc<Mutex<u32>>` 计数器
4. 若连续 block 次数 >= 8，忽略 block 并正常结束

## 涉及文件

- `peri-middlewares/src/hooks/middleware.rs` — `after_agent` 中 Stop 触发和返回值处理

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-06-01 | — | Open | agent | 创建 |
| 2026-06-25 | Open | Fixed | agent | 打通 Stop 钩子 Block → 反馈 → agent 继续 全链路 |

## 修复记录

### 修复 #1（2026-06-25）

- **操作人**：agent
- **用户原意**：Stop 钩子返回 Block + reason 时，将 reason 作为反馈注入，Claude 继续工作（最多连续 8 次）
- **修复内容**：
  - `peri-agent/src/agent/react.rs` — `AgentOutput` 新增 `continue_feedback: Option<String>` 字段，作为 middleware → executor 的反馈通道
  - `peri-middlewares/src/hooks/middleware.rs` — `HookMiddleware` 新增 `stop_block_count: Arc<AtomicU32>` 字段（session 共享）和 `MAX_STOP_BLOCKS=8` 常量；`after_agent` 捕获 Stop hook action，`Block{reason}` 且 count < 8 时递增计数器并设置 `continue_feedback`，达到上限后重置计数器并返回 None；`Allow` 等其他 action 重置计数器；新增 `with_stop_block_count()` builder 方法
  - `peri-acp/src/agent/builder.rs` — `AcpAgentConfig` 新增 `hook_stop_block_count: Arc<AtomicU32>` 字段，传递给所有 hook group 的 HookMiddleware 实例共享
  - `peri-acp/src/session/executor.rs` — 在 `execute_prompt` 开头创建 per-prompt 计数器，将 `agent.execute()` 包装在 loop 中消费 `continue_feedback`：Some(reason) 时将 reason 作为新 user 消息内容重新调用 execute()
- **涉及 commit**：（待提交）
- **验证状态**：待验证

### 测试覆盖

- `test_after_agent_stop_block_sets_continue_feedback` — Block+reason 写入 continue_feedback，计数器递增
- `test_after_agent_stop_allow_resets_count` — Allow 重置计数器
- `test_after_agent_stop_block_limit_caps_at_8` — 达到 8 次上限后忽略 Block 并重置
- `test_stop_block_counter_shared_across_middleware_instances` — 多个 hook group 实例通过 Arc 共享计数器

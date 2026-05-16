# 并发 SubAgent 工具调用路由错误 + 背景色移除

**状态**：Open
**优先级**：中
**创建日期**：2026-05-16

## 问题描述

两个问题：

1. SubAgent 展开后内部工具调用渲染时使用了 `SUB_AGENT_BG` 背景色，用户希望去掉背景色，与父 Agent 工具调用的视觉效果一致。
2. 当父 Agent 在同一轮中并发调用多个普通 SubAgent 时，前面的 SubAgent 展开后看不到工具调用记录，只有最后一个 SubAgent 的 `recent_messages` 中保留了什么。

## 症状详情

| 现象 | 详情 |
|------|------|
| 背景色问题 | SubAgent 内部工具调用（ToolBlock）有背景色，用户期望无背景色 |
| 并发路由问题 | 并发 2+ 个普通 SubAgent 时，仅最后一个 SubAgentGroup 的 `recent_messages` 中有工具调用记录，其余为空 |
| 影响范围 | 普通 SubAgent（非 background），fork/dispatching 版本暂未确认 |

## 复现条件

- **复现频率**：必现（并发时）
- **触发步骤**：
  1. 启动 TUI
  2. 让父 Agent 在同一轮中并发调用 2 个 Agent 工具（不同的 subagent_type）
  3. SubAgent 全部完成后，展开各 SubAgentGroup
  4. 观察：只有最后一个完成的 SubAgent 内部有工具调用记录
- **环境**：任意模型

## 涉及文件

- `peri-tui/src/ui/message_render.rs:511` —— SubAgentGroup 内部消息渲染时的 `bg(theme::SUB_AGENT_BG)` 逻辑
- `peri-tui/src/app/message_pipeline.rs:475-496` —— `subagent_tool_start` 通过 `subagent_stack.last_mut()` 路由工具调用
- `peri-tui/src/app/message_pipeline.rs:250-268` —— `ToolEnd` 事件同样通过 `last_mut()` 更新 `recent_messages`
- `peri-tui/src/app/message_pipeline.rs:595-600` —— `in_subagent()` 仅检查栈顶 SubAgent 是否运行

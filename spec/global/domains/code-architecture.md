# 代码架构 领域

## 领域综述

代码架构领域记录影响整体项目结构的重大变更，包括 crate 增删、依赖关系重构等。

核心职责：

- Workspace crate 结构管理
- 废弃功能完整清理
- 依赖关系调整

## 核心流程

### Relay Server 移除流程

```
1. 删除 rust-relay-server crate 目录
2. Workspace Cargo.toml members 移除引用
3. TUI 中清理 20+ 文件的 Relay 集成:
   - 面板（RelayPanel）
   - 命令（/relay）
   - 事件转发（RelayMessage）
   - CLI 参数（--remote-control）
   - 配置类型（RemoteControlConfig）
4. App 结构体从 4 子结构体缩减为 3（去掉 RelayState）
5. 评估 MessageAdded 事件若仅被 Relay 使用则从核心框架移除
6. 旧配置文件中 remote_control 字段无需主动清理，serde 自然忽略
```

## 技术方案总结

| 维度 | 选型 |
|------|------|
| Workspace 结构 | 3 crate → 移除 relay 后维持 3+peri-widgets |
| 配置兼容 | serde 忽略旧字段，无需主动清理 |
| 遗留文件 | Dockerfile.relay 保留作历史记录 |

## Feature 附录

### feature_20260427_F001_relay-removal

**摘要:** 完整删除废弃的 Relay Server 远程控制功能及相关代码
**关键决策:**

- 整体删除 rust-relay-server crate（含 server/client feature 及 web 前端）
- 清理 TUI 中 20+ 文件的 Relay 集成
- App 结构体从 4 子结构体缩减为 3（去掉 RelayState）
- 评估 MessageAdded 事件若仅被 Relay 使用则从核心框架一并移除
- 旧配置文件中 remote_control 字段无需主动清理，serde 自然忽略
- workspace 从 4 crate 减为 3 crate
**归档:** [链接](../../archive/feature_20260427_F001_relay-removal/)
**归档日期:** 2026-04-30

---

## Issue 经验附录

### issue_2026-05-14-dead-code-unfinished-features-cleanup

**摘要:** 24 处 #[allow(dead_code/unused)] 抑制了真正的死代码和未完成功能
**状态:** Fixed
**归档日期:** 2026-05-15
**关键词:** 死代码, allow注解, 代码清理, 编译器警告
**问题本质:** 编译器零警告不代表代码健康。24 处 allow 注解中，1 处为真正的死代码（CaptureLLM 完整实现但未接入测试），多处为未完成功能的预留字段/方法。
**通用模式:** 定期审计 #[allow(dead_code/unused)] 注解可以发现未完成功能和技术债。allow 注解是技术债的信号标记——它压制了编译器检测但保留了问题。
**涉及文件:** peri-middlewares/src/subagent/tool_test.rs, peri-tui/src/app/message_pipeline.rs, peri-middlewares/src/tool_search/tool_index.rs, peri-tui/src/app/agent_comm.rs, peri-tui/src/prompt.rs
**CLAUDE.md 链接:** false

### issue_2026-05-14-test-separation-convention-debt

**摘要:** 89.8% 源文件内联测试违反规范，两轮分离后 152 个文件外部化
**状态:** Resolved
**归档日期:** 2026-05-15
**关键词:** 测试分离, include!, #[path], 模块可见性
**问题本质:** `#[path = "..._test.rs"]` 模式将测试模块定义到外部文件后，`use super::*;` 只能看到父模块的公有项（Rust 2018+ 同文件模块特殊规则失效），导致 async_trait、Arc 等私有 import 不可见。
**通用模式:** 外部测试文件有两种引用模式：(1) `#[path]` — 适用于纯公有 API 测试；(2) `include!` — 文本级包含，完全保留原始作用域语义，适用于需要访问私有导入的测试。选择标准：如果测试依赖父模块的私有 use 语句（如 async_trait、Arc、内部模块路径），用 `include!`；否则用 `#[path]`。
**涉及文件:** 152 个 *_test.rs 文件，152 个源文件
**CLAUDE.md 链接:** false

---

## 相关 Feature

- → [tui.md](./tui.md) — TUI App 结构体变更

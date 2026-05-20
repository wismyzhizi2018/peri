# Peri

> Rust 构建的高性能 AI Agent 框架。Agent 定义、Skills 系统、HITL 审批、MCP 协议、LSP 集成——Claude Code 兼容生态，开箱即用。

## 快速安装

```bash
curl -fsSL https://raw.githubusercontent.com/konghayao/peri/main/scripts/install.sh | bash
```

安装脚本将自动下载对应平台的预编译二进制到 `~/.peri/bin/`，并提示添加 PATH。

指定版本：

```bash
PERI_INSTALL_VERSION=agent-v1.17 curl -fsSL https://raw.githubusercontent.com/konghayao/peri/main/scripts/install.sh | bash
```

## 从源码构建

```bash
git clone https://github.com/konghayao/peri.git
cd peri
cargo build --release
```

## 核心特性

- **Claude Code 兼容生态** — 直接复用 `.claude/agents/`、`.claude/skills/`、`CLAUDE.md`、`AGENTS.md`，零迁移成本
- **ReAct 循环引擎** — 思考 → 工具调用 → 反馈，最多 500 轮自主推进
- **可插拔中间件链** — 17 个中间件按需组装：文件系统、终端命令、HITL、子 Agent、MCP、Cron、Skills、LSP、Hooks、Plugin
- **ACP 协议层** — Agent Client Protocol 实现，TUI 与 Agent 通过 MpscTransport/StdioTransport 解耦通信
- **交互式 TUI** — 终端内完整对话体验，多会话分屏、Markdown 渲染、Slash 命令面板
- **多 LLM 支持** — OpenAI / Anthropic / DeepSeek / GLM 及任意兼容接口，`/model` 随时切换，Thinking/推理模式可配
- **子 Agent 系统** — Fork / Background / Normal 三种模式，工具集可精确控制，最多 3 个并发
- **HITL 审批** — 敏感操作（文件写入、命令执行、网络请求）强制拦截，支持 Approve/Edit/Reject/Respond
- **上下文压缩** — 双重策略：Micro-compact（零 API 调用清除旧工具结果） + Full Compact（LLM 摘要压缩）
- **MCP 协议** — 接入外部 MCP 服务器，stdio / Streamable HTTP / OAuth 2.0 全支持
- **LSP 集成** — 10 种代码智能操作（跳转定义、查找引用、hover 类型信息等），`after_tool` 自动同步文件变更
- **Plugin 系统** — 兼容 Claude Code 插件生态，Hooks（14 种事件/4 种执行类型）
- **遥测集成** — Langfuse + OpenTelemetry OTLP，开箱即用
- **SQLite 持久化** — 会话历史、记忆管理，支持多线程并发

## 快速上手

```bash
peri                        # 启动 TUI（默认 YOLO，跳过审批）
peri -- -a                  # 启用 HITL 审批模式
```

在 TUI 中按 `Shift+Tab` 切换权限模式，按 `Alt+M` 切换模型。

## 环境变量

| 变量 | 说明 |
|------|------|
| `ANTHROPIC_API_KEY` | Anthropic API Key |
| `OPENAI_API_KEY` | OpenAI 兼容 API Key |
| `OPENAI_BASE_URL` | API Base URL |
| `OPENAI_MODEL` | 默认模型 |
| `YOLO_MODE` | `true` 跳过审批（默认），`false` 启用 HITL |
| `RUST_LOG` | 日志级别（默认 info） |
| `LANGFUSE_*` | Langfuse 追踪 |

环境变量也可通过 `~/.peri/settings.json` 的 `env` 字段配置。

## 架构

```
┌─────────────────────────────────────────────┐
│                    peri-tui                  │
│              TUI 前端 / ACP Client           │
├─────────────────────────────────────────────┤
│                    peri-acp                  │
│       ACP 服务层 / Agent Builder / Executor  │
├─────────────────────────────────────────────┤
│                peri-middlewares              │
│   17 中间件：FS / Terminal / HITL / SubAgent │
│     MCP / Skills / LSP / Hooks / Plugin ...  │
├──────────────────┬──────────────────────────┤
│   peri-agent     │  langfuse-client (遥测)   │
│  ReAct 核心引擎   │  peri-lsp (LSP 客户端)    │
│  LLM 适配/SQLite  │                          │
└──────────────────┴──────────────────────────┘
```

| Crate | 职责 |
|-------|------|
| `peri-agent` | ReAct 循环、Middleware trait、LLM 适配器、工具系统、SQLite 持久化、遥测 |
| `peri-middlewares` | 中间件集合：Filesystem、Terminal、HITL、SubAgent、Skills、Todo、Cron、MCP、Hooks、Plugin、LSP |
| `peri-acp` | ACP 服务层：Agent Client Protocol 实现，桥接 TUI/IDE 与 Agent |
| `peri-tui` | TUI 应用，通过 ACP 协议与 Agent 通信 |
| `peri-widgets` | Widget 组件库（14 组件），仅依赖 ratatui + pulldown-cmark |
| `langfuse-client` | Langfuse 遥测客户端 |
| `peri-lsp` | LSP 客户端库 |

## License

MIT

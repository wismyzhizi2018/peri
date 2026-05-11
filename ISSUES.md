# LSP 工具测试报告

## 测试环境

- **项目**: perihelion（纯 Rust 项目，无 TS/JS 文件）
- **LSP 配置**: `~/.peri/settings.json` 中**仍未配置** `config.lspServers`
- **已注册 LSP 服务器**: `typescript`（来源不明，非用户配置），无 `rust-analyzer`
- **测试时间**: 2026-05-11（第二轮回归）

## 测试结果

### 第一轮（修复前）

| # | 操作 | 测试文件 | 结果 | 说明 |
|---|------|---------|------|------|
| 1 | `diagnostics` | `react.rs` | **PASS** | 返回 "No diagnostics found"（空注册表，无需 LSP 服务器） |
| 2 | `goToDefinition` | `react.rs:9` | **FAIL** | `LSP 服务器 "all" 初始化失败: 所有 LSP 服务器启动失败: typescript` |
| 3 | `findReferences` | `react.rs:9` | **FAIL** | 同上 |
| 4 | `hover` | `react.rs:9` | **FAIL** | 同上 |
| 5 | `documentSymbol` | `react.rs` | **FAIL** | 同上 |
| 6 | `workspaceSymbol` | query=`ReActAgent` | **FAIL** | 同上 |
| 7 | `goToImplementation` | `mod.rs:10` | **FAIL** | 同上 |
| 8 | `prepareCallHierarchy` | `mod.rs:10` | **FAIL** | 同上 |
| 9 | `incomingCalls` | `mod.rs:10` | **FAIL** | 同上 |
| 10 | `outgoingCalls` | `mod.rs:10` | **FAIL** | 同上 |

### 第二轮（2026-05-11 回归）

| # | 操作 | 测试文件 | 结果 | 说明 |
|---|------|---------|------|------|
| 1 | `diagnostics` | `executor/mod.rs` | **PASS** | 返回 "No diagnostics found"（内存注册表，不走 LSP） |
| 2 | `workspaceSymbol` | query=`ReActAgent` | **PASS** | 返回空结果（无匹配），但操作本身成功执行 |
| 3 | `goToDefinition` | `executor/mod.rs:25` | **FAIL** | `无 LSP 服务器可处理文件: executor/mod.rs (扩展名: rs)` |
| 4 | `findReferences` | `executor/mod.rs:25` | **FAIL** | 同上 |
| 5 | `hover` | `executor/mod.rs:25` | **FAIL** | 同上 |
| 6 | `documentSymbol` | `executor/mod.rs` | **FAIL** | `LSP 服务器 "all" 初始化失败: 所有 LSP 服务器启动失败: typescript` |
| 7 | `goToImplementation` | `executor/mod.rs:25` | **FAIL** | `无 LSP 服务器可处理文件: executor/mod.rs (扩展名: rs)` |
| 8 | `prepareCallHierarchy` | `executor/mod.rs:25` | **FAIL** | 同上 |
| 9 | `incomingCalls` | `executor/mod.rs:25` | **FAIL** | 同上 |
| 10 | `outgoingCalls` | `executor/mod.rs:25` | **FAIL** | 同上 |

## 变化对比

与第一轮相比，错误信息发生了变化：

| 变化 | 第一轮 | 第二轮 |
|------|--------|--------|
| `workspaceSymbol` | FAIL（初始化失败） | **PASS**（成功执行，返回空） |
| 需要文件路径的操作 | FAIL（`所有 LSP 服务器启动失败: typescript`） | FAIL（`无 LSP 服务器可处理文件 (.rs)`） |
| `documentSymbol` | FAIL（初始化失败） | FAIL（初始化失败，同第一轮） |

**说明**：第二轮的错误信息更精确了——代码改进了错误处理，在 LSP 初始化失败前先检查是否有能处理该扩展名的服务器。但根本问题未变：没有配置 `rust-analyzer`。

## 问题分析

### 根本原因

`~/.peri/settings.json` 中**仍然没有 `lspServers` 配置**。`typescript` 服务器来源不明（非用户配置），启动失败。

### `diagnostics` 伪通过

此操作不走 LSP 服务器，直接读内存中的 `DiagnosticsRegistry`，空注册表返回空结果。

### `workspaceSymbol` 行为变化

此操作现在似乎能在没有 LSP 服务器的情况下执行（返回空结果），可能改为了空操作兜底。

### 需要修复的问题

1. **`rust-analyzer` 未配置**：项目是纯 Rust，必须在 `~/.peri/settings.json` 的 `config.lspServers` 中配置 rust-analyzer
2. **`typescript` 服务器来源不明**：非用户配置却被注册，需要排查注入路径
3. **`documentSymbol` 与其他文件操作错误信息不一致**：`documentSymbol` 仍报 "初始化失败"，而 `goToDefinition` 等报 "无 LSP 服务器可处理文件"——两者应在同一检查点返回一致错误

### 建议修复

在 `~/.peri/settings.json` 的 `config` 中添加：

```json
{
  "config": {
    "lspServers": {
      "rust-analyzer": {
        "name": "rust-analyzer",
        "command": "rust-analyzer",
        "args": ["--stdio"],
        "extensionToLanguage": {
          ".rs": "rust"
        }
      }
    }
  }
}
```

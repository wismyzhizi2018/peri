# [BUG] Write/Edit diff 视图宽度硬编码 80，长行被截断

**状态**: Open
**优先级**: P1
**模块**: tui
**创建时间**: 2026-06-24
**发现方式**: 用户报告

## 现象

Write/Edit 工具的内联 diff 视图（Ctrl+O 切换）中，长行在第 80 列被截断，内容不完整。

## 根因

两处调用 `render_diff` 时硬编码宽度为 80：

1. `peri-tui/src/ui/message_view/mod.rs:70` — `build_diff_lines`
   ```rust
   let mut lines = peri_widgets::diff::render_diff(&diff_input, 80, &peri_widgets::DarkTheme);
   ```

2. `peri-tui/src/app/message_pipeline/reconcile.rs:66` — `try_build_diff_lines`
   ```rust
   let lines = peri_widgets::diff::render_diff(&diff_input, 80, &peri_widgets::DarkTheme);
   ```

`render_diff` 本身接受任意 `width` 参数，函数签名：
```rust
pub fn render_diff(input: &DiffInput, width: usize, theme: &dyn Theme) -> Vec<Line<'static>>
```

## 影响

- 终端宽度 > 80 列时（绝大多数现代终端），diff 行在第 80 列被截断
- 影响 Write 和 Edit 两个工具的 diff 渲染
- 用户无法看到完整的代码变更内容

## 修复方向

将硬编码 `80` 替换为终端实际宽度，需要从调用链透传：

1. `build_diff_lines` — 调用方 `tool_block_with_id()`（mod.rs:748）需传入宽度
2. `try_build_diff_lines` — 调用方 reconcile pipeline（reconcile.rs:241）需传入宽度

注意：diff 渲染结果缓存在 `ToolBlock.diff_lines` 中，宽度变化时需要重新计算。
渲染线程的 `RenderEvent::Resize` 已有宽度变化通知机制，diff_lines 重建可复用此路径。

## 相关文件

- `peri-tui/src/ui/message_view/mod.rs` — `build_diff_lines` + `tool_block_with_id` 调用
- `peri-tui/src/app/message_pipeline/reconcile.rs` — `try_build_diff_lines` + reconcile 调用
- `peri-widgets/src/diff/renderer.rs` — `render_diff_impl` 实现（已支持任意宽度）
- `peri-widgets/src/diff/mod.rs` — `render_diff` 公开入口

## 关联 Issue

- `spec/issues/2026-06-06-edit-diff-view-rendering-not-elegant.md` — 现象 2 记录

## 验证标准

终端宽度变化后，diff 视图自动重新渲染适配新宽度——长行不再被截断在第 80 列。

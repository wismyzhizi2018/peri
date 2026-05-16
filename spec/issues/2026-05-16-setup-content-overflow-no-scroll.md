# Setup Wizard 内容溢出无滚动支持

**状态**：Open
**优先级**：低
**创建日期**：2026-05-16

## 问题描述

所有步骤都将内容渲染到 `Paragraph::new(Text::from(lines))` 中，不使用任何滚动组件。Edit 模式有 8 个字段行 + 确认行 + hint ≈ 14+ 行，加 Panel border ≈ 17+ 行。在小终端（如 < 20 行高度）上内容被截断，用户无法滚动查看全部字段。

## 症状详情

| 现象 | 详情 |
|------|------|
| 小终端截断 | 15 行终端下 Edit 模式后半部分字段不可见 |
| 无滚动条 | 没有 Scrollbar 或 Paragraph scroll 支持 |
| Done 页面也可能溢出 | 多 provider 时 Done 步骤的 alias 列表可能超出一屏 |

## 根因

所有渲染函数直接 `f.render_widget(Paragraph::new(...), inner)`，无 `scroll()` 配置。

## 期望

使用 `Paragraph::scroll((offset, 0))` 或 `Scrollbar` 支持内容溢出时显示滚动条并允许导航。

## 涉及文件

- `peri-tui/src/ui/main_ui/popups/setup_wizard.rs` —— 所有 `render_step_*` 函数

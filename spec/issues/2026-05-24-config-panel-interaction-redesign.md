# Config 面板交互混乱，需整体重新设计

**状态**：Open
**优先级**：中
**创建日期**：2026-05-24

## 问题描述

当前 `/config` 面板采用 Browse/Edit 两步式操作模式：用户必须先在 Browse 模式选中一行，按 Enter 进入 Edit 模式，才能修改值。Edit 模式中按键行为因字段类型不同而不同（Space 在布尔字段是切换开关，在文本字段是输入空格；Left/Right 在布尔字段是切换，在文本字段是移动光标），6 个字段（布尔、文本、单选）混在一起无分组，标签中英混杂（Autocompact、Compact 阈值、语言、Persona、Tone、Proactiveness），用户无法预测按键效果。用户明确不喜欢这个交互，需要重新设计。

## 症状详情

| 字段类型 | 交互行为 | 问题 |
|----------|----------|------|
| Autocompact（布尔） | Space/Left/Right 切换 ON/OFF | 与文本字段行为冲突 |
| CompactThreshold（数字文本） | 键盘输入数字，Backspace 删除 | 无校验提示，阈值范围 50-99 不直观 |
| Language（文本） | 自由输入，校验在保存时 | 不知道支持哪些语言，输入错误才报错 |
| Persona（自由文本） | 键盘输入 | 无说明，新用户不知道这是系统提示词覆盖 |
| Tone（自由文本） | 键盘输入 | 同上 |
| Proactiveness（三选一） | Space/Left/Right 切换 low/medium/high | 与布尔字段的切换行为相同但含义不同 |

**Browse 模式**：显示 6 个字段列表，`❯` 指示当前行，Enter 进 Edit，Esc 关闭面板。
**Edit 模式**：所有字段平铺在一个平面，Up/Down 切字段（循环），Esc 返回 Browse，Enter 保存并关闭面板。

核心操作问题：
1. **两步模式切换**：打开面板不能直接编辑，必须 Enter 切模式
2. **按键行为不一致**：同一个键（Space/Left/Right）在不同字段类型上行为完全不同，无法预测
3. **无分组无说明**：6 个字段平铺，没有分组标题，没有说明文字，标签中英混杂
4. **Enter 即保存关闭**：Edit 模式按 Enter 直接保存并关闭面板，无法逐字段确认

## 期望改进方向

重新设计为**直编辑模式**：打开面板即可直接修改值，无需 Browse/Edit 模式切换。6 个字段分两组显示：

- **通用**分组：Autocompact（开关）+ Compact 阈值（数字）+ Language（选择/输入）+ Proactiveness（三选一）
- **提示词覆盖**分组：Persona（文本）+ Tone（文本）

操作一致性：
- 布尔/选择字段：Space 切换，行为统一
- 文本字段：键盘输入，行为统一
- Enter 保存并关闭，Esc 不保存关闭
- 每个字段附简短说明文字（当前值/默认值/有效范围）

期望效果：即改即走，快速配置。

## 涉及文件

- `peri-tui/src/app/config_panel.rs`（537 行）—— ConfigPanel 结构体、ConfigPanelMode、ConfigEditField、所有交互逻辑
- `peri-tui/src/ui/main_ui/panels/config.rs`（243 行）—— Config 面板渲染
- `peri-tui/src/app/panel_config.rs`—— 打开/关闭 ConfigPanel 的 App 扩展方法
- `peri-tui/src/command/core/config.rs`—— /config 命令定义
- `peri-tui/src/app/config_panel_test.rs`—— ConfigPanel 单元测试
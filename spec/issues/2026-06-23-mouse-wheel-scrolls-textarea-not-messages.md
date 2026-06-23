# ConPTY 下鼠标滚轮滚动 textarea 而非消息区 — crossterm EnableMouseCapture 不发送 ANSI 序列

**状态**：Fixed
**优先级**：高
**创建日期**：2026-06-23

## 问题描述

Windows Terminal 下，用户滚动鼠标滚轮想查看历史消息，但消息区纹丝不动，textarea（输入框）内容却在上下滚动。

用户预期：滚轮 → 消息区上下滚动。
实际行为：滚轮 → textarea 光标移动导致内容滚动。

## 症状详情

| 维度 | 描述 |
|------|------|
| 消息区 | 完全不响应滚轮 |
| textarea | 内容随滚轮上下移动（光标在多行间跳转） |
| 复现频率 | 100%（Windows Terminal + ConPTY） |
| 影响范围 | 所有使用 alternate screen + EnableMouseCapture 的 Windows Terminal 会话 |

## 复现条件

- **复现频率**：必现
- **环境**：Windows Terminal（ConPTY），Windows 10/11
- **触发步骤**：
  1. `cargo run -p peri-tui` 启动 TUI
  2. 发送一条消息（确保有历史消息可滚动）
  3. 鼠标滚轮向上滚动
  4. 观察：消息区不动，textarea 内容在滚动

## 根因分析

### 因果链（全部基于源码确认，零猜测）

#### 环节 1：crossterm `EnableMouseCapture` 在 Windows 上不发送 ANSI 序列

```rust
// crossterm 0.29.0 src/event.rs:318-346
impl Command for EnableMouseCapture {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        // ?1000h ?1002h ?1003h ?1015h ?1006h — 但这条路径在 Windows 上不走
        f.write_str(concat!(csi!("?1000h"), csi!("?1002h"), ...))
    }

    #[cfg(windows)]
    fn execute_winapi(&self) -> std::io::Result<()> {
        sys::windows::enable_mouse_capture()  // 只设 ConsoleMode
    }

    #[cfg(windows)]
    fn is_ansi_code_supported(&self) -> bool {
        false  // ← 关键：强制走 WinAPI，跳过 ANSI
    }
}
```

```rust
// crossterm 0.29.0 src/command.rs:121-134 — queue() 分发逻辑
fn queue(&mut self, command: impl Command) -> io::Result<&mut Self> {
    #[cfg(windows)]
    if !command.is_ansi_code_supported() {   // EnableMouseCapture → false → !false = true
        self.flush()?;
        command.execute_winapi()?;            // ← 只设置 ConsoleMode
        return Ok(self);                      // ← 直接返回，?1000h 从未发送
    }
    write_command_ansi(self, command)?;       // ← 永远到不了
    Ok(self)
}
```

```rust
// crossterm 0.29.0 src/event/sys/windows.rs:36-42
const ENABLE_MOUSE_MODE: u32 = 0x0010 | 0x0080 | 0x0008;
// ENABLE_MOUSE_INPUT | ENABLE_EXTENDED_FLAGS | ENABLE_WINDOW_INPUT

pub(crate) fn enable_mouse_capture() -> std::io::Result<()> {
    let mode = ConsoleMode::from(Handle::current_in_handle()?);
    init_original_console_mode(mode.mode()?);
    mode.set_mode(ENABLE_MOUSE_MODE)?;  // ← 只调 SetConsoleMode，零 stdout I/O
    Ok(())
}
```

**结论**：`EnableMouseCapture` 在 Windows 上通过 `SetConsoleMode` 设置 ConPTY Console 对象的 `ENABLE_MOUSE_INPUT`，**不发送 `?1000h` ANSI 序列**。Windows Terminal 前端永远不知道 mouse tracking 已开启。

#### 环节 2：`EnterAlternateScreen` 确实发送了 ANSI 序列

```rust
// crossterm 0.29.0 src/terminal.rs:220-231
impl Command for EnterAlternateScreen {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        f.write_str(csi!("?1049h"))
    }

    #[cfg(windows)]
    fn execute_winapi(&self) -> io::Result<()> {
        let alternate_screen = ScreenBuffer::create()?;
        alternate_screen.show()?;
        Ok(())
    }
    // 没有覆盖 is_ansi_code_supported() → 使用默认实现 supports_ansi()
}
```

在 Windows Terminal (ConPTY) 下，`supports_ansi()` 返回 `true`，所以 `EnterAlternateScreen` 走 `write_ansi()` 路径，发送 `\x1b[?1049h`。Windows Terminal 收到后进入 alternate screen。

**对比**：`EnterAlternateScreen` 的 `is_ansi_code_supported()` 用默认实现（返回 `true`），而 `EnableMouseCapture` 显式覆盖为 `false`。这就是为什么前者发送 ANSI 而后者不发送。

#### 环节 3：Windows Terminal 在 alt screen + 无 mouse tracking 下将滚轮转为方向键

```cpp
// Windows Terminal src/terminal/input/mouseInput.cpp:295-389
TerminalInput::OutputType TerminalInput::HandleMouse(...)
{
    if (IsTrackingMouseInput())          // 没收到 ?1000h → false → 跳过
    {
        // 滚轮作为鼠标事件报告 — 到不了这里
    }

    if (ShouldSendAlternateScroll(button, delta))  // ← 执行到这里
    {
        return _makeAlternateScrollOutput(button, delta);  // 滚轮 → 方向键
    }
}
```

```cpp
// mouseInput.cpp:488-494 — alternate scroll 生效条件
bool ShouldSendAlternateScroll(button, delta) const noexcept
{
    return _inAlternateBuffer              // ?1049h 已发送 → true ✓
        && _inputMode.test(Mode::AlternateScroll)  // 默认 ON（构造函数写死）✓
        && wasMouseWheel;                 // ✓
}
```

```cpp
// mouseInput.cpp:500-526 — 滚轮 → 方向键
TerminalInput::OutputType _makeAlternateScrollOutput(button, delta)
{
    vkey = delta > 0 ? VK_UP : VK_DOWN;  // 滚轮向上 → ↑，向下 → ↓
}
```

**关键事实**：alternate scroll mode (`?1007`) 默认 ON，写在 `TerminalInput` 构造函数中。没有 profile 设置可以关闭——只能通过应用发送 `\x1b[?1007l`。当 mouse tracking 关闭时，滚轮在 alt screen 中必然被转为方向键。

#### 环节 4：方向键通过 ConPTY → Console → crossterm

```rust
// crossterm 0.29.0 src/event/source/windows.rs — Windows 100% 走 WinAPI
impl EventSource for WindowsEventSource {
    fn try_read(&mut self, ...) {
        let event = match self.console.read_single_input_event()? {
            InputRecord::KeyEvent(record) => handle_key_event(record, ...),  // ← 方向键
            InputRecord::MouseEvent(record) => handle_mouse_event(record, ...), // ← 到不了
        };
    }
}
```

Windows Terminal 将滚轮转为方向键事件 → ConPTY 放入 Console 输入队列 → `ReadConsoleInputW` 返回 `KEY_EVENT_RECORD` → crossterm 解析为 `Event::Key(KeyCode::Up/Down)`。

#### 环节 5：peri-tui 代码逻辑（已验证无其他路径）

```
Event::Key(Down)
  → handle_event (event/mod.rs:318)
    → keyboard::handle_key_event
      → normal_keys.rs:52: Input { key: Key::Down, .. } => handle_down(app)
        → normal_keys.rs:500: textarea.input(Key::Down)
          → textarea 光标下移 → 视口跟随 → 内容滚动
```

**textarea 滚动的唯一路径**：`textarea.input(Key::Up/Down)`。全代码库搜索确认无其他代码路径在滚轮时移动 textarea 光标（`textarea.move_cursor` 仅用于鼠标拖拽选区和渲染时的 `!` 前缀处理，不在 ScrollUp/Down 路径中）。

`Event::Mouse(ScrollUp/Down)` 路径（`event/mod.rs:380`）只调用 `app.scroll_up()/down()`（`thread_ops.rs:4-29`），只修改 `ui.scroll_offset` + `ui.scroll_follow`，**完全不触碰 textarea**。

`Event::Key` 和 `Event::Mouse` 在 `handle_event:305` 是互斥的 match arm。

### 为什么旧架构（inline viewport）没这个问题

旧代码用 `Viewport::Inline(height)`，不进入 alternate screen，Windows Terminal 保留原生 scrollback。滚轮由终端原生处理（滚动 scrollback），不经过 ConPTY → crossterm 路径，alternate scroll mode 不生效。

### 完整因果链总结

```
EnableMouseCapture.is_ansi_code_supported() = false
  → queue() 走 execute_winapi()，只 SetConsoleMode，不发送 ?1000h
    → Windows Terminal 前端 IsTrackingMouseInput() = false
      → ShouldSendAlternateScroll() = true（alt screen + ?1007 默认 ON）
        → 滚轮转为 VK_UP/VK_DOWN
          → ConPTY 传入 Console → ReadConsoleInputW → KEY_EVENT_RECORD
            → crossterm handle_key_event() → Event::Key(KeyCode::Up/Down)
              → normal_keys.rs handle_down() → textarea.input(Key::Down)
                → textarea 内容滚动（消息区纹丝不动）
```

**本质**：crossterm `EnableMouseCapture` 在 Windows 上的 WinAPI 路径只设置了 ConPTY 的 Console 对象，但 Windows Terminal 前端与 Console 对象在 ConPTY 架构下是分离的——WinAPI 无法通知终端前端启用 mouse tracking，只有 ANSI 序列 `?1000h` 才能。这是 crossterm 在 ConPTY 环境下的设计缺陷。

## 涉及文件

- `peri-tui/src/main.rs:495-501` —— 终端初始化（EnterAlternateScreen + EnableMouseCapture）
- `peri-tui/src/app/panel_memory.rs:62-65` —— 外部编辑器恢复（EnterAlternateScreen + EnableMouseCapture）
- `peri-tui/src/event/mod.rs:380-455` —— ScrollUp/ScrollDown 处理（正确，但事件到不了）
- `peri-tui/src/event/keyboard/normal_keys.rs:439-508` —— handle_up/handle_down → textarea.input
- `peri-tui/src/app/thread_ops.rs:4-29` —— scroll_up/scroll_down（消息区滚动）

## 修复方案

### 方案 A（推荐）：手动发送 ANSI mouse tracking 序列

在 `EnableMouseCapture` 之后手动发送 `\x1b[?1000h\x1b[?1006h`，确保 Windows Terminal 前端启用 mouse tracking：

```rust
// main.rs
execute!(
    stdout,
    EnterAlternateScreen,
    EnableMouseCapture,
    EnableBracketedPaste,
    EnableFocusChange
)?;
// crossterm EnableMouseCapture 在 Windows 上 is_ansi_code_supported()=false，
// 走 WinAPI 路径只设 ConsoleMode，不发送 ?1000h ANSI 序列。
// ConPTY 下 Windows Terminal 前端不知道 mouse tracking 已开启，
// alternate scroll mode（默认 ON）将滚轮转为方向键 → textarea 滚动而非消息区。
std::io::Write::write_all(&mut stdout, b"\x1b[?1000h\x1b[?1006h")?;
std::io::Write::flush(&mut stdout)?;
```

`panel_memory.rs:62-65` 外部编辑器恢复后同样需要追加。

### 方案 B（备选）：禁用 alternate scroll mode

```rust
std::io::Write::write_all(&mut stdout, b"\x1b[?1007l")?;
std::io::Write::flush(&mut stdout)?;
```

方案 B 更简洁但只解决滚轮问题；方案 A 更完整，确保所有鼠标事件（点击、拖拽、滚轮）在 ConPTY 下都正确报告。推荐方案 A。

### 退出时配套清理

```rust
// main.rs 恢复终端处
std::io::Write::write_all(terminal.backend_mut(), b"\x1b[?1006l\x1b[?1000l")?;
std::io::Write::flush(terminal.backend_mut())?;
```

## 验证方法

修复后确认：
1. 滚轮在消息区 → 消息区上下滚动（调用 `app.scroll_up()/down()`）
2. 滚轮在 textarea 上 → textarea 不动（方向键不到达）
3. 鼠标拖拽选区 → 正常工作
4. 鼠标点击面板 → 正常工作

## [TRAP] 经验沉淀

**crossterm `EnableMouseCapture` 在 Windows + ConPTY 下不发送 ANSI `?1000h`，必须手动补发。**

**Why:** crossterm 的 `EnableMouseCapture::is_ansi_code_supported()` 硬编码返回 `false`，导致 `execute!` 走 WinAPI 路径（`SetConsoleMode`），不发送 VT 序列。在 ConPTY 架构下，WinAPI 设置的是 ConPTY 的 Console 对象，而 Windows Terminal 前端与 Console 对象分离——终端前端的 mouse tracking 状态只受 VT 序列控制。终端前端不知道 mouse tracking 开启 → alternate scroll mode（默认 ON）将滚轮转为方向键。

**How to apply:**
- 任何使用 `EnableMouseCapture` 的位置（终端初始化、外部编辑器恢复），都必须在之后手动发送 `\x1b[?1000h\x1b[?1006h`
- 对应的退出/挂起路径必须发送 `\x1b[?1006l\x1b[?1000l` 清理
- 不要假设 `EnableMouseCapture` 在所有平台上行为一致——Unix 走 ANSI 路径，Windows 走 WinAPI 路径
- ConPTY 是中间层：WinAPI 控制应用端 Console 对象，VT 序列控制终端前端，两者必须同步

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-06-23 | — | Open | agent | 根因分析完成，源码级证据链 100% 确认 |
| 2026-06-23 | Open | Fixed | agent | commit 82db370f：手动发送 ?1000h ?1006h |

## 修复记录

### 修复 #1（2026-06-23）

- **操作人**：agent
- **修复内容**：在 `EnableMouseCapture` 之后手动发送 `\x1b[?1000h\x1b[?1006h`，退出/挂起时发送 `\x1b[?1006l\x1b[?1000l` 清理
- **涉及 commit**：`82db370f`
- **涉及文件**：`peri-tui/src/main.rs`（终端初始化 + 退出）、`peri-tui/src/app/panel_memory.rs`（外部编辑器挂起 + 恢复）
- **验证状态**：待用户运行时验证（cargo check 通过）

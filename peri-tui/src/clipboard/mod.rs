//! 剪贴板抽象层
//!
//! 参照 openai/codex 的 `clipboard_copy.rs` 实现多层 fallback：
//! 1. SSH 远程会话：优先 tmux clipboard，回落 OSC 52 转义
//! 2. 本地：优先 arboard（native clipboard）
//! 3. WSL 本地：arboard 失败时回落到 powershell.exe Set-Clipboard
//! 4. 都失败：tmux / OSC 52 兜底
//!
//! Linux X11/Wayland 的 ClipboardLease 持有 + macOS SuppressStderr 抑制
//! 留待后续 issue（#1 内补 lease、#3 单独做 stderr）。

pub mod copy;

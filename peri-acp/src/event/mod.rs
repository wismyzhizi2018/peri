//! Event mapping from ExecutorEvent to ACP SessionUpdate.
//!
//! Translates peri-agent executor events into standard ACP session notifications
//! for consumption by TUI or other frontends.

pub mod mapper;
pub use mapper::*;

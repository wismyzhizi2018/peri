// ─── Core module (always available) ────────────────────────────
#[cfg(feature = "core")]
pub mod core;

// ─── Runtime modules (feature-gated) ─────────────────────────────
#[cfg(feature = "runtime")]
pub mod api;
#[cfg(feature = "runtime")]
pub mod db;
#[cfg(feature = "runtime")]
pub mod runner;
#[cfg(feature = "runtime")]
pub mod schema;
#[cfg(feature = "runtime")]
pub mod watcher;

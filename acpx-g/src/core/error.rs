/// Core error types for the acpx-g DAG engine.
///
/// Uses manual `std::fmt::Display` + `std::error::Error` impls
/// (no thiserror dependency) to keep the `core` feature lightweight.
use std::fmt;

/// Top-level error for all core operations.
#[derive(Debug)]
pub struct CoreError {
    pub kind: ErrorKind,
    pub message: String,
    // TODO: source chain when needed
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// YAML parsing failed.
    Parse,
    /// Structural validation failed (empty name, cycle, etc.).
    Validation,
    /// File I/O error.
    Io,
    /// A reference to a node or workflow was not found.
    NotFound,
    /// Execution was cancelled.
    Cancelled,
    /// Execution timed out.
    Timeout,
    /// A node exited with a non-zero code.
    NodeFailed,
    /// Generic / uncategorised.
    Other,
}

impl CoreError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn parse(msg: impl Into<String>) -> Self {
        Self::new(ErrorKind::Parse, msg)
    }

    pub fn validation(msg: impl Into<String>) -> Self {
        Self::new(ErrorKind::Validation, msg)
    }

    pub fn io(msg: impl Into<String>) -> Self {
        Self::new(ErrorKind::Io, msg)
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::new(ErrorKind::NotFound, msg)
    }

    pub fn cancelled(msg: impl Into<String>) -> Self {
        Self::new(ErrorKind::Cancelled, msg)
    }

    pub fn timeout(msg: impl Into<String>) -> Self {
        Self::new(ErrorKind::Timeout, msg)
    }

    pub fn node_failed(msg: impl Into<String>) -> Self {
        Self::new(ErrorKind::NodeFailed, msg)
    }
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.kind.as_str(), self.message)
    }
}

impl std::error::Error for CoreError {}

impl ErrorKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Parse => "parse error",
            Self::Validation => "validation error",
            Self::Io => "I/O error",
            Self::NotFound => "not found",
            Self::Cancelled => "cancelled",
            Self::Timeout => "timeout",
            Self::NodeFailed => "node failed",
            Self::Other => "error",
        }
    }
}

// ─── anyhow interop ─────────────────────────────────────────────────
// CoreError already impl std::error::Error, so anyhow blanket From<E> handles it.
// No manual From<CoreError> for anyhow::Error needed.

//! Core DAG workflow engine — zero-dependency (aside from `rayon`) foundation.
//!
//! This module contains the pure DAG logic: schema types, topological sort,
//! template interpolation, and synchronous execution primitives. It has **no**
//! async runtime, no database, no HTTP — those live in the `runtime` feature
//! layers (`runner/`, `db/`, `api/`, `watcher/`).

pub mod context;
pub mod dag;
pub mod engine;
pub mod error;
pub mod executor;
pub mod schema;
pub mod template;

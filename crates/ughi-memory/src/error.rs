// UGHI-memory/src/error.rs
// Follows strict_rules.md | No panic! in core
// Memory cost: ~128 bytes per variant (stack)

use thiserror::Error;

/// Memory errors – all recoverable, no panic!
#[derive(Error, Debug)]
pub enum MemoryError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("memory budget exceeded: agent {agent_id} used {used_mb:.1} MB > {limit_mb:.1} MB")]
    BudgetExceeded {
        agent_id: String,
        used_mb: f64,
        limit_mb: f64,
    },

    #[error("key not found: {key}")]
    KeyNotFound { key: String },

    #[error("agent namespace not found: {agent_id}")]
    NamespaceNotFound { agent_id: String },

    #[error("embedding dimension mismatch: expected {expected}, got {got}")]
    DimensionMismatch { expected: usize, got: usize },

    #[error("short-term memory full: {used_mb:.1} MB >= {limit_mb:.1} MB")]
    ShortTermFull { used_mb: f64, limit_mb: f64 },

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type MemoryResult<T> = Result<T, MemoryError>;

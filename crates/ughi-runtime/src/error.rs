// UGHI-runtime/src/error.rs
// Follows strict_rules.md | All errors recoverable | No panic! in core
// Memory cost: ~128 bytes per error variant (stack-allocated)

use thiserror::Error;

/// Comprehensive runtime errors – every variant is recoverable.
/// Memory cost: ~128 bytes per variant (stack)
#[derive(Error, Debug)]
pub enum RuntimeError {
    #[error("agent limit exceeded: max {max} agents (strict_rules.md)")]
    AgentLimitExceeded { max: u32 },

    #[error("agent not found: {id}")]
    AgentNotFound { id: String },

    #[error("agent memory budget exceeded: {used_bytes} bytes > {limit_bytes} bytes (140 MB peak per agent.md)")]
    MemoryBudgetExceeded { used_bytes: u64, limit_bytes: u64 },

    #[error("agent crashed: {id} – reason: {reason}")]
    AgentCrashed { id: String, reason: String },

    #[error("scheduler queue full: capacity {capacity}")]
    SchedulerFull { capacity: usize },

    #[error("spawn timeout: agent did not reach Spawned state within {timeout_ms} ms")]
    SpawnTimeout { timeout_ms: u64 },

    #[error("invalid state transition: {id} cannot go from {from:?} to {to:?}")]
    InvalidTransition {
        id: String,
        from: String,
        to: String,
    },

    #[error("agent already exists: {id}")]
    AgentAlreadyExists { id: String },

    #[error("runtime shutting down")]
    ShuttingDown,

    #[error("channel error: {reason}")]
    ChannelError { reason: String },
}

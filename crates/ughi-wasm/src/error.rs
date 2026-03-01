// UGHI-wasm/src/error.rs
// Follows strict_rules.md | No panic! in core
// Memory cost: ~128 bytes per variant (stack)

use thiserror::Error;

/// Sandbox errors – all recoverable, no panic!
/// strict_rules.md rule #6: "All errors must be recoverable."
#[derive(Error, Debug)]
pub enum SandboxError {
    #[error("WASM engine init failed: {reason}")]
    EngineInitFailed { reason: String },

    #[error("module compilation failed: {reason}")]
    CompilationFailed { reason: String },

    #[error("capability denied: agent {agent_id} lacks token for {skill}")]
    CapabilityDenied { agent_id: String, skill: String },

    #[error("capability expired: agent {agent_id} token for {skill} expired")]
    CapabilityExpired { agent_id: String, skill: String },

    #[error("skill not registered: {skill}")]
    SkillNotRegistered { skill: String },

    #[error("resource limit exceeded: {resource} ({used} > {limit})")]
    ResourceExceeded {
        resource: String,
        used: u64,
        limit: u64,
    },

    #[error("execution trapped: {reason}")]
    ExecutionTrapped { reason: String },

    #[error("execution timeout: {elapsed_ms}ms > {limit_ms}ms")]
    ExecutionTimeout { elapsed_ms: u64, limit_ms: u64 },

    #[error("sandbox violation: agent {agent_id} – {reason}")]
    SecurityViolation { agent_id: String, reason: String },

    #[error("agent quarantined: {agent_id} (violations: {count})")]
    AgentQuarantined { agent_id: String, count: u32 },

    #[error("wasmtime error: {0}")]
    Wasmtime(#[from] anyhow::Error),
}

pub type SandboxResult<T> = Result<T, SandboxError>;

// UGHI-inference/src/error.rs
// Follows strict_rules.md | No panic! in core
// Memory cost: ~128 bytes per variant (stack-allocated)
// All inference errors are recoverable – no crashes.

use thiserror::Error;

/// Inference errors – all recoverable, no panic! in core.
/// Memory cost: ~128 bytes per variant
#[derive(Error, Debug)]
pub enum InferenceError {
    /// Model file not found on disk
    #[error("model not found: {path}")]
    ModelNotFound { path: String },

    /// Model format unsupported or corrupt
    #[error("invalid model format: {reason}")]
    InvalidModelFormat { reason: String },

    /// Model failed to load (OOM, corrupt weights, etc.)
    #[error("model load failed: {reason}")]
    ModelLoadFailed { reason: String },

    /// Inference forward pass failed
    #[error("inference failed: {reason}")]
    InferenceFailed { reason: String },

    /// Token sampling failed
    #[error("sampling failed: {reason}")]
    SamplingFailed { reason: String },

    /// Agent's KV cache slot exceeded budget (45 MB per agent)
    #[error("KV cache budget exceeded: agent {agent_id} used {used_mb:.1} MB > {limit_mb:.1} MB")]
    KvCacheBudgetExceeded {
        agent_id: String,
        used_mb: f64,
        limit_mb: f64,
    },

    /// Total model memory exceeded (1.1 GB for 10 agents)
    #[error("model memory budget exceeded: {used_mb:.1} MB > {limit_mb:.1} MB")]
    ModelMemoryExceeded { used_mb: f64, limit_mb: f64 },

    /// No suitable model found for the task
    #[error("no suitable model for task complexity: {complexity}")]
    NoSuitableModel { complexity: String },

    /// Model is currently being loaded by another agent
    #[error("model busy: {model_name} is loading")]
    ModelBusy { model_name: String },

    /// Engine has been shut down
    #[error("inference engine shut down")]
    EngineShutDown,

    /// Candle framework error
    #[error("candle error: {0}")]
    Candle(#[from] candle_core::Error),

    /// IO error (file read, etc.)
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result type alias for inference operations.
pub type InferenceResult<T> = Result<T, InferenceError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = InferenceError::ModelNotFound {
            path: "test.gguf".to_string(),
        };
        assert!(err.to_string().contains("test.gguf"));
    }

    #[test]
    fn test_kv_cache_budget_error() {
        let err = InferenceError::KvCacheBudgetExceeded {
            agent_id: "abc123".to_string(),
            used_mb: 48.5,
            limit_mb: 45.0,
        };
        assert!(err.to_string().contains("48.5"));
    }

    #[test]
    fn test_all_errors_recoverable() {
        // strict_rules.md: "All errors must be recoverable. No panic! in core."
        let errors: Vec<InferenceError> = vec![
            InferenceError::ModelNotFound { path: "x".into() },
            InferenceError::InferenceFailed { reason: "x".into() },
            InferenceError::EngineShutDown,
            InferenceError::NoSuitableModel {
                complexity: "x".into(),
            },
        ];
        for err in errors {
            // All should be displayable (no panic)
            let _ = format!("{}", err);
        }
    }
}

// UGHI-inference/src/lib.rs
// Follows strict_rules.md | No GPU ever | CPU only (Candle)
// Memory: ~8 MB base engine, model weights loaded lazily and shared.
// Target: ≥ 15 tokens/sec on CPU (Phi-3-mini-3.8B 4-bit or Gemma-2-2B)
// KV cache: ≤ 45 MB per agent, shared model weights across all agents.
// 10 agents × 45 MB KV + 1 model (~0.9-2.0 GB) ≤ 1.1 GB total.
// No panic! in core – all errors via InferenceError.
//
// Module structure:
// - error:    InferenceError enum (all recoverable)
// - model:    Model catalog, auto-selection, ModelConfig
// - request:  InferenceRequest/Response, StreamToken, InferenceMetrics
// - sampler:  Temperature, top-k, top-p token sampling
// - engine:   SharedInferenceEngine (model pool, KV cache, lazy load)
// - reflect:  Self-critique / reflection mode (skills.md SelfCritique)

pub mod engine;
pub mod error;
pub mod model;
pub mod reflect;
pub mod request;
pub mod sampler;

// --- Public re-exports for ergonomic API ---

pub use engine::{EngineConfig, SharedInferenceEngine};
pub use error::InferenceError;
pub use model::{auto_select_model, ModelConfig, ModelFamily, TaskComplexity};
pub use reflect::ReflectionEngine;
pub use request::{
    InferenceMetrics, InferenceRequest, InferenceResponse, ReflectionResult, StreamToken,
};
pub use sampler::{SamplingParams, TokenSampler};

// --- Backward compatibility with kernel main.rs ---

/// Legacy InferenceEngine for the kernel boot sequence.
/// Wraps SharedInferenceEngine for backward compatibility.
/// Memory cost: ~8 MB
pub struct InferenceEngine {
    inner: SharedInferenceEngine,
    model_path: String,
}

impl InferenceEngine {
    /// Create a new inference engine (backward compatible).
    /// Memory cost: ~8 MB
    pub fn new(model_path: &str) -> Result<Self, InferenceError> {
        let config = EngineConfig {
            default_model: ModelFamily::Qwen1_5B,
            ..Default::default()
        };
        Ok(Self {
            inner: SharedInferenceEngine::new(config),
            model_path: model_path.to_string(),
        })
    }

    /// Check if model is loaded.
    pub fn is_model_loaded(&self) -> bool {
        // Lazy load – not loaded until first infer
        false
    }

    /// Get model path.
    pub fn model_path(&self) -> &str {
        &self.model_path
    }

    /// Get a reference to the shared engine.
    pub fn shared_engine(&self) -> &SharedInferenceEngine {
        &self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backward_compat() {
        let engine = InferenceEngine::new("test.gguf").unwrap();
        assert!(!engine.is_model_loaded());
        assert_eq!(engine.model_path(), "test.gguf");
    }

    #[test]
    fn test_reexports() {
        // Verify all key types are accessible from crate root
        let _config = EngineConfig::default();
        let _params = SamplingParams::default();
        let _req = InferenceRequest::new("test", "agent-1");
    }
}

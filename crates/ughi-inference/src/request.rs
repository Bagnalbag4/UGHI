// UGHI-inference/src/request.rs
// Follows strict_rules.md | No GPU ever
// Memory cost: proportional to prompt/response length
// Defines the input/output types for inference operations.

use serde::{Deserialize, Serialize};

use crate::model::ModelFamily;

/// Inference request – input to the SLM engine.
/// Memory cost: ~prompt_len + 64 bytes overhead
#[derive(Debug, Clone)]
pub struct InferenceRequest {
    /// The prompt text to send to the model
    pub prompt: String,
    /// Maximum tokens to generate (default: 256)
    pub max_tokens: u32,
    /// Temperature for sampling (0.0 = greedy, 1.0 = creative)
    pub temperature: f32,
    /// Top-k sampling (0 = disabled)
    pub top_k: u32,
    /// Top-p (nucleus) sampling (1.0 = disabled)
    pub top_p: f32,
    /// Agent ID requesting inference (for KV cache isolation)
    pub agent_id: String,
    /// Override model selection (None = auto-select)
    pub model_override: Option<ModelFamily>,
    /// Enable self-critique reflection after generation
    pub reflect: bool,
    /// Enable token-by-token streaming
    pub stream: bool,
    /// System prompt prepended to the prompt
    pub system_prompt: Option<String>,
}

impl InferenceRequest {
    /// Create a new inference request with defaults.
    /// Memory cost: ~prompt_len bytes
    pub fn new(prompt: impl Into<String>, agent_id: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            max_tokens: 256,
            temperature: 0.7,
            top_k: 40,
            top_p: 0.9,
            agent_id: agent_id.into(),
            model_override: None,
            reflect: false,
            stream: false,
            system_prompt: None,
        }
    }

    /// Set max tokens.
    pub fn with_max_tokens(mut self, n: u32) -> Self {
        self.max_tokens = n;
        self
    }

    /// Set temperature.
    pub fn with_temperature(mut self, t: f32) -> Self {
        self.temperature = t;
        self
    }

    /// Enable self-critique reflection.
    pub fn with_reflection(mut self) -> Self {
        self.reflect = true;
        self
    }

    /// Enable streaming.
    pub fn with_streaming(mut self) -> Self {
        self.stream = true;
        self
    }

    /// Override model selection.
    pub fn with_model(mut self, model: ModelFamily) -> Self {
        self.model_override = Some(model);
        self
    }

    /// Set system prompt.
    pub fn with_system_prompt(mut self, sp: impl Into<String>) -> Self {
        self.system_prompt = Some(sp.into());
        self
    }
}

/// Inference response from the SLM engine.
/// Memory cost: ~response_len + 128 bytes overhead
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceResponse {
    /// Generated text output
    pub text: String,
    /// Number of tokens generated
    pub tokens_generated: u32,
    /// Tokens per second achieved (must be ≥ 15 per strict_rules.md)
    pub tokens_per_sec: f32,
    /// Total inference time in milliseconds
    pub inference_time_ms: u64,
    /// Model that was used
    pub model_used: String,
    /// Task complexity auto-detected
    pub task_complexity: String,
    /// Agent ID that requested inference
    pub agent_id: String,
    /// Memory used for this inference (KV cache, bytes)
    pub kv_cache_bytes: u64,
    /// Self-critique reflection result (if enabled)
    pub reflection: Option<ReflectionResult>,
}

/// Result of the self-critique reflection pass.
/// Memory cost: ~reflection_len + 64 bytes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflectionResult {
    /// The critique text from the self-review
    pub critique: String,
    /// Confidence score 0.0–1.0
    pub confidence: f32,
    /// Whether the reflection suggests revising the response
    pub should_revise: bool,
    /// Revised response (if should_revise is true)
    pub revised_text: Option<String>,
    /// Tokens used for the reflection pass
    pub reflection_tokens: u32,
}

/// A single streamed token during generation.
/// Memory cost: ~32 bytes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamToken {
    /// The token text
    pub token: String,
    /// Token index in the sequence
    pub index: u32,
    /// Whether this is the final token
    pub is_final: bool,
    /// Cumulative tokens per second
    pub tokens_per_sec: f32,
}

/// Inference engine metrics snapshot for the dashboard.
/// Memory cost: ~128 bytes
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InferenceMetrics {
    /// Number of models currently loaded
    pub models_loaded: u32,
    /// Total model weight memory in bytes
    pub model_memory_bytes: u64,
    /// Total KV cache memory across all agents
    pub kv_cache_total_bytes: u64,
    /// Total inference requests served
    pub total_requests: u64,
    /// Total tokens generated
    pub total_tokens_generated: u64,
    /// Average tokens per second across all requests
    pub avg_tokens_per_sec: f32,
    /// Currently active inference requests
    pub active_inferences: u32,
    /// Model load count (including reloads)
    pub model_loads: u64,
    /// Model unload count (idle evictions)
    pub model_unloads: u64,
}

impl std::fmt::Display for InferenceMetrics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "models={} mem={:.1}MB kv={:.1}MB reqs={} tok={} avg_tps={:.1}",
            self.models_loaded,
            self.model_memory_bytes as f64 / (1024.0 * 1024.0),
            self.kv_cache_total_bytes as f64 / (1024.0 * 1024.0),
            self.total_requests,
            self.total_tokens_generated,
            self.avg_tokens_per_sec,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_builder() {
        let req = InferenceRequest::new("Hello", "agent-001")
            .with_max_tokens(512)
            .with_temperature(0.5)
            .with_reflection()
            .with_streaming();

        assert_eq!(req.prompt, "Hello");
        assert_eq!(req.max_tokens, 512);
        assert_eq!(req.temperature, 0.5);
        assert!(req.reflect);
        assert!(req.stream);
    }

    #[test]
    fn test_response_serializable() {
        let resp = InferenceResponse {
            text: "test".to_string(),
            tokens_generated: 10,
            tokens_per_sec: 15.5,
            inference_time_ms: 645,
            model_used: "phi-3-mini".to_string(),
            task_complexity: "complex".to_string(),
            agent_id: "abc123".to_string(),
            kv_cache_bytes: 45 * 1024 * 1024,
            reflection: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("tokens_per_sec"));
    }

    #[test]
    fn test_metrics_display() {
        let m = InferenceMetrics {
            models_loaded: 1,
            total_requests: 42,
            avg_tokens_per_sec: 18.3,
            ..Default::default()
        };
        let s = format!("{}", m);
        assert!(s.contains("18.3"));
    }
}

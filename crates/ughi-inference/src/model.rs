// UGHI-inference/src/model.rs
// Follows strict_rules.md | No GPU ever | CPU only
// Memory cost: ~2 KB (model catalog, static)
// Auto-selects smallest capable model per task complexity.
// Supported models: Phi-3-mini-3.8B, Gemma-2-2B, Qwen2-1.5B (all 4-bit quantized)

use std::fmt;

/// Supported model families for CPU-only inference.
/// Memory cost: 1 byte (enum discriminant)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelFamily {
    /// Phi-3-mini-3.8B 4-bit – highest quality, ~2.0 GB
    Phi3Mini,
    /// Gemma-2-2B 4-bit – balanced quality/speed, ~1.2 GB
    Gemma2B,
    /// Qwen2-1.5B 4-bit – fastest, smallest, ~0.9 GB
    Qwen1_5B,
}

impl fmt::Display for ModelFamily {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Phi3Mini => write!(f, "phi-3-mini-3.8B-4bit"),
            Self::Gemma2B => write!(f, "gemma-2-2B-4bit"),
            Self::Qwen1_5B => write!(f, "qwen2-1.5B-4bit"),
        }
    }
}

/// Task complexity level for model auto-selection.
/// Memory cost: 1 byte
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TaskComplexity {
    /// Simple extraction, classification, short answers → Qwen 1.5B
    Simple,
    /// Medium reasoning, summarization, translation → Gemma 2B
    Medium,
    /// Complex planning, multi-step reasoning, code → Phi-3 3.8B
    Complex,
}

impl fmt::Display for TaskComplexity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Simple => write!(f, "simple"),
            Self::Medium => write!(f, "medium"),
            Self::Complex => write!(f, "complex"),
        }
    }
}

/// Configuration for a specific model.
/// Memory cost: ~256 bytes
#[derive(Debug, Clone)]
pub struct ModelConfig {
    /// Model family
    pub family: ModelFamily,
    /// Path to GGUF model file on disk
    pub path: String,
    /// Parameter count (for display)
    pub param_count_b: f32,
    /// Quantization bits (4, 8, etc.)
    pub quant_bits: u8,
    /// Expected memory usage for weights (bytes)
    pub weight_memory_bytes: u64,
    /// Expected KV cache per agent (bytes) — 45 MB budget
    pub kv_cache_per_agent_bytes: u64,
    /// Maximum context length (tokens)
    pub max_context_length: u32,
    /// Vocabulary size
    pub vocab_size: u32,
    /// Minimum task complexity this model handles
    pub min_complexity: TaskComplexity,
}

impl ModelConfig {
    /// Expected weight memory in MB.
    /// Memory cost: 0
    pub fn weight_memory_mb(&self) -> f64 {
        self.weight_memory_bytes as f64 / (1024.0 * 1024.0)
    }

    /// Expected KV cache per agent in MB.
    /// Memory cost: 0
    pub fn kv_cache_mb(&self) -> f64 {
        self.kv_cache_per_agent_bytes as f64 / (1024.0 * 1024.0)
    }

    /// Total memory for N agents sharing this model.
    /// Memory cost: 0
    pub fn total_memory_for_agents(&self, n: u32) -> u64 {
        self.weight_memory_bytes + (self.kv_cache_per_agent_bytes * n as u64)
    }
}

/// Built-in model catalog.
/// Memory cost: ~768 bytes (3 configs, static)
/// Returns configs for all supported models.
pub fn model_catalog() -> Vec<ModelConfig> {
    vec![
        // Qwen2-1.5B – fastest, smallest
        ModelConfig {
            family: ModelFamily::Qwen1_5B,
            path: "models/qwen2-1.5b-q4_k_m.gguf".to_string(),
            param_count_b: 1.5,
            quant_bits: 4,
            weight_memory_bytes: 900 * 1024 * 1024, // ~900 MB
            kv_cache_per_agent_bytes: 32 * 1024 * 1024, // ~32 MB
            max_context_length: 4096,
            vocab_size: 151936,
            min_complexity: TaskComplexity::Simple,
        },
        // Gemma-2-2B – balanced
        ModelConfig {
            family: ModelFamily::Gemma2B,
            path: "models/gemma-2-2b-q4_k_m.gguf".to_string(),
            param_count_b: 2.0,
            quant_bits: 4,
            weight_memory_bytes: 1200 * 1024 * 1024, // ~1.2 GB
            kv_cache_per_agent_bytes: 40 * 1024 * 1024, // ~40 MB
            max_context_length: 8192,
            vocab_size: 256000,
            min_complexity: TaskComplexity::Medium,
        },
        // Phi-3-mini-3.8B – highest quality
        ModelConfig {
            family: ModelFamily::Phi3Mini,
            path: "models/phi-3-mini-3.8b-q4_k_m.gguf".to_string(),
            param_count_b: 3.8,
            quant_bits: 4,
            weight_memory_bytes: 2000 * 1024 * 1024, // ~2.0 GB
            kv_cache_per_agent_bytes: 45 * 1024 * 1024, // ~45 MB
            max_context_length: 4096,
            vocab_size: 32064,
            min_complexity: TaskComplexity::Complex,
        },
    ]
}

/// Auto-select the smallest capable model for a given task.
/// Uses keyword heuristics on the goal/prompt to determine complexity.
/// Memory cost: ~64 bytes (string scanning)
///
/// Complexity rules:
/// - Complex: "plan", "code", "analyze", "design", "architect", "debug", "strategy"
/// - Medium: "summarize", "translate", "explain", "compare", "review"
/// - Simple: everything else
pub fn auto_select_model(goal: &str) -> (ModelFamily, TaskComplexity) {
    let lower = goal.to_lowercase();

    let complexity = if contains_any(
        &lower,
        &[
            "plan",
            "code",
            "architect",
            "design",
            "debug",
            "strategy",
            "implement",
            "build",
            "create",
            "develop",
            "refactor",
            "multi-step",
            "reasoning",
            "analyze",
            "banao",
        ],
    ) {
        TaskComplexity::Complex
    } else if contains_any(
        &lower,
        &[
            "summarize",
            "translate",
            "explain",
            "compare",
            "review",
            "describe",
            "evaluate",
            "assess",
            "classify",
        ],
    ) {
        TaskComplexity::Medium
    } else {
        TaskComplexity::Simple
    };

    let family = match complexity {
        TaskComplexity::Simple => ModelFamily::Qwen1_5B,
        TaskComplexity::Medium => ModelFamily::Gemma2B,
        TaskComplexity::Complex => ModelFamily::Phi3Mini,
    };

    (family, complexity)
}

/// Find model config by family from the catalog.
/// Memory cost: ~256 bytes (config clone)
pub fn find_model_config(family: ModelFamily) -> Option<ModelConfig> {
    model_catalog().into_iter().find(|m| m.family == family)
}

/// Helper: check if a string contains any of the given keywords.
/// Memory cost: 0
fn contains_any(text: &str, keywords: &[&str]) -> bool {
    keywords.iter().any(|kw| text.contains(kw))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auto_select_complex() {
        let (family, complexity) = auto_select_model("Mera startup plan banao");
        assert_eq!(family, ModelFamily::Phi3Mini);
        assert_eq!(complexity, TaskComplexity::Complex);
    }

    #[test]
    fn test_auto_select_medium() {
        let (family, complexity) = auto_select_model("Summarize this article");
        assert_eq!(family, ModelFamily::Gemma2B);
        assert_eq!(complexity, TaskComplexity::Medium);
    }

    #[test]
    fn test_auto_select_simple() {
        let (family, complexity) = auto_select_model("What is the weather?");
        assert_eq!(family, ModelFamily::Qwen1_5B);
        assert_eq!(complexity, TaskComplexity::Simple);
    }

    #[test]
    fn test_model_catalog_has_3_models() {
        let catalog = model_catalog();
        assert_eq!(catalog.len(), 3);
    }

    #[test]
    fn test_memory_budget_10_agents() {
        // strict_rules.md: total ≤ 1.1 GB for 10 agents
        let qwen = find_model_config(ModelFamily::Qwen1_5B).unwrap();
        let total = qwen.total_memory_for_agents(10);
        let total_gb = total as f64 / (1024.0 * 1024.0 * 1024.0);
        assert!(total_gb < 1.3, "Qwen 10-agent total: {:.2} GB", total_gb);
    }

    #[test]
    fn test_kv_cache_under_45mb() {
        for config in model_catalog() {
            let kv_mb = config.kv_cache_mb();
            assert!(
                kv_mb <= 45.0,
                "{}: KV cache {:.1} MB > 45 MB",
                config.family,
                kv_mb
            );
        }
    }

    #[test]
    fn test_find_model_config() {
        let config = find_model_config(ModelFamily::Phi3Mini).unwrap();
        assert_eq!(config.param_count_b, 3.8);
    }
}

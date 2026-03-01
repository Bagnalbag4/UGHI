// ughi-computer/src/router.rs
// Follows strict_rules.md | claude.md | Keys encrypted, never logged
// Intelligent Model Router: auto-routes tasks to the best model
// Memory cost: ~1 KB base (routing table + key refs)
//
// 19+ Models Supported:
//   LOCAL: Phi-3-mini (3.8B q4), Gemma-2-2B, Llama-3.2-1B, Mistral-7B-q4
//   CLOUD: GPT-4o, Claude-3.5, Gemini-2.0, Grok-3, DeepSeek-V3, Llama-3.3-70B,
//          Mixtral-8x22B, Command-R+, Qwen-2.5, Yi-Large, DBRX, Jamba-1.5,
//          Reka-Core, Inflection-3, WizardLM-2

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::info;

/// Model provider categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModelProvider {
    // Local models (zero cost, private, always available)
    LocalPhi3,
    LocalGemma2,
    LocalLlama1B,
    LocalMistral,
    // Cloud models (require API key)
    OpenAI,     // GPT-4o, GPT-4o-mini
    Anthropic,  // Claude-3.5 Sonnet/Opus
    Google,     // Gemini 2.0 Flash/Pro
    XAI,        // Grok-3, Grok-3-mini
    DeepSeek,   // DeepSeek-V3, DeepSeek-R1
    Meta,       // Llama-3.3-70B (via Together/Groq)
    Mistral,    // Mixtral-8x22B, Mistral-Large
    Cohere,     // Command-R+
    Alibaba,    // Qwen-2.5-72B
    Yi,         // Yi-Large
    Databricks, // DBRX
    AI21,       // Jamba-1.5
    Reka,       // Reka-Core
    Inflection, // Inflection-3
    Microsoft,  // WizardLM-2
}

impl std::fmt::Display for ModelProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// Task category for intelligent routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TaskCategory {
    /// Complex reasoning, planning, architecture
    Reasoning,
    /// Code generation, debugging, refactoring
    Coding,
    /// Research, web search, fact-finding
    Research,
    /// Creative writing, content, marketing
    Creative,
    /// Data analysis, math, statistics
    Analysis,
    /// Quick Q&A, simple tasks
    Quick,
    /// Private/sensitive data (must stay local)
    Private,
    /// Translation, multilingual
    Multilingual,
    /// Image/video description, vision
    Vision,
    /// Long-context tasks (>100K tokens)
    LongContext,
}

impl TaskCategory {
    /// Auto-detect category from prompt keywords.
    pub fn detect(prompt: &str) -> Self {
        let p = prompt.to_lowercase();
        if p.contains("private") || p.contains("secret") || p.contains("confidential") {
            Self::Private
        } else if p.contains("code")
            || p.contains("build")
            || p.contains("debug")
            || p.contains("deploy")
            || p.contains("app")
            || p.contains("api")
        {
            Self::Coding
        } else if p.contains("research")
            || p.contains("find")
            || p.contains("search")
            || p.contains("analyze data")
        {
            Self::Research
        } else if p.contains("write")
            || p.contains("blog")
            || p.contains("marketing")
            || p.contains("campaign")
            || p.contains("creative")
        {
            Self::Creative
        } else if p.contains("plan")
            || p.contains("architect")
            || p.contains("design system")
            || p.contains("strategy")
        {
            Self::Reasoning
        } else if p.contains("analyze")
            || p.contains("math")
            || p.contains("calculate")
            || p.contains("statistics")
        {
            Self::Analysis
        } else if p.contains("translate") || p.contains("language") {
            Self::Multilingual
        } else if p.contains("image") || p.contains("video") || p.contains("visual") {
            Self::Vision
        } else {
            Self::Quick
        }
    }
}

/// Model routing decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingDecision {
    pub primary: ModelProvider,
    pub fallback: ModelProvider,
    pub category: TaskCategory,
    pub reason: String,
    pub estimated_cost_usd: f64,
    pub estimated_latency_ms: u64,
}

/// Encrypted API key storage (same pattern as multimedia config).
#[allow(dead_code)]
#[derive(Clone)]
struct EncryptedApiKey {
    encrypted: Vec<u8>,
    provider_name: String,
}

impl EncryptedApiKey {
    fn new(provider: &str, plaintext: &str) -> Self {
        let key = derive_key(provider);
        Self {
            encrypted: xor_cipher(plaintext.as_bytes(), &key),
            provider_name: provider.to_string(),
        }
    }

    #[allow(dead_code)]
    fn decrypt(&self) -> String {
        let key = derive_key(&self.provider_name);
        String::from_utf8(xor_cipher(&self.encrypted, &key)).unwrap_or_default()
    }
}

impl std::fmt::Debug for EncryptedApiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ApiKey({},[REDACTED])", self.provider_name)
    }
}

/// Intelligent Model Router.
/// Routes each subtask to the optimal model based on:
/// 1. Task category (reasoning/coding/research/creative/etc.)
/// 2. Available API keys (cloud vs local)
/// 3. Privacy requirements (private → always local)
/// 4. Cost optimization (prefer local when quality is sufficient)
pub struct ModelRouter {
    api_keys: HashMap<ModelProvider, EncryptedApiKey>,
    /// Routing statistics
    pub routes_decided: u64,
    pub local_routes: u64,
    pub cloud_routes: u64,
}

impl ModelRouter {
    pub fn new() -> Self {
        Self {
            api_keys: HashMap::with_capacity(19),
            routes_decided: 0,
            local_routes: 0,
            cloud_routes: 0,
        }
    }

    /// Set an API key for a cloud provider.
    /// Keys are encrypted immediately, plaintext is never stored.
    pub fn set_api_key(&mut self, provider: ModelProvider, key: &str) {
        let name = format!("{:?}", provider);
        self.api_keys
            .insert(provider, EncryptedApiKey::new(&name, key));
        info!(provider = %provider, "API key set (encrypted)");
    }

    /// Check if a cloud provider key is available.
    pub fn has_key(&self, provider: ModelProvider) -> bool {
        self.api_keys.contains_key(&provider)
    }

    /// Route a task to the optimal model.
    pub fn route(&mut self, prompt: &str) -> RoutingDecision {
        let category = TaskCategory::detect(prompt);
        self.routes_decided += 1;

        // Private tasks ALWAYS stay local
        if category == TaskCategory::Private {
            self.local_routes += 1;
            return RoutingDecision {
                primary: ModelProvider::LocalPhi3,
                fallback: ModelProvider::LocalGemma2,
                category,
                reason: "Private data — routed to local model (zero cloud exposure)".into(),
                estimated_cost_usd: 0.0,
                estimated_latency_ms: 2000,
            };
        }

        // Route based on category + available keys
        let (primary, fallback, reason, cost, latency) = match category {
            TaskCategory::Reasoning => {
                if self.has_key(ModelProvider::Anthropic) {
                    (
                        ModelProvider::Anthropic,
                        ModelProvider::LocalPhi3,
                        "Complex reasoning → Claude-3.5 (best CoT)",
                        0.015,
                        3000,
                    )
                } else if self.has_key(ModelProvider::OpenAI) {
                    (
                        ModelProvider::OpenAI,
                        ModelProvider::LocalPhi3,
                        "Complex reasoning → GPT-4o",
                        0.010,
                        2500,
                    )
                } else {
                    (
                        ModelProvider::LocalPhi3,
                        ModelProvider::LocalGemma2,
                        "Complex reasoning → Phi-3 local (no cloud keys)",
                        0.0,
                        4000,
                    )
                }
            }
            TaskCategory::Coding => {
                if self.has_key(ModelProvider::Anthropic) {
                    (
                        ModelProvider::Anthropic,
                        ModelProvider::LocalPhi3,
                        "Code generation → Claude-3.5 Sonnet (best coder)",
                        0.015,
                        2000,
                    )
                } else if self.has_key(ModelProvider::DeepSeek) {
                    (
                        ModelProvider::DeepSeek,
                        ModelProvider::LocalPhi3,
                        "Code generation → DeepSeek-V3 (excellent coder)",
                        0.002,
                        1500,
                    )
                } else {
                    (
                        ModelProvider::LocalPhi3,
                        ModelProvider::LocalMistral,
                        "Code generation → Phi-3 local",
                        0.0,
                        3000,
                    )
                }
            }
            TaskCategory::Research => {
                if self.has_key(ModelProvider::Google) {
                    (
                        ModelProvider::Google,
                        ModelProvider::LocalPhi3,
                        "Research → Gemini 2.0 (best search integration)",
                        0.005,
                        2000,
                    )
                } else if self.has_key(ModelProvider::XAI) {
                    (
                        ModelProvider::XAI,
                        ModelProvider::LocalPhi3,
                        "Research → Grok-3 (real-time web access)",
                        0.010,
                        1500,
                    )
                } else {
                    (
                        ModelProvider::LocalPhi3,
                        ModelProvider::LocalGemma2,
                        "Research → Phi-3 local + web search skill",
                        0.0,
                        5000,
                    )
                }
            }
            TaskCategory::Creative => {
                if self.has_key(ModelProvider::Anthropic) {
                    (
                        ModelProvider::Anthropic,
                        ModelProvider::LocalPhi3,
                        "Creative → Claude-3.5 (best creative writing)",
                        0.015,
                        3000,
                    )
                } else if self.has_key(ModelProvider::OpenAI) {
                    (
                        ModelProvider::OpenAI,
                        ModelProvider::LocalPhi3,
                        "Creative → GPT-4o",
                        0.010,
                        2500,
                    )
                } else {
                    (
                        ModelProvider::LocalPhi3,
                        ModelProvider::LocalGemma2,
                        "Creative → Phi-3 local",
                        0.0,
                        4000,
                    )
                }
            }
            TaskCategory::Analysis => {
                if self.has_key(ModelProvider::Google) {
                    (
                        ModelProvider::Google,
                        ModelProvider::LocalPhi3,
                        "Analysis → Gemini 2.0 (strong analytical)",
                        0.005,
                        2000,
                    )
                } else {
                    (
                        ModelProvider::LocalPhi3,
                        ModelProvider::LocalGemma2,
                        "Analysis → Phi-3 local",
                        0.0,
                        3000,
                    )
                }
            }
            TaskCategory::Quick => {
                if self.has_key(ModelProvider::XAI) {
                    (
                        ModelProvider::XAI,
                        ModelProvider::LocalGemma2,
                        "Quick task → Grok-3-mini (fastest cloud)",
                        0.001,
                        500,
                    )
                } else {
                    (
                        ModelProvider::LocalGemma2,
                        ModelProvider::LocalLlama1B,
                        "Quick task → Gemma-2 local (fast, small)",
                        0.0,
                        800,
                    )
                }
            }
            TaskCategory::Multilingual => {
                if self.has_key(ModelProvider::Google) {
                    (
                        ModelProvider::Google,
                        ModelProvider::LocalPhi3,
                        "Multilingual → Gemini (100+ languages)",
                        0.005,
                        2000,
                    )
                } else {
                    (
                        ModelProvider::LocalPhi3,
                        ModelProvider::LocalGemma2,
                        "Multilingual → Phi-3 local",
                        0.0,
                        3000,
                    )
                }
            }
            TaskCategory::Vision | TaskCategory::LongContext => {
                if self.has_key(ModelProvider::Google) {
                    (
                        ModelProvider::Google,
                        ModelProvider::LocalPhi3,
                        "Vision/Long-context → Gemini 2.0 (2M context)",
                        0.005,
                        3000,
                    )
                } else if self.has_key(ModelProvider::Anthropic) {
                    (
                        ModelProvider::Anthropic,
                        ModelProvider::LocalPhi3,
                        "Vision/Long-context → Claude-3.5 (200K ctx)",
                        0.015,
                        3000,
                    )
                } else {
                    (
                        ModelProvider::LocalPhi3,
                        ModelProvider::LocalGemma2,
                        "Vision → Phi-3 local (limited)",
                        0.0,
                        5000,
                    )
                }
            }
            TaskCategory::Private => unreachable!(),
        };

        if matches!(
            primary,
            ModelProvider::LocalPhi3
                | ModelProvider::LocalGemma2
                | ModelProvider::LocalLlama1B
                | ModelProvider::LocalMistral
        ) {
            self.local_routes += 1;
        } else {
            self.cloud_routes += 1;
        }

        RoutingDecision {
            primary,
            fallback,
            category,
            reason: reason.to_string(),
            estimated_cost_usd: cost,
            estimated_latency_ms: latency,
        }
    }

    pub fn key_count(&self) -> usize {
        self.api_keys.len()
    }

    pub fn metrics(&self) -> RouterMetrics {
        RouterMetrics {
            routes_decided: self.routes_decided,
            local_routes: self.local_routes,
            cloud_routes: self.cloud_routes,
            api_keys_configured: self.api_keys.len() as u32,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RouterMetrics {
    pub routes_decided: u64,
    pub local_routes: u64,
    pub cloud_routes: u64,
    pub api_keys_configured: u32,
}

/// Key derivation (FNV-1a, same as other crates).
fn derive_key(name: &str) -> [u8; 32] {
    let mut key = [0u8; 32];
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in name.bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    for i in 0..4 {
        let seg = hash.wrapping_add(i as u64).to_le_bytes();
        key[i * 8..(i + 1) * 8].copy_from_slice(&seg);
    }
    key
}

fn xor_cipher(data: &[u8], key: &[u8; 32]) -> Vec<u8> {
    data.iter()
        .enumerate()
        .map(|(i, b)| b ^ key[i % 32])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_category_detection() {
        assert_eq!(
            TaskCategory::detect("build a REST API"),
            TaskCategory::Coding
        );
        assert_eq!(
            TaskCategory::detect("research market trends"),
            TaskCategory::Research
        );
        assert_eq!(
            TaskCategory::detect("private company data"),
            TaskCategory::Private
        );
        assert_eq!(
            TaskCategory::detect("write a blog post"),
            TaskCategory::Creative
        );
        assert_eq!(TaskCategory::detect("what time is it"), TaskCategory::Quick);
    }

    #[test]
    fn test_private_always_local() {
        let mut router = ModelRouter::new();
        router.set_api_key(ModelProvider::Anthropic, "sk-test");
        let decision = router.route("analyze my private financial data");
        assert!(matches!(decision.primary, ModelProvider::LocalPhi3));
        assert_eq!(decision.estimated_cost_usd, 0.0);
    }

    #[test]
    fn test_cloud_routing_with_keys() {
        let mut router = ModelRouter::new();
        router.set_api_key(ModelProvider::Anthropic, "sk-ant-test");
        let decision = router.route("build a todo app with React");
        assert_eq!(decision.primary, ModelProvider::Anthropic);
        assert!(decision.reason.contains("Claude"));
    }

    #[test]
    fn test_local_fallback_without_keys() {
        let mut router = ModelRouter::new();
        let decision = router.route("build a complex system");
        assert!(matches!(
            decision.primary,
            ModelProvider::LocalPhi3 | ModelProvider::LocalGemma2
        ));
        assert_eq!(decision.estimated_cost_usd, 0.0);
    }

    #[test]
    fn test_metrics() {
        let mut router = ModelRouter::new();
        router.route("test1");
        router.route("private test2");
        let m = router.metrics();
        assert_eq!(m.routes_decided, 2);
        assert_eq!(m.local_routes, 2); // both local (no keys)
    }
}

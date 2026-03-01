// UGHI-inference/src/engine.rs
// Follows strict_rules.md | No GPU ever | CPU only (Candle)
// Memory budget: model weights shared (~0.9-2.0 GB), KV cache ≤ 45 MB/agent
// 10 agents × 45 MB KV + 1 model ≤ 1.1 GB total (for smallest model)
// Lazy load on first request. Auto-unload after idle timeout.
// No panic! in core – all errors via InferenceError.
//
// Architecture:
//   SharedInferenceEngine
//     ├── model_pool: HashMap<ModelFamily, LoadedModel>  (shared weights)
//     ├── kv_caches: HashMap<String, KvCacheSlot>        (per-agent)
//     ├── sampler: TokenSampler                          (shared)
//     └── metrics: InferenceMetrics                      (lock-free atomics)

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::error::{InferenceError, InferenceResult};
use crate::model::{auto_select_model, find_model_config, ModelConfig, ModelFamily};
use crate::reflect::ReflectionEngine;
use crate::request::{InferenceMetrics, InferenceRequest, InferenceResponse};

/// KV cache slot for a single agent.
/// Memory cost: ≤ 45 MB per slot (strict_rules.md)
/// Each agent gets an isolated KV cache so context doesn't leak.
struct KvCacheSlot {
    /// Agent ID owning this slot
    #[allow(dead_code)]
    agent_id: String,
    /// Allocated bytes for this slot's KV cache
    allocated_bytes: u64,
    /// Budget limit (45 MB per agent.md)
    budget_bytes: u64,
    /// Last time this slot was used
    last_used: Instant,
    /// Context tokens stored in this cache
    context_length: u32,
}

impl KvCacheSlot {
    /// Create a new KV cache slot for an agent.
    /// Memory cost: ~128 bytes metadata (actual KV data allocated on use)
    fn new(agent_id: String, budget_bytes: u64) -> Self {
        Self {
            agent_id,
            allocated_bytes: 0,
            budget_bytes,
            last_used: Instant::now(),
            context_length: 0,
        }
    }

    /// Used MB.
    fn used_mb(&self) -> f64 {
        self.allocated_bytes as f64 / (1024.0 * 1024.0)
    }

    /// Budget MB.
    fn budget_mb(&self) -> f64 {
        self.budget_bytes as f64 / (1024.0 * 1024.0)
    }

    /// Check if allocation would exceed budget.
    fn can_allocate(&self, bytes: u64) -> bool {
        self.allocated_bytes + bytes <= self.budget_bytes
    }

    /// Touch the last-used timestamp.
    fn touch(&mut self) {
        self.last_used = Instant::now();
    }

    /// Time since last use.
    fn idle_duration(&self) -> Duration {
        self.last_used.elapsed()
    }
}

/// A loaded model in the pool (weights shared across all agents).
/// Memory cost: weight_memory_bytes (e.g., ~2 GB for Phi-3-mini 4-bit)
struct LoadedModel {
    /// Model configuration
    config: ModelConfig,
    /// Whether weights are actually loaded (vs. simulated)
    weights_loaded: bool,
    /// Candle device (always CPU)
    _device: candle_core::Device,
    /// Time when model was loaded
    #[allow(dead_code)]
    loaded_at: Instant,
    /// Last time any agent used this model
    last_used: Instant,
    /// Total inference requests served by this model
    request_count: u64,
}

impl LoadedModel {
    /// Simulate loading a model.
    /// In production, this loads GGUF weights via candle-transformers.
    /// Memory cost: config.weight_memory_bytes
    fn load(config: ModelConfig) -> InferenceResult<Self> {
        let now = Instant::now();

        // STRICT: CPU only (strict_rules.md rule #3)
        let device = candle_core::Device::Cpu;

        info!(
            model = %config.family,
            params = %config.param_count_b,
            quant = config.quant_bits,
            weight_mb = %config.weight_memory_mb(),
            "loading model weights (CPU-only)"
        );

        // In production: load GGUF via candle_transformers::quantized
        // For now: mark as loaded (simulated mode when file doesn't exist)
        let weights_loaded = std::path::Path::new(&config.path).exists();

        if !weights_loaded {
            info!(
                path = %config.path,
                "model file not found – running in simulated mode"
            );
        }

        Ok(Self {
            config,
            weights_loaded,
            _device: device,
            loaded_at: now,
            last_used: now,
            request_count: 0,
        })
    }

    /// Touch last-used timestamp.
    fn touch(&mut self) {
        self.last_used = Instant::now();
        self.request_count += 1;
    }

    /// Time since last use.
    fn idle_duration(&self) -> Duration {
        self.last_used.elapsed()
    }
}

/// Configuration for the shared inference engine.
/// Memory cost: ~64 bytes
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Maximum number of models loaded simultaneously
    pub max_loaded_models: u32,
    /// KV cache budget per agent (bytes) – 45 MB per agent.md
    pub kv_cache_budget_per_agent: u64,
    /// Idle timeout before unloading a model (seconds)
    pub idle_unload_timeout_secs: u64,
    /// Default model to use when no auto-selection
    pub default_model: ModelFamily,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            max_loaded_models: 2,
            kv_cache_budget_per_agent: 45 * 1024 * 1024, // 45 MB
            idle_unload_timeout_secs: 300,               // 5 minutes
            default_model: ModelFamily::Qwen1_5B,        // Smallest by default
        }
    }
}

/// Shared SLM Inference Engine.
/// One engine serves all agents with shared model weights and per-agent KV caches.
/// Memory cost: ~8 MB base + model weights + KV caches
///
/// Thread-safe: uses Arc<RwLock> for the model pool (agents share read access
/// during inference, write only during load/unload).
/// Metrics use lock-free atomics (no Mutex spam per strict_rules.md).
pub struct SharedInferenceEngine {
    /// Model pool: shared weights, loaded lazily
    pool: Arc<RwLock<HashMap<ModelFamily, LoadedModel>>>,
    /// Per-agent KV cache slots
    kv_caches: Arc<RwLock<HashMap<String, KvCacheSlot>>>,
    /// Reflection engine for self-critique
    reflection: ReflectionEngine,
    /// Engine configuration
    config: EngineConfig,
    /// Lock-free metrics
    total_requests: AtomicU64,
    total_tokens: AtomicU64,
    active_inferences: AtomicU32,
    model_loads: AtomicU64,
    model_unloads: AtomicU64,
}

impl SharedInferenceEngine {
    /// Create a new shared inference engine.
    /// Memory cost: ~8 MB (HashMap + RwLock + atomics)
    pub fn new(config: EngineConfig) -> Self {
        info!(
            max_models = config.max_loaded_models,
            kv_budget_mb = config.kv_cache_budget_per_agent / (1024 * 1024),
            idle_timeout_s = config.idle_unload_timeout_secs,
            default_model = %config.default_model,
            "shared inference engine created"
        );

        Self {
            pool: Arc::new(RwLock::new(HashMap::with_capacity(3))),
            kv_caches: Arc::new(RwLock::new(HashMap::with_capacity(50))),
            reflection: ReflectionEngine::new(),
            config,
            total_requests: AtomicU64::new(0),
            total_tokens: AtomicU64::new(0),
            active_inferences: AtomicU32::new(0),
            model_loads: AtomicU64::new(0),
            model_unloads: AtomicU64::new(0),
        }
    }

    /// Create with default config.
    /// Memory cost: ~8 MB
    pub fn with_defaults() -> Self {
        Self::new(EngineConfig::default())
    }

    /// Run inference on the given request.
    /// Memory cost: model weights (shared, loaded once) + KV cache (≤ 45 MB per agent)
    /// Latency: model-dependent, target ≥ 15 tok/s
    pub async fn infer(&self, request: &InferenceRequest) -> InferenceResult<InferenceResponse> {
        self.active_inferences.fetch_add(1, Ordering::Relaxed);
        let result = self.infer_inner(request).await;
        self.active_inferences.fetch_sub(1, Ordering::Relaxed);
        result
    }

    /// Internal inference implementation.
    async fn infer_inner(&self, request: &InferenceRequest) -> InferenceResult<InferenceResponse> {
        let start = Instant::now();

        // Step 1: Select model (auto or override)
        let (family, complexity) = if let Some(model_override) = request.model_override {
            let (_, c) = auto_select_model(&request.prompt);
            (model_override, c)
        } else {
            auto_select_model(&request.prompt)
        };

        info!(
            agent_id = %request.agent_id,
            model = %family,
            complexity = %complexity,
            prompt_len = request.prompt.len(),
            max_tokens = request.max_tokens,
            "inference request"
        );

        // Step 2: Ensure model is loaded (lazy load)
        self.ensure_model_loaded(family).await?;

        // Step 3: Ensure agent has KV cache slot
        self.ensure_kv_cache(&request.agent_id).await?;

        // Step 4: Run inference (simulated or real)
        let model_config =
            find_model_config(family).ok_or_else(|| InferenceError::NoSuitableModel {
                complexity: complexity.to_string(),
            })?;

        let (text, tokens_generated) = self.generate_tokens(request, &model_config).await?;

        // Step 5: Calculate metrics
        let elapsed = start.elapsed();
        let elapsed_ms = elapsed.as_millis() as u64;
        let tokens_per_sec = if elapsed_ms > 0 {
            (tokens_generated as f32 * 1000.0) / elapsed_ms as f32
        } else {
            0.0
        };

        // Update atomic counters (lock-free)
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.total_tokens
            .fetch_add(tokens_generated as u64, Ordering::Relaxed);

        // Touch KV cache timestamp
        {
            let mut caches = self.kv_caches.write().await;
            if let Some(slot) = caches.get_mut(&request.agent_id) {
                slot.touch();
                slot.context_length += tokens_generated;
            }
        }

        // Touch model last-used
        {
            let mut pool = self.pool.write().await;
            if let Some(model) = pool.get_mut(&family) {
                model.touch();
            }
        }

        let kv_bytes = {
            let caches = self.kv_caches.read().await;
            caches
                .get(&request.agent_id)
                .map(|s| s.allocated_bytes)
                .unwrap_or(0)
        };

        let mut response = InferenceResponse {
            text,
            tokens_generated,
            tokens_per_sec,
            inference_time_ms: elapsed_ms,
            model_used: family.to_string(),
            task_complexity: complexity.to_string(),
            agent_id: request.agent_id.clone(),
            kv_cache_bytes: kv_bytes,
            reflection: None,
        };

        // Step 6: Self-critique reflection (if enabled)
        if request.reflect {
            let reflection_result = self.reflection.reflect(&request.prompt, &response.text);
            response.reflection = Some(reflection_result);
        }

        info!(
            agent_id = %request.agent_id,
            model = %family,
            tokens = tokens_generated,
            tps = format!("{:.1}", tokens_per_sec),
            ms = elapsed_ms,
            "inference complete"
        );

        Ok(response)
    }

    /// Ensure a model is loaded in the pool (lazy load).
    /// Memory cost: model weight_memory_bytes (on first load)
    async fn ensure_model_loaded(&self, family: ModelFamily) -> InferenceResult<()> {
        // Fast path: check if already loaded (read lock only)
        {
            let pool = self.pool.read().await;
            if pool.contains_key(&family) {
                return Ok(());
            }
        }

        // Slow path: need to load (write lock)
        let mut pool = self.pool.write().await;

        // Double-check after acquiring write lock
        if pool.contains_key(&family) {
            return Ok(());
        }

        // Check if we need to evict a model
        if pool.len() >= self.config.max_loaded_models as usize {
            // Evict the least recently used model
            let lru_family = pool
                .iter()
                .min_by_key(|(_, m)| m.last_used)
                .map(|(f, _)| *f);

            if let Some(evict_family) = lru_family {
                info!(model = %evict_family, "evicting LRU model to load {}", family);
                pool.remove(&evict_family);
                self.model_unloads.fetch_add(1, Ordering::Relaxed);
            }
        }

        // Load the model
        let config = find_model_config(family).ok_or_else(|| InferenceError::NoSuitableModel {
            complexity: "unknown".to_string(),
        })?;

        let loaded = LoadedModel::load(config)?;
        pool.insert(family, loaded);
        self.model_loads.fetch_add(1, Ordering::Relaxed);

        Ok(())
    }

    /// Ensure an agent has a KV cache slot.
    /// Memory cost: ~128 bytes metadata per slot
    async fn ensure_kv_cache(&self, agent_id: &str) -> InferenceResult<()> {
        let mut caches = self.kv_caches.write().await;
        if !caches.contains_key(agent_id) {
            let slot =
                KvCacheSlot::new(agent_id.to_string(), self.config.kv_cache_budget_per_agent);
            caches.insert(agent_id.to_string(), slot);
        }
        Ok(())
    }

    /// Generate tokens (simulated or real).
    /// Memory cost: ~response_length bytes
    async fn generate_tokens(
        &self,
        request: &InferenceRequest,
        model_config: &ModelConfig,
    ) -> InferenceResult<(String, u32)> {
        // Check if real model is loaded
        let pool = self.pool.read().await;
        let model = pool.get(&model_config.family);

        let has_real_weights = model.map(|m| m.weights_loaded).unwrap_or(false);

        if has_real_weights {
            // REAL INFERENCE PATH (when model file exists)
            // In production: run candle transformer forward pass here
            // For now: return realistic simulated output
            drop(pool); // Release read lock

            self.simulate_inference(request, model_config).await
        } else {
            // SIMULATED INFERENCE (graceful degradation when no model file)
            drop(pool);

            self.simulate_inference(request, model_config).await
        }
    }

    /// Simulate inference for testing/demo (when no GGUF model file is available).
    /// Generates a realistic-looking response with proper timing.
    /// Memory cost: ~response_length bytes
    async fn simulate_inference(
        &self,
        request: &InferenceRequest,
        model_config: &ModelConfig,
    ) -> InferenceResult<(String, u32)> {
        let goal = &request.prompt;
        let max_tokens = request.max_tokens.min(512);

        // Build system prompt + user prompt
        let system = request
            .system_prompt
            .as_deref()
            .unwrap_or("You are UGHI, an efficient AI assistant.");

        // Generate simulated response based on task
        let response_text = format!(
            "[{model} | CPU-only | simulated]\n\n\
             System: {system}\n\n\
             Task: {goal}\n\n\
             Analysis:\n\
             This task requires {complexity} reasoning. \
             Using {model} ({params}B params, {quant}-bit quantized) \
             for optimal CPU inference.\n\n\
             Response:\n\
             I would approach this task by:\n\
             1. Breaking down the objective into sub-tasks\n\
             2. Analyzing requirements and constraints\n\
             3. Generating a structured plan\n\
             4. Evaluating alternatives\n\
             5. Delivering actionable results\n\n\
             [Generated {max_tokens} tokens at ~{tps} tok/s on CPU]",
            model = model_config.family,
            system = system,
            goal = goal,
            complexity = request
                .model_override
                .map(|_| "targeted".to_string())
                .unwrap_or_else(|| {
                    let (_, c) = auto_select_model(goal);
                    c.to_string()
                }),
            params = model_config.param_count_b,
            quant = model_config.quant_bits,
            max_tokens = max_tokens,
            tps = 18,
        );

        // Simulate token count (rough: ~4 chars per token)
        let tokens = (response_text.len() as u32 / 4).min(max_tokens);

        // Simulate realistic latency (~18 tok/s)
        let simulated_delay = Duration::from_millis((tokens as u64 * 1000) / 18);
        if simulated_delay.as_millis() > 0 && simulated_delay.as_millis() < 2000 {
            tokio::time::sleep(simulated_delay).await;
        }

        // Update KV cache allocation (simulated)
        {
            let mut caches = self.kv_caches.write().await;
            if let Some(slot) = caches.get_mut(&request.agent_id) {
                // Each token uses ~1 KB of KV cache
                let kv_bytes = tokens as u64 * 1024;
                if slot.can_allocate(kv_bytes) {
                    slot.allocated_bytes += kv_bytes;
                } else {
                    warn!(
                        agent_id = %request.agent_id,
                        used_mb = slot.used_mb(),
                        budget_mb = slot.budget_mb(),
                        "KV cache budget pressure"
                    );
                }
            }
        }

        Ok((response_text, tokens))
    }

    /// Get current inference metrics (lock-free reads).
    /// Memory cost: ~128 bytes (snapshot copy)
    pub async fn metrics(&self) -> InferenceMetrics {
        let pool = self.pool.read().await;
        let caches = self.kv_caches.read().await;

        let model_memory: u64 = pool.values().map(|m| m.config.weight_memory_bytes).sum();

        let kv_total: u64 = caches.values().map(|s| s.allocated_bytes).sum();

        let total_reqs = self.total_requests.load(Ordering::Relaxed);
        let total_toks = self.total_tokens.load(Ordering::Relaxed);

        let avg_tps = if total_reqs > 0 {
            // Approximate: assume ~55ms per token average
            total_toks as f32 / (total_reqs as f32 * 0.055)
        } else {
            0.0
        };

        InferenceMetrics {
            models_loaded: pool.len() as u32,
            model_memory_bytes: model_memory,
            kv_cache_total_bytes: kv_total,
            total_requests: total_reqs,
            total_tokens_generated: total_toks,
            avg_tokens_per_sec: avg_tps,
            active_inferences: self.active_inferences.load(Ordering::Relaxed),
            model_loads: self.model_loads.load(Ordering::Relaxed),
            model_unloads: self.model_unloads.load(Ordering::Relaxed),
        }
    }

    /// Evict idle KV caches and models.
    /// Call periodically from the scheduler.
    /// Memory cost: 0 (frees memory)
    pub async fn evict_idle(&self) {
        let timeout = Duration::from_secs(self.config.idle_unload_timeout_secs);

        // Evict idle KV caches
        {
            let mut caches = self.kv_caches.write().await;
            let idle_agents: Vec<String> = caches
                .iter()
                .filter(|(_, s)| s.idle_duration() > timeout)
                .map(|(id, _)| id.clone())
                .collect();

            for agent_id in &idle_agents {
                caches.remove(agent_id);
                info!(agent_id = %agent_id, "evicted idle KV cache");
            }
        }

        // Evict idle models
        {
            let mut pool = self.pool.write().await;
            let idle_models: Vec<ModelFamily> = pool
                .iter()
                .filter(|(_, m)| m.idle_duration() > timeout)
                .map(|(f, _)| *f)
                .collect();

            for family in &idle_models {
                pool.remove(family);
                self.model_unloads.fetch_add(1, Ordering::Relaxed);
                info!(model = %family, "unloaded idle model");
            }
        }
    }

    /// Release an agent's KV cache (called when agent is killed).
    /// Memory cost: frees KV cache bytes
    pub async fn release_agent(&self, agent_id: &str) {
        let mut caches = self.kv_caches.write().await;
        if caches.remove(agent_id).is_some() {
            info!(agent_id = %agent_id, "released agent KV cache");
        }
    }

    /// Get the number of loaded models.
    pub async fn loaded_model_count(&self) -> usize {
        let pool = self.pool.read().await;
        pool.len()
    }

    /// Get the number of active KV cache slots.
    pub async fn kv_cache_count(&self) -> usize {
        let caches = self.kv_caches.read().await;
        caches.len()
    }

    /// Check if engine is ready for inference.
    pub fn is_ready(&self) -> bool {
        true // Always ready (lazy load)
    }

    /// Shut down the engine, releasing all resources.
    pub async fn shutdown(&self) {
        let mut pool = self.pool.write().await;
        let model_count = pool.len();
        pool.clear();

        let mut caches = self.kv_caches.write().await;
        let cache_count = caches.len();
        caches.clear();

        info!(
            models = model_count,
            caches = cache_count,
            "inference engine shut down"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_engine_creation() {
        let engine = SharedInferenceEngine::with_defaults();
        assert!(engine.is_ready());
        assert_eq!(engine.loaded_model_count().await, 0);
        assert_eq!(engine.kv_cache_count().await, 0);
    }

    #[tokio::test]
    async fn test_lazy_model_loading() {
        let engine = SharedInferenceEngine::with_defaults();

        let req = InferenceRequest::new("What is Rust?", "agent-001");
        let resp = engine.infer(&req).await.unwrap();

        // Model should have been lazy-loaded
        assert!(engine.loaded_model_count().await > 0);
        assert!(resp.tokens_generated > 0);
        assert!(!resp.text.is_empty());
    }

    #[tokio::test]
    async fn test_auto_model_selection() {
        let engine = SharedInferenceEngine::with_defaults();

        // Complex task → Phi-3-mini
        let req = InferenceRequest::new("Mera startup plan banao", "agent-002");
        let resp = engine.infer(&req).await.unwrap();
        assert!(resp.model_used.contains("phi-3"));
        assert_eq!(resp.task_complexity, "complex");

        // Simple task → Qwen 1.5B
        let req = InferenceRequest::new("What is 2+2?", "agent-003");
        let resp = engine.infer(&req).await.unwrap();
        assert!(resp.model_used.contains("qwen2"));
    }

    #[tokio::test]
    async fn test_kv_cache_isolation() {
        let engine = SharedInferenceEngine::with_defaults();

        let req1 = InferenceRequest::new("Task A", "agent-001");
        let req2 = InferenceRequest::new("Task B", "agent-002");

        engine.infer(&req1).await.unwrap();
        engine.infer(&req2).await.unwrap();

        // Each agent should have its own KV cache
        assert_eq!(engine.kv_cache_count().await, 2);
    }

    #[tokio::test]
    async fn test_kv_cache_budget() {
        let config = EngineConfig {
            kv_cache_budget_per_agent: 45 * 1024 * 1024, // 45 MB
            ..Default::default()
        };
        let engine = SharedInferenceEngine::new(config);

        let req = InferenceRequest::new("test", "agent-001");
        engine.infer(&req).await.unwrap();

        let caches = engine.kv_caches.read().await;
        let slot = caches.get("agent-001").unwrap();
        assert!(
            slot.allocated_bytes <= slot.budget_bytes,
            "KV cache {} bytes exceeds budget {} bytes",
            slot.allocated_bytes,
            slot.budget_bytes
        );
    }

    #[tokio::test]
    async fn test_release_agent() {
        let engine = SharedInferenceEngine::with_defaults();

        let req = InferenceRequest::new("test", "agent-release");
        engine.infer(&req).await.unwrap();
        assert_eq!(engine.kv_cache_count().await, 1);

        engine.release_agent("agent-release").await;
        assert_eq!(engine.kv_cache_count().await, 0);
    }

    #[tokio::test]
    async fn test_metrics() {
        let engine = SharedInferenceEngine::with_defaults();

        let req = InferenceRequest::new("metric test", "agent-metrics");
        engine.infer(&req).await.unwrap();

        let metrics = engine.metrics().await;
        assert_eq!(metrics.total_requests, 1);
        assert!(metrics.total_tokens_generated > 0);
        assert!(metrics.models_loaded > 0);
    }

    #[tokio::test]
    async fn test_reflection_mode() {
        let engine = SharedInferenceEngine::with_defaults();

        let req = InferenceRequest::new("analyze this code", "agent-reflect").with_reflection();
        let resp = engine.infer(&req).await.unwrap();

        assert!(resp.reflection.is_some());
        let refl = resp.reflection.unwrap();
        assert!(refl.confidence >= 0.0 && refl.confidence <= 1.0);
    }

    #[tokio::test]
    async fn test_10_agents_concurrent() {
        let engine = SharedInferenceEngine::with_defaults();
        let engine = Arc::new(engine);

        let mut handles = Vec::new();
        for i in 0..10 {
            let eng = engine.clone();
            let handle = tokio::spawn(async move {
                let req = InferenceRequest::new(
                    format!("Task for agent {}", i),
                    format!("agent-{:03}", i),
                );
                eng.infer(&req).await.unwrap()
            });
            handles.push(handle);
        }

        let mut total_tokens = 0u32;
        for handle in handles {
            let resp = handle.await.unwrap();
            total_tokens += resp.tokens_generated;
        }

        // All 10 agents should have completed
        assert_eq!(engine.kv_cache_count().await, 10);
        assert!(total_tokens > 0);

        let metrics = engine.metrics().await;
        assert_eq!(metrics.total_requests, 10);
    }

    #[tokio::test]
    async fn test_shutdown() {
        let engine = SharedInferenceEngine::with_defaults();

        let req = InferenceRequest::new("test", "agent-shutdown");
        engine.infer(&req).await.unwrap();

        engine.shutdown().await;
        assert_eq!(engine.loaded_model_count().await, 0);
        assert_eq!(engine.kv_cache_count().await, 0);
    }
}

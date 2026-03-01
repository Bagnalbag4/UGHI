// UGHI-memory/src/lib.rs
// Follows strict_rules.md | Hierarchical memory: short-term RAM + long-term SQLite
// Memory budget: short-term ≤ 120 MB system-wide | long-term ≤ 280 MB disk
// Per-agent: ~6 MB RAM + ~14 MB disk = ~20 MB total
// 20 agents × 20 MB = ~400 MB total (within 3.2 GB system budget)
// Query latency: < 40 ms (SQLite WAL + in-memory short-term)
// No panic! in core – all errors via MemoryError.
//
// Module structure:
// - error:      MemoryError enum (all recoverable)
// - types:      MemoryEntry, MemoryQuery, SearchResult, MemoryMetrics
// - search:     Pure Rust cosine similarity (no external vector DB)
// - short_term: In-RAM LRU vector store
// - long_term:  File-backed SQLite + BLOB embeddings
// - namespace:  Per-agent isolation registry
// - pruning:    LRU + time-decay auto-pruning, promotion logic

pub mod error;
pub mod long_term;
pub mod migrations;
pub mod namespace;
pub mod pruning;
pub mod search;
pub mod short_term;
pub mod types;

// --- Public re-exports ---
pub use error::{MemoryError, MemoryResult};
pub use long_term::LongTermStore;
pub use namespace::{Namespace, NamespaceRegistry};
pub use pruning::{relevance_score, should_promote, PROMOTION_THRESHOLD};
pub use search::{cosine_similarity, simple_text_embedding};
pub use short_term::ShortTermStore;
pub use types::{MemoryEntry, MemoryMetrics, MemoryQuery, MemoryTier, SearchResult};

/// Unified MemoryStore facade: hierarchical short-term + long-term.
/// Memory cost: ~8 MB base (ShortTermStore + LongTermStore + Registry)
/// skills.md: "MemoryReadWrite – Vector + SQLite" with ≤ 45 MB per call
///
/// Usage:
///   let store = MemoryStore::new("data/memory.db")?;
///   store.put("agent-1", "goal", json!({"plan": "startup"}))?;
///   let results = store.recall("agent-1", "startup strategy")?;
pub struct MemoryStore {
    short: ShortTermStore,
    long: LongTermStore,
    registry: NamespaceRegistry,
}

impl MemoryStore {
    /// Create a new hierarchical memory store.
    /// Memory cost: ~8 MB (SQLite + empty short-term + registry)
    pub fn new(db_path: &str) -> MemoryResult<Self> {
        Ok(Self {
            short: ShortTermStore::new(),
            long: LongTermStore::open(db_path)?,
            registry: NamespaceRegistry::new(),
        })
    }

    /// Create an in-memory store (for tests).
    pub fn in_memory() -> MemoryResult<Self> {
        Self::new(":memory:")
    }

    /// Store a key-value entry with optional embedding.
    /// Goes to short-term first; promoted to long-term after repeated access.
    /// Memory cost: ~entry_size bytes
    pub fn put(&mut self, agent_id: &str, key: &str, value: serde_json::Value) -> MemoryResult<()> {
        let entry = MemoryEntry::new_short(agent_id, key, value);
        let ns = self.registry.ensure(agent_id);
        ns.short_term_entries += 1;
        ns.short_term_bytes += entry.estimated_bytes() as u64;

        self.short.put(entry)
    }

    /// Store with an embedding vector for semantic search.
    pub fn put_with_embedding(
        &mut self,
        agent_id: &str,
        key: &str,
        value: serde_json::Value,
        embedding: Vec<f32>,
    ) -> MemoryResult<()> {
        let mut entry = MemoryEntry::new_short(agent_id, key, value);
        entry.embedding = Some(embedding);
        let ns = self.registry.ensure(agent_id);
        ns.short_term_entries += 1;
        ns.short_term_bytes += entry.estimated_bytes() as u64;

        self.short.put(entry)
    }

    /// Save an entry directly to long-term (persistent across restarts).
    pub fn persist(
        &mut self,
        agent_id: &str,
        key: &str,
        value: serde_json::Value,
    ) -> MemoryResult<()> {
        let entry = MemoryEntry::new_short(agent_id, key, value);
        let ns = self.registry.ensure(agent_id);
        ns.long_term_entries += 1;

        self.long.put(&entry)
    }

    /// Get a specific entry by key (checks short-term first, then long-term).
    /// Memory cost: ~entry_size bytes
    pub fn get(&mut self, agent_id: &str, key: &str) -> MemoryResult<MemoryEntry> {
        // Try short-term first (fast, in-RAM)
        if let Ok(entry) = self.short.get(agent_id, key) {
            return Ok(entry);
        }
        // Fall back to long-term (SQLite)
        self.long.get(agent_id, key)
    }

    /// Semantic recall: search both tiers using text similarity.
    /// Returns top-k results sorted by relevance.
    /// Latency target: < 40 ms
    pub fn recall(&mut self, agent_id: &str, query_text: &str) -> MemoryResult<Vec<SearchResult>> {
        let embedding = simple_text_embedding(query_text, 128);

        let query = MemoryQuery::by_agent(agent_id)
            .with_embedding(embedding)
            .with_limit(10);

        let ns = self.registry.ensure(agent_id);
        ns.total_queries += 1;

        // Search both tiers
        let mut results = self.short.search(&query);
        let long_results = self.long.search(&query)?;
        results.extend(long_results);

        // Sort by similarity descending
        results.sort_by(|a, b| {
            b.similarity
                .unwrap_or(0.0)
                .partial_cmp(&a.similarity.unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        results.truncate(query.limit);
        Ok(results)
    }

    /// Promote frequently accessed short-term entries to long-term.
    /// Call periodically from the scheduler.
    pub fn promote(&mut self, agent_id: &str) -> MemoryResult<u64> {
        let promotable = self.short.get_promotable(agent_id, PROMOTION_THRESHOLD);
        let mut promoted = 0u64;

        for entry in promotable {
            self.long.put(&entry)?;
            promoted += 1;
        }

        if promoted > 0 {
            let ns = self.registry.ensure(agent_id);
            ns.promotions += promoted;
            ns.long_term_entries += promoted;
        }

        Ok(promoted)
    }

    /// Run auto-pruning: evict expired long-term entries.
    pub fn prune(&self) -> MemoryResult<u64> {
        self.long.prune_expired()
    }

    /// Delete all memory for an agent (both tiers).
    pub fn delete_agent(&mut self, agent_id: &str) -> MemoryResult<()> {
        self.short.delete_namespace(agent_id);
        self.long.delete_namespace(agent_id)?;
        self.registry.remove(agent_id);
        Ok(())
    }

    /// Get memory metrics snapshot for the dashboard.
    pub fn metrics(&self) -> MemoryResult<MemoryMetrics> {
        Ok(MemoryMetrics {
            short_term_entries: self.short.total_entries() as u64,
            short_term_bytes: self.short.total_bytes() as u64,
            long_term_entries: self.long.total_entries()?,
            long_term_bytes: self.long.total_disk_bytes()?,
            agents_with_memory: self.registry.count() as u32,
            ..Default::default()
        })
    }

    /// Get namespace info for a specific agent.
    pub fn agent_info(&self, agent_id: &str) -> Option<&Namespace> {
        self.registry.get(agent_id)
    }

    /// Get all namespace snapshots (for dashboard).
    pub fn all_namespaces(&self) -> Vec<&Namespace> {
        self.registry.all()
    }
}

// --- Backward compatibility (used by existing tests) ---

impl MemoryStore {
    /// Backward-compat: usage_bytes for an agent.
    pub fn usage_bytes(&self, agent_id: &str) -> Result<u64, MemoryError> {
        let st = self.short.agent_bytes(agent_id) as u64;
        let lt = self.long.agent_usage_bytes(agent_id)?;
        Ok(st + lt)
    }

    /// Backward-compat: check budget (20 MB total per agent).
    pub fn check_budget(&self, agent_id: &str) -> Result<bool, MemoryError> {
        let total = self.usage_bytes(agent_id)?;
        let limit = 20 * 1024 * 1024; // 20 MB per agent across both tiers
        Ok(total <= limit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hierarchical_put_get() {
        let mut store = MemoryStore::in_memory().unwrap();
        store
            .put("agent-1", "goal", serde_json::json!({"plan": "startup"}))
            .unwrap();

        let entry = store.get("agent-1", "goal").unwrap();
        assert_eq!(entry.value["plan"], "startup");
    }

    #[test]
    fn test_persist_and_recall() {
        let mut store = MemoryStore::in_memory().unwrap();
        store
            .persist("a", "task:startup", serde_json::json!("build MVP"))
            .unwrap();

        let entry = store.get("a", "task:startup").unwrap();
        assert_eq!(entry.value, serde_json::json!("build MVP"));
    }

    #[test]
    fn test_semantic_recall() {
        let mut store = MemoryStore::in_memory().unwrap();

        store
            .put_with_embedding(
                "a",
                "startup",
                serde_json::json!("startup plan"),
                simple_text_embedding("startup plan business", 128),
            )
            .unwrap();

        store
            .put_with_embedding(
                "a",
                "weather",
                serde_json::json!("rainy day"),
                simple_text_embedding("weather forecast rain", 128),
            )
            .unwrap();

        let results = store.recall("a", "startup strategy").unwrap();
        assert!(!results.is_empty());
    }

    #[test]
    fn test_namespace_isolation() {
        let mut store = MemoryStore::in_memory().unwrap();
        store.put("a1", "key", serde_json::json!("val1")).unwrap();
        store.put("a2", "key", serde_json::json!("val2")).unwrap();

        assert_eq!(
            store.get("a1", "key").unwrap().value,
            serde_json::json!("val1")
        );
        assert_eq!(
            store.get("a2", "key").unwrap().value,
            serde_json::json!("val2")
        );
    }

    #[test]
    fn test_delete_agent() {
        let mut store = MemoryStore::in_memory().unwrap();
        store.put("a", "k1", serde_json::json!("v")).unwrap();
        store.persist("a", "k2", serde_json::json!("v")).unwrap();

        store.delete_agent("a").unwrap();
        assert!(store.get("a", "k1").is_err());
        assert!(store.get("a", "k2").is_err());
    }

    #[test]
    fn test_metrics() {
        let mut store = MemoryStore::in_memory().unwrap();
        store.put("a", "k", serde_json::json!("v")).unwrap();

        let m = store.metrics().unwrap();
        assert_eq!(m.short_term_entries, 1);
        assert_eq!(m.agents_with_memory, 1);
    }

    #[test]
    fn test_budget_check() {
        let store = MemoryStore::in_memory().unwrap();
        assert!(store.check_budget("a").unwrap());
    }

    #[test]
    fn test_backward_compat_usage() {
        let mut store = MemoryStore::in_memory().unwrap();
        store
            .put("a", "data", serde_json::json!({"x": "y"}))
            .unwrap();
        let usage = store.usage_bytes("a").unwrap();
        assert!(usage > 0);
    }
}

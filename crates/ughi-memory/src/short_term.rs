// UGHI-memory/src/short_term.rs
// Follows strict_rules.md | System-wide ≤ 120 MB RAM
// Per-agent short-term limit: ~6 MB
// In-RAM vector store with LRU eviction and cosine similarity search.

use std::collections::HashMap;
use tracing::{info, warn};

use crate::error::{MemoryError, MemoryResult};
use crate::search::cosine_similarity;
use crate::types::{MemoryEntry, MemoryQuery, MemoryTier, SearchResult};

/// System-wide short-term memory budget (120 MB).
const SYSTEM_BUDGET_BYTES: usize = 120 * 1024 * 1024;

/// Per-agent short-term memory budget (~6 MB).
const AGENT_BUDGET_BYTES: usize = 6 * 1024 * 1024;

/// Short-term in-RAM memory store.
/// Memory cost: proportional to stored entries (≤ 120 MB system-wide)
/// Uses HashMap<agent_id, Vec<MemoryEntry>> for O(1) namespace lookup.
pub struct ShortTermStore {
    /// Per-agent entry storage
    entries: HashMap<String, Vec<MemoryEntry>>,
    /// Total bytes used (tracked, not measured)
    total_bytes: usize,
    /// Max entries per agent before LRU eviction
    max_entries_per_agent: usize,
}

impl ShortTermStore {
    /// Create a new short-term store.
    /// Memory cost: ~1 KB (empty HashMap)
    pub fn new() -> Self {
        Self {
            entries: HashMap::with_capacity(50),
            total_bytes: 0,
            max_entries_per_agent: 500,
        }
    }

    /// Store an entry in short-term memory.
    /// Memory cost: entry.estimated_bytes()
    /// Evicts LRU entries if agent or system budget exceeded.
    pub fn put(&mut self, mut entry: MemoryEntry) -> MemoryResult<()> {
        entry.tier = MemoryTier::ShortTerm;
        let entry_size = entry.estimated_bytes();

        // System budget check
        if self.total_bytes + entry_size > SYSTEM_BUDGET_BYTES {
            self.evict_system_lru(entry_size);
        }

        let agent_entries = self
            .entries
            .entry(entry.agent_id.clone())
            .or_insert_with(|| Vec::with_capacity(64));

        // Agent budget: check total bytes for this agent
        let agent_bytes: usize = agent_entries.iter().map(|e| e.estimated_bytes()).sum();
        if agent_bytes + entry_size > AGENT_BUDGET_BYTES {
            self.evict_agent_lru(&entry.agent_id);
        }

        // Check for existing key (upsert)
        let agent_entries = self.entries.get_mut(&entry.agent_id).unwrap();
        if let Some(existing) = agent_entries.iter_mut().find(|e| e.key == entry.key) {
            self.total_bytes -= existing.estimated_bytes();
            *existing = entry;
            self.total_bytes += entry_size;
        } else {
            // Max entries eviction
            if agent_entries.len() >= self.max_entries_per_agent {
                // Remove least recently accessed
                if let Some(lru_idx) = agent_entries
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, e)| e.last_accessed)
                    .map(|(i, _)| i)
                {
                    self.total_bytes -= agent_entries[lru_idx].estimated_bytes();
                    agent_entries.swap_remove(lru_idx);
                }
            }
            self.total_bytes += entry_size;
            agent_entries.push(entry);
        }

        Ok(())
    }

    /// Get an entry by agent_id and key.
    /// Memory cost: 0 (returns reference via clone)
    pub fn get(&mut self, agent_id: &str, key: &str) -> MemoryResult<MemoryEntry> {
        let entries = self
            .entries
            .get_mut(agent_id)
            .ok_or_else(|| MemoryError::KeyNotFound {
                key: key.to_string(),
            })?;

        let entry =
            entries
                .iter_mut()
                .find(|e| e.key == key)
                .ok_or_else(|| MemoryError::KeyNotFound {
                    key: key.to_string(),
                })?;

        entry.touch();
        Ok(entry.clone())
    }

    /// Search short-term memory using cosine similarity.
    /// Memory cost: O(n) where n = entries for this agent
    /// Latency: O(n * d) where d = embedding dimension
    pub fn search(&mut self, query: &MemoryQuery) -> Vec<SearchResult> {
        let entries = match self.entries.get_mut(&query.agent_id) {
            Some(e) => e,
            None => return Vec::new(),
        };

        let mut results: Vec<SearchResult> = Vec::new();

        for entry in entries.iter_mut() {
            // Key prefix filter
            if let Some(ref prefix) = query.key_prefix {
                if !entry.key.starts_with(prefix) {
                    continue;
                }
            }

            // Semantic similarity filter
            let similarity = if let (Some(ref query_emb), Some(ref entry_emb)) =
                (&query.embedding, &entry.embedding)
            {
                let sim = cosine_similarity(query_emb, entry_emb);
                if sim < query.min_similarity {
                    continue;
                }
                Some(sim)
            } else {
                None
            };

            entry.touch();
            results.push(SearchResult {
                entry: entry.clone(),
                similarity,
            });
        }

        // Sort by similarity (desc) then by last_accessed (desc)
        results.sort_by(|a, b| {
            let sim_cmp = b
                .similarity
                .unwrap_or(0.0)
                .partial_cmp(&a.similarity.unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal);
            if sim_cmp != std::cmp::Ordering::Equal {
                return sim_cmp;
            }
            b.entry.last_accessed.cmp(&a.entry.last_accessed)
        });

        results.truncate(query.limit);
        results
    }

    /// Delete an agent's entire namespace.
    pub fn delete_namespace(&mut self, agent_id: &str) {
        if let Some(entries) = self.entries.remove(agent_id) {
            let freed: usize = entries.iter().map(|e| e.estimated_bytes()).sum();
            self.total_bytes -= freed;
            info!(
                agent_id,
                freed_bytes = freed,
                "cleared short-term namespace"
            );
        }
    }

    /// Get entries eligible for promotion to long-term (access_count >= threshold).
    pub fn get_promotable(&self, agent_id: &str, min_access_count: u32) -> Vec<MemoryEntry> {
        self.entries
            .get(agent_id)
            .map(|entries| {
                entries
                    .iter()
                    .filter(|e| e.access_count >= min_access_count)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Total bytes used across all agents.
    pub fn total_bytes(&self) -> usize {
        self.total_bytes
    }

    /// Total entries across all agents.
    pub fn total_entries(&self) -> usize {
        self.entries.values().map(|v| v.len()).sum()
    }

    /// Number of agents with short-term memory.
    pub fn agent_count(&self) -> usize {
        self.entries.len()
    }

    /// Bytes used by a specific agent.
    pub fn agent_bytes(&self, agent_id: &str) -> usize {
        self.entries
            .get(agent_id)
            .map(|entries| entries.iter().map(|e| e.estimated_bytes()).sum())
            .unwrap_or(0)
    }

    /// Evict least-recently-used entries system-wide to free target_bytes.
    fn evict_system_lru(&mut self, target_bytes: usize) {
        let mut freed = 0usize;
        // Collect all (agent_id, entry_idx, last_accessed) tuples
        let mut candidates: Vec<(String, usize, u64)> = Vec::new();
        for (agent_id, entries) in &self.entries {
            for (idx, entry) in entries.iter().enumerate() {
                candidates.push((agent_id.clone(), idx, entry.last_accessed));
            }
        }
        // Sort by last_accessed ascending (oldest first)
        candidates.sort_by_key(|(_, _, t)| *t);

        for (agent_id, _, _) in &candidates {
            if freed >= target_bytes {
                break;
            }
            if let Some(entries) = self.entries.get_mut(agent_id) {
                if let Some(lru_idx) = entries
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, e)| e.last_accessed)
                    .map(|(i, _)| i)
                {
                    freed += entries[lru_idx].estimated_bytes();
                    entries.swap_remove(lru_idx);
                }
            }
        }
        self.total_bytes = self.total_bytes.saturating_sub(freed);
        warn!(freed_bytes = freed, "system-wide LRU eviction");
    }

    /// Evict LRU entries for a specific agent.
    fn evict_agent_lru(&mut self, agent_id: &str) {
        if let Some(entries) = self.entries.get_mut(agent_id) {
            // Remove oldest 25% of entries
            let to_remove = (entries.len() / 4).max(1);
            entries.sort_by_key(|e| e.last_accessed);
            let mut freed = 0usize;
            for _ in 0..to_remove {
                if entries.is_empty() {
                    break;
                }
                freed += entries[0].estimated_bytes();
                entries.remove(0);
            }
            self.total_bytes = self.total_bytes.saturating_sub(freed);
            info!(
                agent_id,
                freed_bytes = freed,
                evicted = to_remove,
                "agent LRU eviction"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_put_and_get() {
        let mut store = ShortTermStore::new();
        let entry = MemoryEntry::new_short("agent-1", "task", serde_json::json!({"goal": "test"}));
        store.put(entry).unwrap();

        let result = store.get("agent-1", "task").unwrap();
        assert_eq!(result.value["goal"], "test");
    }

    #[test]
    fn test_upsert() {
        let mut store = ShortTermStore::new();
        store
            .put(MemoryEntry::new_short("a", "k", serde_json::json!("v1")))
            .unwrap();
        store
            .put(MemoryEntry::new_short("a", "k", serde_json::json!("v2")))
            .unwrap();

        let result = store.get("a", "k").unwrap();
        assert_eq!(result.value, serde_json::json!("v2"));
        assert_eq!(store.total_entries(), 1);
    }

    #[test]
    fn test_namespace_isolation() {
        let mut store = ShortTermStore::new();
        store
            .put(MemoryEntry::new_short(
                "agent-1",
                "key",
                serde_json::json!("a1"),
            ))
            .unwrap();
        store
            .put(MemoryEntry::new_short(
                "agent-2",
                "key",
                serde_json::json!("a2"),
            ))
            .unwrap();

        assert_eq!(
            store.get("agent-1", "key").unwrap().value,
            serde_json::json!("a1")
        );
        assert_eq!(
            store.get("agent-2", "key").unwrap().value,
            serde_json::json!("a2")
        );
    }

    #[test]
    fn test_delete_namespace() {
        let mut store = ShortTermStore::new();
        store
            .put(MemoryEntry::new_short(
                "agent-1",
                "k1",
                serde_json::json!("v"),
            ))
            .unwrap();
        store
            .put(MemoryEntry::new_short(
                "agent-1",
                "k2",
                serde_json::json!("v"),
            ))
            .unwrap();

        store.delete_namespace("agent-1");
        assert_eq!(store.agent_count(), 0);
    }

    #[test]
    fn test_search_by_prefix() {
        let mut store = ShortTermStore::new();
        store
            .put(MemoryEntry::new_short(
                "a",
                "task:1",
                serde_json::json!("t1"),
            ))
            .unwrap();
        store
            .put(MemoryEntry::new_short(
                "a",
                "task:2",
                serde_json::json!("t2"),
            ))
            .unwrap();
        store
            .put(MemoryEntry::new_short(
                "a",
                "note:1",
                serde_json::json!("n1"),
            ))
            .unwrap();

        let q = MemoryQuery::by_agent("a").with_key_prefix("task:");
        let results = store.search(&q);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_semantic_search() {
        use crate::search::simple_text_embedding;

        let mut store = ShortTermStore::new();

        let mut e1 = MemoryEntry::new_short("a", "startup", serde_json::json!("startup plan"));
        e1.embedding = Some(simple_text_embedding("startup plan business", 128));
        store.put(e1).unwrap();

        let mut e2 = MemoryEntry::new_short("a", "weather", serde_json::json!("rainy day"));
        e2.embedding = Some(simple_text_embedding("weather forecast rain", 128));
        store.put(e2).unwrap();

        let q = MemoryQuery::by_agent("a")
            .with_embedding(simple_text_embedding("startup strategy", 128));
        let results = store.search(&q);
        assert!(!results.is_empty());
        // First result should be more related to startup
        assert_eq!(results[0].entry.key, "startup");
    }

    #[test]
    fn test_promotable() {
        let mut store = ShortTermStore::new();
        store
            .put(MemoryEntry::new_short("a", "hot", serde_json::json!("x")))
            .unwrap();
        // Access multiple times to make it promotable
        for _ in 0..5 {
            store.get("a", "hot").unwrap();
        }
        store
            .put(MemoryEntry::new_short("a", "cold", serde_json::json!("y")))
            .unwrap();

        let promotable = store.get_promotable("a", 3);
        assert_eq!(promotable.len(), 1);
        assert_eq!(promotable[0].key, "hot");
    }
}

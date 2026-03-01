// UGHI-memory/src/types.rs
// Follows strict_rules.md | Memory cost annotations on all types
// Core data types for the hierarchical memory system.

use serde::{Deserialize, Serialize};

/// Memory tier: short-term (RAM) or long-term (SQLite disk).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryTier {
    ShortTerm,
    LongTerm,
}

/// A memory entry in the hierarchical store.
/// Memory cost: ~key_len + value_len + embedding_len*4 + 128 bytes overhead
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub agent_id: String,
    pub key: String,
    pub value: serde_json::Value,
    /// Optional embedding vector for semantic search (dimension: 128 or 384)
    pub embedding: Option<Vec<f32>>,
    pub created_at: u64,
    /// Last access timestamp (Unix ms) – used for LRU
    pub last_accessed: u64,
    /// Access count – used for promotion and pruning
    pub access_count: u32,
    /// Which tier this entry lives in
    pub tier: MemoryTier,
}

impl MemoryEntry {
    /// Create a new short-term entry.
    pub fn new_short(
        agent_id: impl Into<String>,
        key: impl Into<String>,
        value: serde_json::Value,
    ) -> Self {
        let now = current_time_ms();
        Self {
            agent_id: agent_id.into(),
            key: key.into(),
            value,
            embedding: None,
            created_at: now,
            last_accessed: now,
            access_count: 0,
            tier: MemoryTier::ShortTerm,
        }
    }

    /// Estimated memory size in bytes.
    pub fn estimated_bytes(&self) -> usize {
        let val_size = serde_json::to_string(&self.value)
            .map(|s| s.len())
            .unwrap_or(32);
        let emb_size = self.embedding.as_ref().map(|v| v.len() * 4).unwrap_or(0);
        self.agent_id.len() + self.key.len() + val_size + emb_size + 64
    }

    /// Touch the access timestamp and increment count.
    pub fn touch(&mut self) {
        self.last_accessed = current_time_ms();
        self.access_count += 1;
    }

    /// Age in milliseconds since creation.
    pub fn age_ms(&self) -> u64 {
        current_time_ms().saturating_sub(self.created_at)
    }

    /// Idle time since last access in milliseconds.
    pub fn idle_ms(&self) -> u64 {
        current_time_ms().saturating_sub(self.last_accessed)
    }
}

/// Query parameters for searching memory.
#[derive(Debug, Clone)]
pub struct MemoryQuery {
    /// Agent ID to search within (namespace isolation)
    pub agent_id: String,
    /// Optional key prefix filter
    pub key_prefix: Option<String>,
    /// Optional embedding vector for semantic similarity search
    pub embedding: Option<Vec<f32>>,
    /// Max results to return
    pub limit: usize,
    /// Minimum similarity threshold (0.0–1.0) for semantic search
    pub min_similarity: f32,
    /// Search both tiers or specific tier
    pub tier: Option<MemoryTier>,
}

impl MemoryQuery {
    pub fn by_agent(agent_id: impl Into<String>) -> Self {
        Self {
            agent_id: agent_id.into(),
            key_prefix: None,
            embedding: None,
            limit: 10,
            min_similarity: 0.5,
            tier: None,
        }
    }

    pub fn with_key_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.key_prefix = Some(prefix.into());
        self
    }

    pub fn with_embedding(mut self, emb: Vec<f32>) -> Self {
        self.embedding = Some(emb);
        self
    }

    pub fn with_limit(mut self, n: usize) -> Self {
        self.limit = n;
        self
    }
}

/// Search result with similarity score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub entry: MemoryEntry,
    /// Cosine similarity score (0.0–1.0), None for non-semantic queries
    pub similarity: Option<f32>,
}

/// Memory metrics for the dashboard.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryMetrics {
    pub short_term_entries: u64,
    pub short_term_bytes: u64,
    pub long_term_entries: u64,
    pub long_term_bytes: u64,
    pub total_queries: u64,
    pub avg_query_ms: f64,
    pub agents_with_memory: u32,
    pub pruned_entries: u64,
    pub promotions: u64,
}

impl std::fmt::Display for MemoryMetrics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "st={} ({:.1}MB) lt={} ({:.1}MB) agents={} queries={}",
            self.short_term_entries,
            self.short_term_bytes as f64 / (1024.0 * 1024.0),
            self.long_term_entries,
            self.long_term_bytes as f64 / (1024.0 * 1024.0),
            self.agents_with_memory,
            self.total_queries,
        )
    }
}

/// Get current time in milliseconds.
pub fn current_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entry_creation() {
        let entry = MemoryEntry::new_short("agent-1", "task", serde_json::json!({"goal": "test"}));
        assert_eq!(entry.agent_id, "agent-1");
        assert_eq!(entry.tier, MemoryTier::ShortTerm);
        assert!(entry.estimated_bytes() > 0);
    }

    #[test]
    fn test_entry_touch() {
        let mut entry = MemoryEntry::new_short("a", "k", serde_json::json!(null));
        assert_eq!(entry.access_count, 0);
        entry.touch();
        assert_eq!(entry.access_count, 1);
    }

    #[test]
    fn test_query_builder() {
        let q = MemoryQuery::by_agent("agent-1")
            .with_key_prefix("task:")
            .with_limit(5);
        assert_eq!(q.limit, 5);
        assert_eq!(q.key_prefix, Some("task:".into()));
    }
}

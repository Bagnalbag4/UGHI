// UGHI-memory/src/namespace.rs
// Follows strict_rules.md | Zero trust: every agent starts with zero capabilities
// Per-agent namespace isolation. No cross-agent memory leaks.
// Memory cost: ~1 KB per namespace (metadata only)

use serde::{Deserialize, Serialize};

use crate::types::current_time_ms;

/// Namespace metadata for an agent's memory partition.
/// Memory cost: ~256 bytes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Namespace {
    pub agent_id: String,
    pub created_at: u64,
    pub short_term_entries: u64,
    pub long_term_entries: u64,
    pub short_term_bytes: u64,
    pub long_term_bytes: u64,
    pub total_queries: u64,
    pub promotions: u64,
}

impl Namespace {
    /// Create a new namespace for an agent.
    pub fn new(agent_id: impl Into<String>) -> Self {
        Self {
            agent_id: agent_id.into(),
            created_at: current_time_ms(),
            short_term_entries: 0,
            long_term_entries: 0,
            short_term_bytes: 0,
            long_term_bytes: 0,
            total_queries: 0,
            promotions: 0,
        }
    }

    /// Total entries across both tiers.
    pub fn total_entries(&self) -> u64 {
        self.short_term_entries + self.long_term_entries
    }

    /// Total bytes across both tiers.
    pub fn total_bytes(&self) -> u64 {
        self.short_term_bytes + self.long_term_bytes
    }
}

/// Namespace registry: tracks all agent memory partitions.
/// Memory cost: ~256 bytes per agent
pub struct NamespaceRegistry {
    namespaces: std::collections::HashMap<String, Namespace>,
}

impl NamespaceRegistry {
    pub fn new() -> Self {
        Self {
            namespaces: std::collections::HashMap::with_capacity(50),
        }
    }

    /// Get or create a namespace for an agent.
    pub fn ensure(&mut self, agent_id: &str) -> &mut Namespace {
        self.namespaces
            .entry(agent_id.to_string())
            .or_insert_with(|| Namespace::new(agent_id))
    }

    /// Get namespace if it exists.
    pub fn get(&self, agent_id: &str) -> Option<&Namespace> {
        self.namespaces.get(agent_id)
    }

    /// Remove a namespace (agent killed).
    pub fn remove(&mut self, agent_id: &str) -> Option<Namespace> {
        self.namespaces.remove(agent_id)
    }

    /// All namespace snapshots (for dashboard).
    pub fn all(&self) -> Vec<&Namespace> {
        self.namespaces.values().collect()
    }

    /// Number of tracked agents.
    pub fn count(&self) -> usize {
        self.namespaces.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_namespace_creation() {
        let mut reg = NamespaceRegistry::new();
        let ns = reg.ensure("agent-1");
        assert_eq!(ns.agent_id, "agent-1");
        assert_eq!(ns.total_entries(), 0);
    }

    #[test]
    fn test_namespace_isolation() {
        let mut reg = NamespaceRegistry::new();
        reg.ensure("a1").short_term_entries = 5;
        reg.ensure("a2").short_term_entries = 10;

        assert_eq!(reg.get("a1").unwrap().short_term_entries, 5);
        assert_eq!(reg.get("a2").unwrap().short_term_entries, 10);
    }

    #[test]
    fn test_namespace_removal() {
        let mut reg = NamespaceRegistry::new();
        reg.ensure("a");
        assert_eq!(reg.count(), 1);
        reg.remove("a");
        assert_eq!(reg.count(), 0);
    }
}

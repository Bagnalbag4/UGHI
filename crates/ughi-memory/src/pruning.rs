// UGHI-memory/src/pruning.rs
// Follows strict_rules.md | Memory cost: O(1)
// Auto-pruning: LRU eviction + time-decay scoring
// Entries with high access + recent use survive longest.

use crate::types::{current_time_ms, MemoryEntry};

/// Decay factor (halves relevance every 7 days).
const HALF_LIFE_MS: f64 = 7.0 * 24.0 * 60.0 * 60.0 * 1000.0;

/// Promotion threshold: entries accessed >= this count get promoted to long-term.
pub const PROMOTION_THRESHOLD: u32 = 3;

/// Calculate the relevance score for a memory entry.
/// Combines access frequency and time decay.
/// Score = access_count * decay_factor
/// where decay_factor = 2^(-age / half_life)
/// Memory cost: 0
pub fn relevance_score(entry: &MemoryEntry) -> f64 {
    let age_ms = current_time_ms().saturating_sub(entry.last_accessed) as f64;
    let decay = (-(age_ms / HALF_LIFE_MS) * std::f64::consts::LN_2).exp();
    (entry.access_count as f64 + 1.0) * decay
}

/// Check if an entry should be promoted from short-term to long-term.
/// Criteria: access_count >= threshold
pub fn should_promote(entry: &MemoryEntry) -> bool {
    entry.access_count >= PROMOTION_THRESHOLD
}

/// Check if an entry has expired (older than max_age_ms).
pub fn is_expired(entry: &MemoryEntry, max_age_ms: u64) -> bool {
    entry.age_ms() > max_age_ms
}

/// Sort entries by relevance (highest first) for pruning decisions.
/// Entries with lowest relevance are pruned first.
pub fn sort_by_relevance(entries: &mut [MemoryEntry]) {
    entries.sort_by(|a, b| {
        relevance_score(b)
            .partial_cmp(&relevance_score(a))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relevance_recent_higher() {
        let mut recent = MemoryEntry::new_short("a", "k1", serde_json::json!(null));
        recent.access_count = 5;
        recent.last_accessed = current_time_ms();

        let mut old = MemoryEntry::new_short("a", "k2", serde_json::json!(null));
        old.access_count = 5;
        old.last_accessed = current_time_ms() - (14 * 24 * 60 * 60 * 1000); // 14 days ago

        assert!(relevance_score(&recent) > relevance_score(&old));
    }

    #[test]
    fn test_promotion_threshold() {
        let mut entry = MemoryEntry::new_short("a", "k", serde_json::json!(null));
        assert!(!should_promote(&entry));

        entry.access_count = PROMOTION_THRESHOLD;
        assert!(should_promote(&entry));
    }

    #[test]
    fn test_sort_by_relevance() {
        let mut entries = vec![
            {
                let mut e = MemoryEntry::new_short("a", "low", serde_json::json!(null));
                e.access_count = 1;
                e.last_accessed = current_time_ms() - (30 * 24 * 60 * 60 * 1000);
                e
            },
            {
                let mut e = MemoryEntry::new_short("a", "high", serde_json::json!(null));
                e.access_count = 10;
                e.last_accessed = current_time_ms();
                e
            },
        ];

        sort_by_relevance(&mut entries);
        assert_eq!(entries[0].key, "high"); // Most relevant first
    }
}

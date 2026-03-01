// UGHI-wasm/src/violation.rs
// Follows strict_rules.md | strict_rules.md #7: auto-recover on crash
// Memory cost: ~128 bytes per violation record
// Tracks security violations per agent, auto-quarantines after threshold.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{error, warn};

/// Maximum violations before an agent is quarantined.
const QUARANTINE_THRESHOLD: u32 = 5;

/// A recorded security violation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Violation {
    pub agent_id: String,
    pub skill: String,
    pub reason: String,
    pub timestamp_ms: u64,
}

/// Violation tracker: monitors security violations per agent.
/// Memory cost: ~128 bytes per violation record
pub struct ViolationTracker {
    /// Per-agent violation history
    violations: HashMap<String, Vec<Violation>>,
    /// Quarantined agents (cannot execute any skills)
    quarantined: HashMap<String, u64>,
    /// Total violations across all agents
    total_violations: u64,
}

impl ViolationTracker {
    pub fn new() -> Self {
        Self {
            violations: HashMap::with_capacity(32),
            quarantined: HashMap::new(),
            total_violations: 0,
        }
    }

    /// Record a security violation.
    /// Returns true if agent is now quarantined.
    pub fn record(&mut self, agent_id: &str, skill: &str, reason: &str) -> bool {
        let now = current_time_ms();
        let violation = Violation {
            agent_id: agent_id.to_string(),
            skill: skill.to_string(),
            reason: reason.to_string(),
            timestamp_ms: now,
        };

        self.total_violations += 1;
        let history = self
            .violations
            .entry(agent_id.to_string())
            .or_insert_with(Vec::new);
        history.push(violation);

        let count = history.len() as u32;
        warn!(
            agent_id,
            skill, reason, count, "security violation recorded"
        );

        if count >= QUARANTINE_THRESHOLD {
            self.quarantined.insert(agent_id.to_string(), now);
            error!(agent_id, count, "agent QUARANTINED – too many violations");
            return true;
        }
        false
    }

    /// Check if an agent is quarantined.
    pub fn is_quarantined(&self, agent_id: &str) -> bool {
        self.quarantined.contains_key(agent_id)
    }

    /// Release an agent from quarantine (manual override).
    pub fn release(&mut self, agent_id: &str) {
        self.quarantined.remove(agent_id);
        self.violations.remove(agent_id);
    }

    /// Get violation count for an agent.
    pub fn agent_violation_count(&self, agent_id: &str) -> u32 {
        self.violations
            .get(agent_id)
            .map(|v| v.len() as u32)
            .unwrap_or(0)
    }

    /// Get violation history for an agent.
    pub fn agent_violations(&self, agent_id: &str) -> Vec<&Violation> {
        self.violations
            .get(agent_id)
            .map(|v| v.iter().collect())
            .unwrap_or_default()
    }

    /// Metrics snapshot.
    pub fn metrics(&self) -> ViolationMetrics {
        ViolationMetrics {
            total_violations: self.total_violations,
            quarantined_agents: self.quarantined.len() as u32,
            agents_with_violations: self.violations.len() as u32,
        }
    }
}

/// Violation metrics for the dashboard.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ViolationMetrics {
    pub total_violations: u64,
    pub quarantined_agents: u32,
    pub agents_with_violations: u32,
}

fn current_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_start() {
        let tracker = ViolationTracker::new();
        assert!(!tracker.is_quarantined("agent-1"));
        assert_eq!(tracker.agent_violation_count("agent-1"), 0);
    }

    #[test]
    fn test_record_violation() {
        let mut tracker = ViolationTracker::new();
        let quarantined = tracker.record("a", "web_search", "unauthorized access");
        assert!(!quarantined);
        assert_eq!(tracker.agent_violation_count("a"), 1);
    }

    #[test]
    fn test_quarantine_after_threshold() {
        let mut tracker = ViolationTracker::new();
        for i in 0..QUARANTINE_THRESHOLD {
            let q = tracker.record("a", "s", &format!("violation {}", i));
            if i < QUARANTINE_THRESHOLD - 1 {
                assert!(!q);
            } else {
                assert!(q, "should be quarantined at threshold");
            }
        }
        assert!(tracker.is_quarantined("a"));
    }

    #[test]
    fn test_release_quarantine() {
        let mut tracker = ViolationTracker::new();
        for i in 0..QUARANTINE_THRESHOLD {
            tracker.record("a", "s", &format!("v{}", i));
        }
        assert!(tracker.is_quarantined("a"));

        tracker.release("a");
        assert!(!tracker.is_quarantined("a"));
        assert_eq!(tracker.agent_violation_count("a"), 0);
    }

    #[test]
    fn test_agent_isolation() {
        let mut tracker = ViolationTracker::new();
        tracker.record("a1", "s", "bad");
        assert_eq!(tracker.agent_violation_count("a1"), 1);
        assert_eq!(tracker.agent_violation_count("a2"), 0);
    }
}

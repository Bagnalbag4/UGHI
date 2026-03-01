// UGHI-runtime/src/healing.rs
// Follows strict_rules.md | Rule #6: "All errors must be recoverable"
// Self-healing: crash detection, auto-restart, root-cause tracking
// Memory cost: ~2 KB per tracked agent
// Integrates with runtime for automatic agent recovery.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{error, info, warn};

/// Maximum restart attempts before giving up.
const MAX_RESTARTS: u32 = 3;

/// Cooldown between restarts (ms).
#[allow(dead_code)]
const RESTART_COOLDOWN_MS: u64 = 1000;

/// A crash record for an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashRecord {
    pub agent_id: String,
    pub reason: String,
    pub timestamp_ms: u64,
    pub recovered: bool,
    pub restart_count: u32,
}

/// Root cause categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RootCause {
    MemoryExceeded,
    Timeout,
    SandboxViolation,
    InternalError,
    ExternalDependency,
    Unknown,
}

impl RootCause {
    /// Classify a crash reason into a root cause.
    pub fn classify(reason: &str) -> Self {
        let r = reason.to_lowercase();
        if r.contains("memory") || r.contains("oom") || r.contains("budget") {
            RootCause::MemoryExceeded
        } else if r.contains("timeout") || r.contains("deadline") {
            RootCause::Timeout
        } else if r.contains("sandbox") || r.contains("capability") || r.contains("violation") {
            RootCause::SandboxViolation
        } else if r.contains("network") || r.contains("connection") || r.contains("api") {
            RootCause::ExternalDependency
        } else if r.contains("panic") || r.contains("internal") {
            RootCause::InternalError
        } else {
            RootCause::Unknown
        }
    }
}

/// Healing action to apply.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HealingAction {
    Restart,
    RestartWithReducedMemory { new_limit_mb: u64 },
    RestartWithLowerPriority,
    Quarantine,
    NotifyOperator { message: String },
}

/// Self-healing manager.
/// Memory cost: ~2 KB per tracked agent
pub struct SelfHealingManager {
    /// Per-agent crash history
    crash_history: HashMap<String, Vec<CrashRecord>>,
    /// Per-agent restart counts (resets on success)
    restart_counts: HashMap<String, u32>,
    /// Total crashes observed
    total_crashes: u64,
    /// Total successful recoveries
    total_recoveries: u64,
}

impl SelfHealingManager {
    pub fn new() -> Self {
        Self {
            crash_history: HashMap::with_capacity(32),
            restart_counts: HashMap::with_capacity(32),
            total_crashes: 0,
            total_recoveries: 0,
        }
    }

    /// Record a crash and determine healing action.
    /// Returns the action to take for recovery.
    pub fn on_crash(&mut self, agent_id: &str, reason: &str) -> HealingAction {
        let now = current_time_ms();
        self.total_crashes += 1;

        let restart_count = self.restart_counts.entry(agent_id.to_string()).or_insert(0);
        *restart_count += 1;

        let record = CrashRecord {
            agent_id: agent_id.to_string(),
            reason: reason.to_string(),
            timestamp_ms: now,
            recovered: false,
            restart_count: *restart_count,
        };

        self.crash_history
            .entry(agent_id.to_string())
            .or_insert_with(Vec::new)
            .push(record);

        let root_cause = RootCause::classify(reason);
        let count = *restart_count;

        warn!(agent_id, reason, ?root_cause, count, "agent crash detected");

        // Determine healing action based on root cause and restart count
        if count > MAX_RESTARTS {
            error!(agent_id, count, "max restarts exceeded – quarantining");
            return HealingAction::Quarantine;
        }

        match root_cause {
            RootCause::MemoryExceeded => {
                HealingAction::RestartWithReducedMemory {
                    new_limit_mb: 100, // Reduce from 140 MB to 100 MB
                }
            }
            RootCause::Timeout => HealingAction::RestartWithLowerPriority,
            RootCause::SandboxViolation => HealingAction::Quarantine,
            _ => HealingAction::Restart,
        }
    }

    /// Mark an agent as successfully recovered.
    pub fn on_recovery(&mut self, agent_id: &str) {
        self.total_recoveries += 1;
        // Reset restart count on successful recovery
        self.restart_counts.remove(agent_id);

        if let Some(records) = self.crash_history.get_mut(agent_id) {
            if let Some(last) = records.last_mut() {
                last.recovered = true;
            }
        }

        info!(agent_id, "agent recovered successfully");
    }

    /// Get crash count for an agent.
    pub fn crash_count(&self, agent_id: &str) -> u32 {
        self.crash_history
            .get(agent_id)
            .map(|v| v.len() as u32)
            .unwrap_or(0)
    }

    /// Get crash history for an agent.
    pub fn history(&self, agent_id: &str) -> Vec<&CrashRecord> {
        self.crash_history
            .get(agent_id)
            .map(|v| v.iter().collect())
            .unwrap_or_default()
    }

    /// Metrics snapshot.
    pub fn metrics(&self) -> HealingMetrics {
        HealingMetrics {
            total_crashes: self.total_crashes,
            total_recoveries: self.total_recoveries,
            agents_tracked: self.crash_history.len() as u32,
            recovery_rate: if self.total_crashes > 0 {
                self.total_recoveries as f64 / self.total_crashes as f64
            } else {
                1.0
            },
        }
    }
}

/// Healing metrics for dashboard.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HealingMetrics {
    pub total_crashes: u64,
    pub total_recoveries: u64,
    pub agents_tracked: u32,
    pub recovery_rate: f64,
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
        let mgr = SelfHealingManager::new();
        assert_eq!(mgr.crash_count("a"), 0);
    }

    #[test]
    fn test_crash_triggers_restart() {
        let mut mgr = SelfHealingManager::new();
        let action = mgr.on_crash("a", "internal error");
        assert!(matches!(action, HealingAction::Restart));
    }

    #[test]
    fn test_memory_crash_reduces_limit() {
        let mut mgr = SelfHealingManager::new();
        let action = mgr.on_crash("a", "memory budget exceeded");
        assert!(matches!(
            action,
            HealingAction::RestartWithReducedMemory { .. }
        ));
    }

    #[test]
    fn test_sandbox_violation_quarantines() {
        let mut mgr = SelfHealingManager::new();
        let action = mgr.on_crash("a", "sandbox capability violation");
        assert!(matches!(action, HealingAction::Quarantine));
    }

    #[test]
    fn test_max_restarts_quarantine() {
        let mut mgr = SelfHealingManager::new();
        for _ in 0..MAX_RESTARTS {
            let _ = mgr.on_crash("a", "something");
        }
        let action = mgr.on_crash("a", "something again");
        assert!(matches!(action, HealingAction::Quarantine));
    }

    #[test]
    fn test_recovery_resets_count() {
        let mut mgr = SelfHealingManager::new();
        mgr.on_crash("a", "error");
        mgr.on_crash("a", "error");
        mgr.on_recovery("a");

        // After recovery, next crash should be a simple restart, not quarantine
        let action = mgr.on_crash("a", "error again");
        assert!(matches!(action, HealingAction::Restart));
    }

    #[test]
    fn test_root_cause_classification() {
        assert_eq!(
            RootCause::classify("out of memory OOM"),
            RootCause::MemoryExceeded
        );
        assert_eq!(RootCause::classify("request timeout"), RootCause::Timeout);
        assert_eq!(
            RootCause::classify("sandbox violation"),
            RootCause::SandboxViolation
        );
        assert_eq!(
            RootCause::classify("network connection refused"),
            RootCause::ExternalDependency
        );
        assert_eq!(RootCause::classify("something weird"), RootCause::Unknown);
    }

    #[test]
    fn test_metrics() {
        let mut mgr = SelfHealingManager::new();
        mgr.on_crash("a", "err");
        mgr.on_recovery("a");
        let m = mgr.metrics();
        assert_eq!(m.total_crashes, 1);
        assert_eq!(m.total_recoveries, 1);
        assert!((m.recovery_rate - 1.0).abs() < 0.01);
    }
}

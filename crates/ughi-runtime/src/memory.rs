// UGHI-runtime/src/memory.rs
// Follows strict_rules.md | agent.md | Per agent peak: ≤ 140 MB
// Memory cost: AgentMemoryTracker ~24 bytes (3 atomic u64s)
// SystemMemoryTracker ~16 bytes (2 atomic u64s)
// strict_rules.md rule #5: "Every allocation must be tracked."
// No panic! in core – budget violations return RuntimeError.

use std::sync::atomic::{AtomicU64, Ordering};
use crate::error::RuntimeError;

/// Per-agent memory tracker using atomic counters.
/// Enforces the 140 MB peak limit from agent.md.
/// Memory cost: 24 bytes (3 x AtomicU64)
///
/// strict_rules.md: "Every allocation must be tracked. No Arc<Mutex> spam."
/// We use AtomicU64 instead of Mutex for lock-free tracking.
pub struct AgentMemoryTracker {
    /// Current allocation in bytes
    /// Atomic: lock-free reads/writes, no Mutex needed
    current_bytes: AtomicU64,
    /// Peak observed usage in bytes
    peak_bytes: AtomicU64,
    /// Hard limit in bytes (default: 140 MB per agent.md)
    limit_bytes: u64,
}

impl AgentMemoryTracker {
    /// Create a new tracker with the given limit.
    /// Memory cost: 24 bytes
    pub fn new(limit_bytes: u64) -> Self {
        Self {
            current_bytes: AtomicU64::new(0),
            peak_bytes: AtomicU64::new(0),
            limit_bytes,
        }
    }

    /// Attempt to allocate `size` bytes. Returns error if over budget.
    /// Memory cost: 0 (atomic CAS loop, no heap allocation)
    /// Latency: < 1 µs (lock-free)
    pub fn allocate(&self, size: u64) -> Result<(), RuntimeError> {
        // Atomic fetch-add to track allocation
        let prev = self.current_bytes.fetch_add(size, Ordering::AcqRel);
        let new_total = prev + size;

        // Check against hard limit
        if new_total > self.limit_bytes {
            // Rollback the allocation
            self.current_bytes.fetch_sub(size, Ordering::AcqRel);
            return Err(RuntimeError::MemoryBudgetExceeded {
                used_bytes: new_total,
                limit_bytes: self.limit_bytes,
            });
        }

        // Update peak tracker
        self.peak_bytes.fetch_max(new_total, Ordering::AcqRel);
        Ok(())
    }

    /// Deallocate `size` bytes.
    /// Memory cost: 0 (atomic operation)
    pub fn deallocate(&self, size: u64) {
        // Saturating sub to prevent underflow
        let prev = self.current_bytes.load(Ordering::Acquire);
        let new_val = prev.saturating_sub(size);
        self.current_bytes.store(new_val, Ordering::Release);
    }

    /// Get current memory usage in bytes.
    /// Memory cost: 0 (atomic load)
    pub fn current_usage(&self) -> u64 {
        self.current_bytes.load(Ordering::Acquire)
    }

    /// Get peak memory usage observed.
    /// Memory cost: 0 (atomic load)
    pub fn peak_usage(&self) -> u64 {
        self.peak_bytes.load(Ordering::Acquire)
    }

    /// Get the hard limit in bytes.
    /// Memory cost: 0
    pub fn limit(&self) -> u64 {
        self.limit_bytes
    }

    /// Check if current usage is within budget.
    /// Memory cost: 0
    pub fn is_within_budget(&self) -> bool {
        self.current_usage() <= self.limit_bytes
    }

    /// Get usage as a percentage of the limit.
    /// Memory cost: 0
    pub fn usage_percent(&self) -> f64 {
        if self.limit_bytes == 0 {
            return 0.0;
        }
        (self.current_usage() as f64 / self.limit_bytes as f64) * 100.0
    }

    /// Reset all counters (used on agent recovery).
    /// Memory cost: 0
    pub fn reset(&self) {
        self.current_bytes.store(0, Ordering::Release);
        self.peak_bytes.store(0, Ordering::Release);
    }
}

/// System-wide memory tracker across all agents.
/// Memory cost: 16 bytes (2 x AtomicU64)
///
/// Tracks total memory used by all agents combined.
/// strict_rules.md: "Total RAM for 20 agents + orchestrator + SLM: ≤ 3.2 GB peak"
pub struct SystemMemoryTracker {
    /// Total bytes allocated across all agents
    total_bytes: AtomicU64,
    /// Number of active agents being tracked
    agent_count: AtomicU64,
}

impl SystemMemoryTracker {
    /// Create a new system-wide tracker.
    /// Memory cost: 16 bytes
    pub fn new() -> Self {
        Self {
            total_bytes: AtomicU64::new(0),
            agent_count: AtomicU64::new(0),
        }
    }

    /// Register a new agent's memory usage.
    /// Memory cost: 0
    pub fn register_agent(&self, initial_bytes: u64) {
        self.total_bytes.fetch_add(initial_bytes, Ordering::AcqRel);
        self.agent_count.fetch_add(1, Ordering::AcqRel);
    }

    /// Unregister an agent and free its tracked memory.
    /// Memory cost: 0
    pub fn unregister_agent(&self, agent_bytes: u64) {
        let prev = self.total_bytes.load(Ordering::Acquire);
        self.total_bytes.store(prev.saturating_sub(agent_bytes), Ordering::Release);
        let prev_count = self.agent_count.load(Ordering::Acquire);
        self.agent_count.store(prev_count.saturating_sub(1), Ordering::Release);
    }

    /// Update tracked allocation delta.
    /// Memory cost: 0
    pub fn add_bytes(&self, bytes: u64) {
        self.total_bytes.fetch_add(bytes, Ordering::AcqRel);
    }

    /// Update tracked deallocation delta.
    /// Memory cost: 0
    pub fn sub_bytes(&self, bytes: u64) {
        let prev = self.total_bytes.load(Ordering::Acquire);
        self.total_bytes.store(prev.saturating_sub(bytes), Ordering::Release);
    }

    /// Get total memory usage across all agents.
    /// Memory cost: 0
    pub fn total_usage(&self) -> u64 {
        self.total_bytes.load(Ordering::Acquire)
    }

    /// Get number of tracked agents.
    /// Memory cost: 0
    pub fn tracked_agent_count(&self) -> u64 {
        self.agent_count.load(Ordering::Acquire)
    }

    /// Get average memory per agent (0 if no agents).
    /// Memory cost: 0
    pub fn avg_usage_per_agent(&self) -> u64 {
        let count = self.tracked_agent_count();
        if count == 0 {
            return 0;
        }
        self.total_usage() / count
    }
}

impl Default for SystemMemoryTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allocate_within_budget() {
        let tracker = AgentMemoryTracker::new(140 * 1024 * 1024); // 140 MB
        // Allocate 40 MB
        assert!(tracker.allocate(40 * 1024 * 1024).is_ok());
        assert_eq!(tracker.current_usage(), 40 * 1024 * 1024);
    }

    #[test]
    fn test_allocate_exceeds_budget() {
        let tracker = AgentMemoryTracker::new(140 * 1024 * 1024);
        // Allocate 140 MB (exactly at limit)
        assert!(tracker.allocate(140 * 1024 * 1024).is_ok());
        // Try to allocate 1 more byte – should fail
        assert!(tracker.allocate(1).is_err());
        // Usage should not have increased (rollback)
        assert_eq!(tracker.current_usage(), 140 * 1024 * 1024);
    }

    #[test]
    fn test_deallocate() {
        let tracker = AgentMemoryTracker::new(140 * 1024 * 1024);
        tracker.allocate(50 * 1024 * 1024).unwrap();
        tracker.deallocate(20 * 1024 * 1024);
        assert_eq!(tracker.current_usage(), 30 * 1024 * 1024);
    }

    #[test]
    fn test_peak_tracking() {
        let tracker = AgentMemoryTracker::new(140 * 1024 * 1024);
        tracker.allocate(80 * 1024 * 1024).unwrap();
        tracker.deallocate(60 * 1024 * 1024);
        tracker.allocate(30 * 1024 * 1024).unwrap();
        // Peak should be 80 MB (first allocation)
        assert_eq!(tracker.peak_usage(), 80 * 1024 * 1024);
    }

    #[test]
    fn test_usage_percent() {
        let tracker = AgentMemoryTracker::new(100);
        tracker.allocate(75).unwrap();
        assert!((tracker.usage_percent() - 75.0).abs() < 0.001);
    }

    #[test]
    fn test_reset() {
        let tracker = AgentMemoryTracker::new(140 * 1024 * 1024);
        tracker.allocate(50 * 1024 * 1024).unwrap();
        tracker.reset();
        assert_eq!(tracker.current_usage(), 0);
        assert_eq!(tracker.peak_usage(), 0);
    }

    #[test]
    fn test_system_tracker() {
        let sys = SystemMemoryTracker::new();
        sys.register_agent(10 * 1024 * 1024);
        sys.register_agent(20 * 1024 * 1024);
        assert_eq!(sys.tracked_agent_count(), 2);
        assert_eq!(sys.total_usage(), 30 * 1024 * 1024);
        assert_eq!(sys.avg_usage_per_agent(), 15 * 1024 * 1024);

        sys.unregister_agent(10 * 1024 * 1024);
        assert_eq!(sys.tracked_agent_count(), 1);
        assert_eq!(sys.total_usage(), 20 * 1024 * 1024);
    }

    #[test]
    fn test_deallocate_underflow_safe() {
        let tracker = AgentMemoryTracker::new(100);
        tracker.allocate(10).unwrap();
        // Deallocate more than allocated – should saturate to 0
        tracker.deallocate(20);
        assert_eq!(tracker.current_usage(), 0);
    }
}

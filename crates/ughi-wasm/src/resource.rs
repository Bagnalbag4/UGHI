// UGHI-wasm/src/resource.rs
// Follows strict_rules.md | Per-execution resource limits
// Memory cost: ~64 bytes per ResourceLimits struct
// Enforces: max memory, max fuel (CPU), max wall time per skill call.

use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// Resource limits for a single skill execution.
/// Memory cost: ~64 bytes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Max memory in bytes (skills.md: ≤ 45 MB per call)
    pub max_memory_bytes: u64,
    /// Max CPU fuel (wasmtime fuel units, ~1 fuel per wasm instruction)
    pub max_fuel: u64,
    /// Max wall clock time in milliseconds
    pub max_time_ms: u64,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_memory_bytes: 45 * 1024 * 1024, // 45 MB
            max_fuel: 10_000_000,               // ~10M instructions
            max_time_ms: 420,                   // 420 ms cold SLA
        }
    }
}

impl ResourceLimits {
    /// Minimal limits for low-risk skills.
    pub fn minimal() -> Self {
        Self {
            max_memory_bytes: 4 * 1024 * 1024, // 4 MB
            max_fuel: 1_000_000,
            max_time_ms: 100,
        }
    }

    /// High limits for complex skills (still within budget).
    pub fn high() -> Self {
        Self {
            max_memory_bytes: 45 * 1024 * 1024, // 45 MB
            max_fuel: 50_000_000,
            max_time_ms: 420,
        }
    }
}

/// Tracks resource usage during a single execution.
/// Memory cost: ~64 bytes
pub struct ResourceTracker {
    limits: ResourceLimits,
    memory_used: u64,
    fuel_used: u64,
    start_time: Instant,
}

impl ResourceTracker {
    /// Start tracking with given limits.
    pub fn start(limits: ResourceLimits) -> Self {
        Self {
            limits,
            memory_used: 0,
            fuel_used: 0,
            start_time: Instant::now(),
        }
    }

    /// Record memory allocation. Returns false if budget exceeded.
    pub fn allocate(&mut self, bytes: u64) -> bool {
        self.memory_used += bytes;
        self.memory_used <= self.limits.max_memory_bytes
    }

    /// Record fuel consumption. Returns false if budget exceeded.
    pub fn consume_fuel(&mut self, fuel: u64) -> bool {
        self.fuel_used += fuel;
        self.fuel_used <= self.limits.max_fuel
    }

    /// Check if wall clock time exceeded.
    pub fn is_timed_out(&self) -> bool {
        self.elapsed_ms() > self.limits.max_time_ms
    }

    /// Elapsed milliseconds since start.
    pub fn elapsed_ms(&self) -> u64 {
        self.start_time.elapsed().as_millis() as u64
    }

    /// Elapsed duration.
    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Check all resource limits. Returns None if OK, or error description.
    pub fn check_limits(&self) -> Option<String> {
        if self.memory_used > self.limits.max_memory_bytes {
            return Some(format!(
                "memory: {} > {} bytes",
                self.memory_used, self.limits.max_memory_bytes
            ));
        }
        if self.fuel_used > self.limits.max_fuel {
            return Some(format!(
                "fuel: {} > {}",
                self.fuel_used, self.limits.max_fuel
            ));
        }
        if self.is_timed_out() {
            return Some(format!(
                "time: {}ms > {}ms",
                self.elapsed_ms(),
                self.limits.max_time_ms
            ));
        }
        None
    }

    /// Get current usage snapshot.
    pub fn usage(&self) -> ResourceUsage {
        ResourceUsage {
            memory_bytes: self.memory_used,
            memory_limit: self.limits.max_memory_bytes,
            fuel_used: self.fuel_used,
            fuel_limit: self.limits.max_fuel,
            elapsed_ms: self.elapsed_ms(),
            time_limit_ms: self.limits.max_time_ms,
        }
    }
}

/// Snapshot of resource usage (for dashboard).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsage {
    pub memory_bytes: u64,
    pub memory_limit: u64,
    pub fuel_used: u64,
    pub fuel_limit: u64,
    pub elapsed_ms: u64,
    pub time_limit_ms: u64,
}

impl ResourceUsage {
    /// Memory utilization as percentage.
    pub fn memory_pct(&self) -> f64 {
        if self.memory_limit == 0 {
            return 0.0;
        }
        (self.memory_bytes as f64 / self.memory_limit as f64) * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_limits() {
        let limits = ResourceLimits::default();
        assert_eq!(limits.max_memory_bytes, 45 * 1024 * 1024);
        assert_eq!(limits.max_time_ms, 420);
    }

    #[test]
    fn test_memory_tracking() {
        let mut tracker = ResourceTracker::start(ResourceLimits::default());
        assert!(tracker.allocate(1024));
        assert!(tracker.allocate(1024 * 1024));
        assert!(tracker.check_limits().is_none());
    }

    #[test]
    fn test_memory_budget_exceeded() {
        let mut tracker = ResourceTracker::start(ResourceLimits::minimal());
        assert!(!tracker.allocate(5 * 1024 * 1024));
        assert!(tracker.check_limits().is_some());
    }

    #[test]
    fn test_fuel_tracking() {
        let mut tracker = ResourceTracker::start(ResourceLimits::minimal());
        assert!(tracker.consume_fuel(500_000));
        assert!(!tracker.consume_fuel(1_000_000));
    }

    #[test]
    fn test_usage_snapshot() {
        let mut tracker = ResourceTracker::start(ResourceLimits::default());
        tracker.allocate(1024);
        let usage = tracker.usage();
        assert_eq!(usage.memory_bytes, 1024);
        assert!(usage.memory_pct() < 1.0);
    }
}

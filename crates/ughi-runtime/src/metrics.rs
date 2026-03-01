// UGHI-runtime/src/metrics.rs
// Follows strict_rules.md | Memory cost: ~64 bytes (8 x AtomicU64)
// Lock-free metrics exporter using atomic counters.
// strict_rules.md rule #5: "Every allocation must be tracked."
// No panic! in core.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

/// Runtime metrics using atomic counters – fully lock-free.
/// Memory cost: 64 bytes (8 x AtomicU64)
///
/// strict_rules.md: "No Arc<Mutex> spam" – we use AtomicU64 exclusively.
pub struct RuntimeMetrics {
    /// Currently active agents
    agents_active: AtomicU64,
    /// Total agents ever spawned (lifetime)
    agents_total_spawned: AtomicU64,
    /// Total agents completed
    agents_total_completed: AtomicU64,
    /// Total agents crashed
    agents_total_crashed: AtomicU64,
    /// Total memory across all agents (bytes)
    memory_total_bytes: AtomicU64,
    /// Last spawn latency in microseconds
    last_spawn_latency_us: AtomicU64,
    /// Total scheduler ticks processed
    scheduler_ticks: AtomicU64,
    /// Total scheduler dequeues
    scheduler_dequeues: AtomicU64,
}

impl RuntimeMetrics {
    /// Create a new metrics instance.
    /// Memory cost: 64 bytes
    pub fn new() -> Self {
        Self {
            agents_active: AtomicU64::new(0),
            agents_total_spawned: AtomicU64::new(0),
            agents_total_completed: AtomicU64::new(0),
            agents_total_crashed: AtomicU64::new(0),
            memory_total_bytes: AtomicU64::new(0),
            last_spawn_latency_us: AtomicU64::new(0),
            scheduler_ticks: AtomicU64::new(0),
            scheduler_dequeues: AtomicU64::new(0),
        }
    }

    /// Record an agent spawn.
    /// Memory cost: 0
    pub fn record_spawn(&self, latency_us: u64) {
        self.agents_active.fetch_add(1, Ordering::AcqRel);
        self.agents_total_spawned.fetch_add(1, Ordering::AcqRel);
        self.last_spawn_latency_us
            .store(latency_us, Ordering::Release);
    }

    /// Record an agent completion.
    /// Memory cost: 0
    pub fn record_completion(&self) {
        let prev = self.agents_active.load(Ordering::Acquire);
        self.agents_active
            .store(prev.saturating_sub(1), Ordering::Release);
        self.agents_total_completed.fetch_add(1, Ordering::AcqRel);
    }

    /// Record an agent crash.
    /// Memory cost: 0
    pub fn record_crash(&self) {
        let prev = self.agents_active.load(Ordering::Acquire);
        self.agents_active
            .store(prev.saturating_sub(1), Ordering::Release);
        self.agents_total_crashed.fetch_add(1, Ordering::AcqRel);
    }

    /// Record an agent kill (removal without completion).
    /// Memory cost: 0
    pub fn record_kill(&self) {
        let prev = self.agents_active.load(Ordering::Acquire);
        self.agents_active
            .store(prev.saturating_sub(1), Ordering::Release);
    }

    /// Update total memory usage.
    /// Memory cost: 0
    pub fn set_memory_total(&self, bytes: u64) {
        self.memory_total_bytes.store(bytes, Ordering::Release);
    }

    /// Record a scheduler tick.
    /// Memory cost: 0
    pub fn record_scheduler_tick(&self) {
        self.scheduler_ticks.fetch_add(1, Ordering::AcqRel);
    }

    /// Record a scheduler dequeue.
    /// Memory cost: 0
    pub fn record_scheduler_dequeue(&self) {
        self.scheduler_dequeues.fetch_add(1, Ordering::AcqRel);
    }

    /// Get a serializable snapshot of current metrics.
    /// Memory cost: ~128 bytes (MetricsSnapshot struct)
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            agents_active: self.agents_active.load(Ordering::Acquire),
            agents_total_spawned: self.agents_total_spawned.load(Ordering::Acquire),
            agents_total_completed: self.agents_total_completed.load(Ordering::Acquire),
            agents_total_crashed: self.agents_total_crashed.load(Ordering::Acquire),
            memory_total_bytes: self.memory_total_bytes.load(Ordering::Acquire),
            memory_total_mb: self.memory_total_bytes.load(Ordering::Acquire) as f64
                / (1024.0 * 1024.0),
            last_spawn_latency_us: self.last_spawn_latency_us.load(Ordering::Acquire),
            scheduler_ticks: self.scheduler_ticks.load(Ordering::Acquire),
            scheduler_dequeues: self.scheduler_dequeues.load(Ordering::Acquire),
        }
    }
}

impl Default for RuntimeMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Serializable metrics snapshot.
/// Memory cost: ~128 bytes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub agents_active: u64,
    pub agents_total_spawned: u64,
    pub agents_total_completed: u64,
    pub agents_total_crashed: u64,
    pub memory_total_bytes: u64,
    pub memory_total_mb: f64,
    pub last_spawn_latency_us: u64,
    pub scheduler_ticks: u64,
    pub scheduler_dequeues: u64,
}

impl std::fmt::Display for MetricsSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "agents={}/{} (completed={}, crashed={}) | mem={:.1}MB | spawn_lat={}µs | ticks={}",
            self.agents_active,
            self.agents_total_spawned,
            self.agents_total_completed,
            self.agents_total_crashed,
            self.memory_total_mb,
            self.last_spawn_latency_us,
            self.scheduler_ticks,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_spawn_and_complete() {
        let metrics = RuntimeMetrics::new();
        metrics.record_spawn(100);
        metrics.record_spawn(200);
        let snap = metrics.snapshot();
        assert_eq!(snap.agents_active, 2);
        assert_eq!(snap.agents_total_spawned, 2);
        assert_eq!(snap.last_spawn_latency_us, 200);

        metrics.record_completion();
        let snap = metrics.snapshot();
        assert_eq!(snap.agents_active, 1);
        assert_eq!(snap.agents_total_completed, 1);
    }

    #[test]
    fn test_metrics_crash() {
        let metrics = RuntimeMetrics::new();
        metrics.record_spawn(50);
        metrics.record_crash();
        let snap = metrics.snapshot();
        assert_eq!(snap.agents_active, 0);
        assert_eq!(snap.agents_total_crashed, 1);
    }

    #[test]
    fn test_metrics_memory() {
        let metrics = RuntimeMetrics::new();
        metrics.set_memory_total(512 * 1024 * 1024); // 512 MB
        let snap = metrics.snapshot();
        assert_eq!(snap.memory_total_bytes, 512 * 1024 * 1024);
        assert!((snap.memory_total_mb - 512.0).abs() < 0.1);
    }

    #[test]
    fn test_metrics_display() {
        let metrics = RuntimeMetrics::new();
        metrics.record_spawn(42);
        let snap = metrics.snapshot();
        let display = format!("{}", snap);
        assert!(display.contains("agents=1/1"));
        assert!(display.contains("spawn_lat=42µs"));
    }
}

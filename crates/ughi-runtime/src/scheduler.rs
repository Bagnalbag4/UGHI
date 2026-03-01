// UGHI-runtime/src/scheduler.rs
// Follows strict_rules.md | Latency: tick < 1 ms | No panic! in core
// Memory cost: Scheduler ~2 MB base (priority queue + agent map)
// Priority queue with fair scheduling: round-robin within same priority level.
// Starvation prevention: agents waiting > 5 ticks get priority boost.

use crate::agent::AgentPriority;
use std::cmp::Ordering;
use std::collections::BinaryHeap;

/// An entry in the scheduler's priority queue.
/// Ordering: highest priority first, then oldest (lowest tick) first for fairness.
/// Memory cost: ~48 bytes (String + priority + counters)
#[derive(Debug, Clone)]
pub struct SchedulerEntry {
    /// Agent ID to schedule
    pub agent_id: String,
    /// Current effective priority (may be boosted for fairness)
    pub effective_priority: AgentPriority,
    /// Original priority (never changes)
    pub base_priority: AgentPriority,
    /// Tick when this entry was enqueued (lower = older = higher priority within same level)
    pub enqueued_at_tick: u64,
    /// Number of ticks this agent has waited without being scheduled
    pub wait_ticks: u64,
}

impl SchedulerEntry {
    /// Create a new entry.
    /// Memory cost: ~48 bytes
    pub fn new(agent_id: String, priority: AgentPriority, tick: u64) -> Self {
        Self {
            agent_id,
            effective_priority: priority,
            base_priority: priority,
            enqueued_at_tick: tick,
            wait_ticks: 0,
        }
    }
}

/// Priority ordering: higher effective_priority first, then older (lower tick) first.
/// This gives fair scheduling within the same priority level.
impl Eq for SchedulerEntry {}

impl PartialEq for SchedulerEntry {
    fn eq(&self, other: &Self) -> bool {
        self.effective_priority == other.effective_priority
            && self.enqueued_at_tick == other.enqueued_at_tick
    }
}

impl Ord for SchedulerEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        // Higher priority first
        self.effective_priority
            .cmp(&other.effective_priority)
            // Within same priority, older entries first (lower tick = higher priority)
            .then_with(|| other.enqueued_at_tick.cmp(&self.enqueued_at_tick))
    }
}

impl PartialOrd for SchedulerEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Starvation threshold: agents waiting this many ticks get a priority boost.
/// Memory cost: 0 (constant)
const STARVATION_THRESHOLD: u64 = 5;

/// Tokio-based priority scheduler with fair queuing.
/// Memory cost: ~2 MB base (BinaryHeap + metadata)
///
/// Design:
/// - BinaryHeap for O(log n) insert/dequeue by priority
/// - Round-robin fairness within same priority level (via enqueued_at_tick)
/// - Starvation prevention: agents waiting > STARVATION_THRESHOLD ticks get boosted
pub struct Scheduler {
    /// Priority queue (max-heap by effective priority, then by age)
    /// Allocation: ~48 bytes per entry
    queue: BinaryHeap<SchedulerEntry>,
    /// Maximum queue capacity
    capacity: usize,
    /// Current scheduler tick (monotonically increasing)
    current_tick: u64,
    /// Total agents scheduled (lifetime counter for metrics)
    total_scheduled: u64,
    /// Total ticks processed
    total_ticks: u64,
}

impl Scheduler {
    /// Create a new scheduler with the given capacity.
    /// Memory cost: ~2 MB (pre-allocated heap)
    pub fn new(capacity: usize) -> Self {
        Self {
            queue: BinaryHeap::with_capacity(capacity),
            capacity,
            current_tick: 0,
            total_scheduled: 0,
            total_ticks: 0,
        }
    }

    /// Enqueue an agent for scheduling.
    /// Memory cost: ~48 bytes per entry added
    /// Latency: O(log n) heap insertion
    pub fn enqueue(
        &mut self,
        agent_id: String,
        priority: AgentPriority,
    ) -> Result<(), crate::error::RuntimeError> {
        if self.queue.len() >= self.capacity {
            return Err(crate::error::RuntimeError::SchedulerFull {
                capacity: self.capacity,
            });
        }

        let entry = SchedulerEntry::new(agent_id, priority, self.current_tick);
        self.queue.push(entry);
        Ok(())
    }

    /// Dequeue the highest-priority agent.
    /// Memory cost: 0 (returns existing allocation)
    /// Latency: O(log n) heap extraction
    pub fn dequeue(&mut self) -> Option<SchedulerEntry> {
        let entry = self.queue.pop();
        if entry.is_some() {
            self.total_scheduled += 1;
        }
        entry
    }

    /// Advance the scheduler by one tick.
    /// Applies starvation prevention: boosts priority of long-waiting agents.
    /// Memory cost: O(n) temporary Vec for re-heapification
    /// Latency: O(n log n) worst case, but n is bounded by capacity (≤ 50)
    pub fn tick(&mut self) {
        self.current_tick += 1;
        self.total_ticks += 1;

        // Starvation prevention: re-heapify with boosted priorities
        // Only run when queue is non-empty
        if self.queue.is_empty() {
            return;
        }

        // Drain, boost, and re-insert
        // Allocation: temporary Vec ~48 bytes * queue.len()
        let mut entries: Vec<SchedulerEntry> = self.queue.drain().collect();
        for entry in &mut entries {
            entry.wait_ticks += 1;

            // Boost priority if waiting too long (starvation prevention)
            if entry.wait_ticks > STARVATION_THRESHOLD {
                entry.effective_priority = match entry.effective_priority {
                    AgentPriority::Background => AgentPriority::Low,
                    AgentPriority::Low => AgentPriority::Normal,
                    AgentPriority::Normal => AgentPriority::High,
                    AgentPriority::High => AgentPriority::Critical,
                    AgentPriority::Critical => AgentPriority::Critical, // Already max
                };
                entry.wait_ticks = 0; // Reset after boost
            }
        }

        // Re-heapify
        self.queue = BinaryHeap::from(entries);
    }

    /// Get the number of agents currently queued.
    /// Memory cost: 0
    pub fn queue_len(&self) -> usize {
        self.queue.len()
    }

    /// Check if the queue is empty.
    /// Memory cost: 0
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Get current tick count.
    /// Memory cost: 0
    pub fn current_tick(&self) -> u64 {
        self.current_tick
    }

    /// Get total agents scheduled (lifetime).
    /// Memory cost: 0
    pub fn total_scheduled(&self) -> u64 {
        self.total_scheduled
    }

    /// Get total ticks processed.
    /// Memory cost: 0
    pub fn total_ticks(&self) -> u64 {
        self.total_ticks
    }

    /// Remove a specific agent from the queue (e.g., on kill).
    /// Memory cost: O(n) temporary Vec
    /// Latency: O(n log n) worst case
    pub fn remove(&mut self, agent_id: &str) -> bool {
        let before = self.queue.len();
        let entries: Vec<SchedulerEntry> = self
            .queue
            .drain()
            .filter(|e| e.agent_id != agent_id)
            .collect();
        self.queue = BinaryHeap::from(entries);
        self.queue.len() < before
    }

    /// Peek at the highest-priority entry without removing it.
    /// Memory cost: 0
    pub fn peek(&self) -> Option<&SchedulerEntry> {
        self.queue.peek()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_priority_ordering() {
        let mut scheduler = Scheduler::new(10);
        scheduler
            .enqueue("low_agent".into(), AgentPriority::Low)
            .unwrap();
        scheduler
            .enqueue("high_agent".into(), AgentPriority::High)
            .unwrap();
        scheduler
            .enqueue("normal_agent".into(), AgentPriority::Normal)
            .unwrap();

        // Highest priority dequeued first
        let first = scheduler.dequeue().unwrap();
        assert_eq!(first.agent_id, "high_agent");
        let second = scheduler.dequeue().unwrap();
        assert_eq!(second.agent_id, "normal_agent");
        let third = scheduler.dequeue().unwrap();
        assert_eq!(third.agent_id, "low_agent");
    }

    #[test]
    fn test_fifo_within_same_priority() {
        let mut scheduler = Scheduler::new(10);
        scheduler
            .enqueue("first".into(), AgentPriority::Normal)
            .unwrap();
        scheduler.current_tick += 1; // Advance tick so second has higher tick
        scheduler
            .enqueue("second".into(), AgentPriority::Normal)
            .unwrap();

        // Same priority: FIFO (older first)
        let first = scheduler.dequeue().unwrap();
        assert_eq!(first.agent_id, "first");
    }

    #[test]
    fn test_starvation_prevention() {
        let mut scheduler = Scheduler::new(10);
        scheduler
            .enqueue("bg_agent".into(), AgentPriority::Background)
            .unwrap();

        // Tick 6 times past starvation threshold
        for _ in 0..6 {
            scheduler.tick();
        }

        // Background agent should have been boosted to Low
        let entry = scheduler.dequeue().unwrap();
        assert_eq!(entry.effective_priority, AgentPriority::Low);
        assert_eq!(entry.base_priority, AgentPriority::Background); // Base unchanged
    }

    #[test]
    fn test_capacity_limit() {
        let mut scheduler = Scheduler::new(2);
        scheduler
            .enqueue("a1".into(), AgentPriority::Normal)
            .unwrap();
        scheduler
            .enqueue("a2".into(), AgentPriority::Normal)
            .unwrap();
        assert!(scheduler
            .enqueue("a3".into(), AgentPriority::Normal)
            .is_err());
    }

    #[test]
    fn test_remove() {
        let mut scheduler = Scheduler::new(10);
        scheduler
            .enqueue("keep".into(), AgentPriority::Normal)
            .unwrap();
        scheduler
            .enqueue("remove_me".into(), AgentPriority::Normal)
            .unwrap();
        assert_eq!(scheduler.queue_len(), 2);

        assert!(scheduler.remove("remove_me"));
        assert_eq!(scheduler.queue_len(), 1);
        let remaining = scheduler.dequeue().unwrap();
        assert_eq!(remaining.agent_id, "keep");
    }

    #[test]
    fn test_tick_counter() {
        let mut scheduler = Scheduler::new(10);
        assert_eq!(scheduler.current_tick(), 0);
        scheduler.tick();
        scheduler.tick();
        assert_eq!(scheduler.current_tick(), 2);
        assert_eq!(scheduler.total_ticks(), 2);
    }

    #[test]
    fn test_empty_queue_operations() {
        let mut scheduler = Scheduler::new(10);
        assert!(scheduler.dequeue().is_none());
        assert!(scheduler.peek().is_none());
        assert!(scheduler.is_empty());
        scheduler.tick(); // Should not panic on empty queue
    }
}

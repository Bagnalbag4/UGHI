// UGHI-runtime/src/lib.rs
// Follows strict_rules.md | agent.md | skills.md
// Memory budget: Runtime base ~8 MB | Per agent peak ≤ 140 MB
// Total 50 agents: ~1.75 GB < 1.8 GB target
//
// Module structure:
// - agent:     Agent struct, lifecycle FSM, priority, config, snapshots
// - error:     RuntimeError enum (all recoverable, no panic!)
// - memory:    Lock-free per-agent memory tracking (140 MB enforcement)
// - scheduler: Priority BinaryHeap with fair scheduling
// - runtime:   The micro-kernel: spawn/kill/monitor/list APIs
// - metrics:   Lock-free atomic counters for observability
//
// No panic! in core. No Arc<Mutex> spam (single Mutex on inner state,
// atomics for metrics/memory). No GPU. No Python in hot path.

pub mod agent;
pub mod backup;
pub mod error;
pub mod healing;
pub mod hibernation;
pub mod memory;
pub mod metrics;
pub mod runtime;
pub mod scheduler;

// --- Public re-exports for ergonomic API ---

pub use agent::{Agent, AgentConfig, AgentPriority, AgentSnapshot, AgentState};
pub use error::RuntimeError;
pub use healing::{HealingAction, HealingMetrics, RootCause, SelfHealingManager};
pub use hibernation::{GovernorMetrics, HibernatedState, ResourceGovernor};
pub use memory::{AgentMemoryTracker, SystemMemoryTracker};
pub use metrics::{MetricsSnapshot, RuntimeMetrics};
pub use runtime::{Runtime, RuntimeConfig};
pub use scheduler::Scheduler;

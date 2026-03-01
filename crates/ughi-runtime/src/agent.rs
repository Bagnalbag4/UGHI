// UGHI-runtime/src/agent.rs
// Follows strict_rules.md | agent.md | Per agent peak: ≤ 140 MB
// Memory cost: Agent struct ~512 bytes base (excluding short-term memory contents)
// Lifecycle FSM: Spawned → Planning → Tool-Using → Thinking → Collaborating → Reviewing → Completing
// Crashed state is reachable from any state; auto-recovery resets to Spawned.
// No panic! in core – invalid transitions return RuntimeError.

use serde::{Deserialize, Serialize};
use std::time::Instant;
use crate::error::RuntimeError;
use crate::memory::AgentMemoryTracker;

/// Agent lifecycle states per agent.md specification.
/// Memory cost: 1 byte (enum discriminant)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum AgentState {
    /// Just created, idle < 80ms
    Spawned = 0,
    /// Building execution plan
    Planning = 1,
    /// Executing a skill/tool
    ToolUsing = 2,
    /// Running SLM inference
    Thinking = 3,
    /// Communicating with other agents
    Collaborating = 4,
    /// Self-critique via SLM reflection
    Reviewing = 5,
    /// Task complete or suspended
    Completing = 6,
    /// Crashed but recoverable → auto-recover to Spawned
    Crashed = 7,
}

impl AgentState {
    /// Check if this state can transition to the target state.
    /// Enforces valid lifecycle transitions per agent.md.
    /// Memory cost: 0 (pure logic)
    pub fn can_transition_to(&self, target: AgentState) -> bool {
        // Any state can crash
        if target == AgentState::Crashed {
            return true;
        }
        // Crashed can only go to Spawned (auto-recovery)
        if *self == AgentState::Crashed {
            return target == AgentState::Spawned;
        }
        // Completing is terminal (except crash)
        if *self == AgentState::Completing {
            return false;
        }

        matches!(
            (self, target),
            // Spawned → Planning (agent starts working)
            (AgentState::Spawned, AgentState::Planning)
            // Planning → ToolUsing | Thinking | Collaborating
            | (AgentState::Planning, AgentState::ToolUsing)
            | (AgentState::Planning, AgentState::Thinking)
            | (AgentState::Planning, AgentState::Collaborating)
            // ToolUsing → Planning | Thinking | Reviewing | Completing
            | (AgentState::ToolUsing, AgentState::Planning)
            | (AgentState::ToolUsing, AgentState::Thinking)
            | (AgentState::ToolUsing, AgentState::Reviewing)
            | (AgentState::ToolUsing, AgentState::Completing)
            // Thinking → Planning | ToolUsing | Reviewing | Collaborating | Completing
            | (AgentState::Thinking, AgentState::Planning)
            | (AgentState::Thinking, AgentState::ToolUsing)
            | (AgentState::Thinking, AgentState::Reviewing)
            | (AgentState::Thinking, AgentState::Collaborating)
            | (AgentState::Thinking, AgentState::Completing)
            // Collaborating → Planning | Thinking | Reviewing
            | (AgentState::Collaborating, AgentState::Planning)
            | (AgentState::Collaborating, AgentState::Thinking)
            | (AgentState::Collaborating, AgentState::Reviewing)
            // Reviewing → Planning | ToolUsing | Completing
            | (AgentState::Reviewing, AgentState::Planning)
            | (AgentState::Reviewing, AgentState::ToolUsing)
            | (AgentState::Reviewing, AgentState::Completing)
        )
    }

    /// Human-readable state name.
    /// Memory cost: 0 (static str)
    pub fn as_str(&self) -> &'static str {
        match self {
            AgentState::Spawned => "spawned",
            AgentState::Planning => "planning",
            AgentState::ToolUsing => "tool_using",
            AgentState::Thinking => "thinking",
            AgentState::Collaborating => "collaborating",
            AgentState::Reviewing => "reviewing",
            AgentState::Completing => "completing",
            AgentState::Crashed => "crashed",
        }
    }
}

impl std::fmt::Display for AgentState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Agent execution priority – determines scheduling order.
/// Memory cost: 1 byte (enum discriminant)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum AgentPriority {
    /// Background maintenance tasks
    Background = 0,
    /// Default priority for most agents
    Low = 1,
    /// Standard user-facing tasks
    Normal = 2,
    /// Time-sensitive operations
    High = 3,
    /// System-critical, must run immediately
    Critical = 4,
}

impl Default for AgentPriority {
    fn default() -> Self {
        AgentPriority::Normal
    }
}

impl std::fmt::Display for AgentPriority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentPriority::Background => write!(f, "background"),
            AgentPriority::Low => write!(f, "low"),
            AgentPriority::Normal => write!(f, "normal"),
            AgentPriority::High => write!(f, "high"),
            AgentPriority::Critical => write!(f, "critical"),
        }
    }
}

/// Per-agent memory model from agent.md.
/// Memory cost: 32 bytes (4 x u64)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMemoryModel {
    /// Short-term memory limit: 40 MB (in-memory working set)
    pub short_term_limit_bytes: u64,
    /// Long-term memory (SQLite + vector): 15 MB per agent
    pub long_term_limit_bytes: u64,
    /// Peak allowed: 140 MB total (including KV cache sharing)
    pub peak_limit_bytes: u64,
    /// Current tracked usage
    pub current_usage_bytes: u64,
}

impl Default for AgentMemoryModel {
    /// Memory cost: 32 bytes (stack)
    fn default() -> Self {
        Self {
            short_term_limit_bytes: 40 * 1024 * 1024,   // 40 MB
            long_term_limit_bytes: 15 * 1024 * 1024,    // 15 MB
            peak_limit_bytes: 140 * 1024 * 1024,        // 140 MB (agent.md)
            current_usage_bytes: 0,
        }
    }
}

/// Agent configuration passed at spawn time.
/// Memory cost: ~256 bytes (goal string + config)
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// The goal/task this agent should accomplish
    pub goal: String,
    /// Parent agent ID (supervisor tree)
    pub parent_id: Option<String>,
    /// Scheduling priority
    pub priority: AgentPriority,
    /// Custom peak memory limit (default: 140 MB per agent.md)
    pub peak_memory_limit: Option<u64>,
}

impl AgentConfig {
    /// Create a config with a goal string and default settings.
    /// Memory cost: ~128 bytes
    pub fn new(goal: impl Into<String>) -> Self {
        Self {
            goal: goal.into(),
            parent_id: None,
            priority: AgentPriority::Normal,
            peak_memory_limit: None,
        }
    }

    /// Set parent agent (builder pattern).
    pub fn with_parent(mut self, parent_id: impl Into<String>) -> Self {
        self.parent_id = Some(parent_id.into());
        self
    }

    /// Set priority (builder pattern).
    pub fn with_priority(mut self, priority: AgentPriority) -> Self {
        self.priority = priority;
        self
    }
}

/// A single agent – first-class, memory-safe, sandboxed OS process equivalent.
/// This is the core unit of UGHI per agent.md.
/// Memory cost: ~512 bytes base (excluding short-term memory contents)
pub struct Agent {
    /// Unique 12-char ID per agent.md
    pub id: String,
    /// The goal/task this agent is working on
    pub goal: String,
    /// Parent agent ID (supervisor tree)
    pub parent_id: Option<String>,
    /// Current lifecycle state (FSM)
    pub state: AgentState,
    /// Scheduling priority
    pub priority: AgentPriority,
    /// Memory model and tracking
    pub memory_model: AgentMemoryModel,
    /// Live memory tracker (atomic counters)
    pub memory_tracker: AgentMemoryTracker,
    /// Capability manifest – starts empty (zero trust per strict_rules.md)
    pub capabilities: Vec<String>,
    /// Timestamp: when agent was created
    pub created_at: Instant,
    /// Timestamp: last state transition or activity
    pub last_active: Instant,
    /// Number of state transitions (for metrics)
    pub transition_count: u64,
}

impl Agent {
    /// Spawn a new agent with zero capabilities (zero trust per strict_rules.md).
    /// Memory cost: ~512 bytes + 12 bytes for nanoid
    /// Latency: < 1 ms (no I/O, no allocation beyond struct)
    pub fn new(config: AgentConfig) -> Self {
        let now = Instant::now();
        let peak_limit = config.peak_memory_limit
            .unwrap_or(140 * 1024 * 1024); // 140 MB default per agent.md

        Self {
            id: nanoid::nanoid!(12),
            goal: config.goal,
            parent_id: config.parent_id,
            state: AgentState::Spawned,
            priority: config.priority,
            memory_model: AgentMemoryModel::default(),
            memory_tracker: AgentMemoryTracker::new(peak_limit),
            capabilities: Vec::new(), // Zero trust – no capabilities at start
            created_at: now,
            last_active: now,
            transition_count: 0,
        }
    }

    /// Transition agent to a new state with FSM validation.
    /// Returns error on invalid transition (no panic!).
    /// Memory cost: 0 (in-place mutation)
    pub fn transition(&mut self, new_state: AgentState) -> Result<(), RuntimeError> {
        if !self.state.can_transition_to(new_state) {
            return Err(RuntimeError::InvalidTransition {
                id: self.id.clone(),
                from: self.state.as_str().to_string(),
                to: new_state.as_str().to_string(),
            });
        }

        tracing::info!(
            agent_id = %self.id,
            from = %self.state,
            to = %new_state,
            transitions = self.transition_count,
            "agent state transition"
        );

        self.state = new_state;
        self.last_active = Instant::now();
        self.transition_count += 1;
        Ok(())
    }

    /// Get agent uptime since creation.
    /// Memory cost: 0
    pub fn uptime(&self) -> std::time::Duration {
        self.created_at.elapsed()
    }

    /// Get time since last activity.
    /// Memory cost: 0
    pub fn idle_time(&self) -> std::time::Duration {
        self.last_active.elapsed()
    }

    /// Check if agent is in a terminal or inactive state.
    /// Memory cost: 0
    pub fn is_done(&self) -> bool {
        matches!(self.state, AgentState::Completing | AgentState::Crashed)
    }

    /// Check if agent is actively working (not idle, not done).
    /// Memory cost: 0
    pub fn is_active(&self) -> bool {
        !matches!(self.state, AgentState::Spawned | AgentState::Completing | AgentState::Crashed)
    }

    /// Get current memory usage in bytes.
    /// Memory cost: 0 (reads atomic counter)
    pub fn memory_usage(&self) -> u64 {
        self.memory_tracker.current_usage()
    }

    /// Check if agent memory is within budget (140 MB peak).
    /// Memory cost: 0
    pub fn is_within_budget(&self) -> bool {
        self.memory_tracker.is_within_budget()
    }

    /// Grant a capability to this agent.
    /// Memory cost: ~64 bytes per capability string
    pub fn grant_capability(&mut self, capability: String) {
        if !self.capabilities.contains(&capability) {
            tracing::info!(agent_id = %self.id, capability = %capability, "capability granted");
            self.capabilities.push(capability);
        }
    }

    /// Check if agent has a specific capability.
    /// Memory cost: 0
    pub fn has_capability(&self, capability: &str) -> bool {
        self.capabilities.iter().any(|c| c == capability)
    }
}

/// Lightweight, serializable snapshot of an agent's current state.
/// Used by monitor() and list_agents() APIs.
/// Memory cost: ~256 bytes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSnapshot {
    pub id: String,
    pub goal: String,
    pub state: AgentState,
    pub priority: AgentPriority,
    pub parent_id: Option<String>,
    pub memory_usage_bytes: u64,
    pub memory_peak_limit_bytes: u64,
    pub capabilities_count: usize,
    pub transition_count: u64,
    pub uptime_ms: u64,
    pub idle_time_ms: u64,
}

impl AgentSnapshot {
    /// Create a snapshot from a live agent.
    /// Memory cost: ~256 bytes (clones strings)
    pub fn from_agent(agent: &Agent) -> Self {
        Self {
            id: agent.id.clone(),
            goal: agent.goal.clone(),
            state: agent.state,
            priority: agent.priority,
            parent_id: agent.parent_id.clone(),
            memory_usage_bytes: agent.memory_usage(),
            memory_peak_limit_bytes: agent.memory_model.peak_limit_bytes,
            capabilities_count: agent.capabilities.len(),
            transition_count: agent.transition_count,
            uptime_ms: agent.uptime().as_millis() as u64,
            idle_time_ms: agent.idle_time().as_millis() as u64,
        }
    }
}

impl std::fmt::Display for AgentSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{}] {} | state={} pri={} mem={:.1}MB uptime={}ms",
            self.id,
            self.goal,
            self.state,
            self.priority,
            self.memory_usage_bytes as f64 / (1024.0 * 1024.0),
            self.uptime_ms,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_spawn_zero_trust() {
        let agent = Agent::new(AgentConfig::new("test goal"));
        assert_eq!(agent.id.len(), 12); // 12-char ID per agent.md
        assert!(agent.capabilities.is_empty()); // Zero trust
        assert_eq!(agent.state, AgentState::Spawned);
        assert_eq!(agent.transition_count, 0);
    }

    #[test]
    fn test_valid_transitions() {
        let mut agent = Agent::new(AgentConfig::new("test"));
        // Spawned → Planning
        assert!(agent.transition(AgentState::Planning).is_ok());
        // Planning → Thinking
        assert!(agent.transition(AgentState::Thinking).is_ok());
        // Thinking → Reviewing
        assert!(agent.transition(AgentState::Reviewing).is_ok());
        // Reviewing → Completing
        assert!(agent.transition(AgentState::Completing).is_ok());
        assert_eq!(agent.transition_count, 4);
    }

    #[test]
    fn test_invalid_transitions() {
        let mut agent = Agent::new(AgentConfig::new("test"));
        // Spawned → Completing (invalid: must go through Planning first)
        assert!(agent.transition(AgentState::Completing).is_err());
        // Spawned → ToolUsing (invalid: must plan first)
        assert!(agent.transition(AgentState::ToolUsing).is_err());
    }

    #[test]
    fn test_crash_from_any_state() {
        let mut agent = Agent::new(AgentConfig::new("test"));
        agent.transition(AgentState::Planning).unwrap();
        // Any state can crash
        assert!(agent.transition(AgentState::Crashed).is_ok());
        // Crashed → Spawned (auto-recovery)
        assert!(agent.transition(AgentState::Spawned).is_ok());
    }

    #[test]
    fn test_crash_recovery_only_to_spawned() {
        let mut agent = Agent::new(AgentConfig::new("test"));
        agent.transition(AgentState::Planning).unwrap();
        agent.transition(AgentState::Crashed).unwrap();
        // Crashed → Planning (invalid: must go through Spawned)
        assert!(agent.transition(AgentState::Planning).is_err());
    }

    #[test]
    fn test_completing_is_terminal() {
        let mut agent = Agent::new(AgentConfig::new("test"));
        agent.transition(AgentState::Planning).unwrap();
        agent.transition(AgentState::Thinking).unwrap();
        agent.transition(AgentState::Completing).unwrap();
        // Completing is terminal
        assert!(agent.transition(AgentState::Planning).is_err());
        // But can still crash
        assert!(agent.transition(AgentState::Crashed).is_ok());
    }

    #[test]
    fn test_agent_config_builder() {
        let config = AgentConfig::new("solve puzzle")
            .with_parent("parent123")
            .with_priority(AgentPriority::High);
        let agent = Agent::new(config);
        assert_eq!(agent.goal, "solve puzzle");
        assert_eq!(agent.parent_id.as_deref(), Some("parent123"));
        assert_eq!(agent.priority, AgentPriority::High);
    }

    #[test]
    fn test_snapshot() {
        let agent = Agent::new(AgentConfig::new("test goal"));
        let snap = AgentSnapshot::from_agent(&agent);
        assert_eq!(snap.id, agent.id);
        assert_eq!(snap.goal, "test goal");
        assert_eq!(snap.state, AgentState::Spawned);
        assert_eq!(snap.capabilities_count, 0);
    }

    #[test]
    fn test_is_active() {
        let mut agent = Agent::new(AgentConfig::new("test"));
        assert!(!agent.is_active()); // Spawned = not active yet
        agent.transition(AgentState::Planning).unwrap();
        assert!(agent.is_active());
        agent.transition(AgentState::Thinking).unwrap();
        assert!(agent.is_active());
        agent.transition(AgentState::Completing).unwrap();
        assert!(!agent.is_active()); // Done
    }
}

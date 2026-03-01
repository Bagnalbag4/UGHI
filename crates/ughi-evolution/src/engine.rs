// UGHI-evolution/src/engine.rs
// Follows self_evolution.md | Unified Evolution Engine facade
// Orchestrates: lessons + patterns + meta-agent
// Memory cost: ~16 KB total (all 3 subsystems)

use serde::{Deserialize, Serialize};
use tracing::info;

use crate::lessons::LessonStore;
use crate::meta_agent::{MetaAgent, MetaMetrics, Proposal};
use crate::patterns::PatternTracker;

/// The Self-Evolution Engine – makes UGHI alive.
/// Integrates mistake-learning + usage-learning + meta-evolution.
pub struct EvolutionEngine {
    pub lessons: LessonStore,
    pub patterns: PatternTracker,
    pub meta_agent: MetaAgent,
}

impl EvolutionEngine {
    pub fn new() -> Self {
        info!("Self-Evolution Engine initialized");
        Self {
            lessons: LessonStore::new(),
            patterns: PatternTracker::new(),
            meta_agent: MetaAgent::new(),
        }
    }

    /// Record a completed task (learns from both success and failure).
    pub fn on_task_complete(
        &mut self,
        agent_id: &str,
        goal: &str,
        expert_persona: &str,
        skills_used: &[&str],
        succeeded: bool,
        error_msg: Option<&str>,
        execution_ms: u64,
    ) -> bool {
        // Learn from mistakes/successes
        let domain = expert_persona.split('-').next().unwrap_or("general");
        self.lessons
            .auto_reflect(agent_id, goal, domain, succeeded, error_msg);

        // Track usage patterns
        self.patterns
            .record(goal, expert_persona, skills_used, succeeded, execution_ms);

        // Check if auto-evolve should trigger
        self.meta_agent.on_task_complete()
    }

    /// Run the evolution cycle (manually or after threshold).
    pub fn evolve(&mut self) -> Vec<&Proposal> {
        self.meta_agent.evolve(&self.lessons, &self.patterns)
    }

    /// Get the evolution score (0-100).
    pub fn evolution_score(&self) -> f64 {
        let win_rate = self.patterns.system_win_rate();
        let failure_rate = self.lessons.failure_rate();
        let lesson_count = self.lessons.count() as f64;
        let evolution_count = self.meta_agent.metrics().total_evolutions as f64;

        // Score formula: win_rate weighted heavily + lessons learned + evolutions
        let base = win_rate * 60.0;
        let learning_bonus = (lesson_count.sqrt() * 5.0).min(20.0);
        let evolution_bonus = (evolution_count * 5.0).min(20.0);
        let failure_penalty = failure_rate * 10.0;

        (base + learning_bonus + evolution_bonus - failure_penalty).clamp(0.0, 100.0)
    }

    /// Get full metrics snapshot.
    pub fn metrics(&self) -> EvolutionMetrics {
        EvolutionMetrics {
            score: self.evolution_score(),
            lessons_total: self.lessons.count() as u64,
            lessons_today: self.lessons.count() as u64, // Simplified: would filter by date
            patterns_total: self.patterns.count() as u64,
            system_win_rate: self.patterns.system_win_rate(),
            failure_rate: self.lessons.failure_rate(),
            meta: self.meta_agent.metrics(),
        }
    }

    /// Rollback to a previous version.
    pub fn rollback(&mut self, version: &str) -> bool {
        self.meta_agent.rollback(version)
    }

    /// Current version.
    pub fn version(&self) -> &str {
        self.meta_agent.version()
    }
}

/// Full evolution metrics for dashboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvolutionMetrics {
    pub score: f64,
    pub lessons_total: u64,
    pub lessons_today: u64,
    pub patterns_total: u64,
    pub system_win_rate: f64,
    pub failure_rate: f64,
    pub meta: MetaMetrics,
}

impl Default for EvolutionMetrics {
    fn default() -> Self {
        Self {
            score: 60.0,
            lessons_total: 0,
            lessons_today: 0,
            patterns_total: 0,
            system_win_rate: 1.0,
            failure_rate: 0.0,
            meta: MetaMetrics::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_new() {
        let engine = EvolutionEngine::new();
        assert_eq!(engine.version(), "1.0.0");
        assert!(engine.evolution_score() >= 0.0);
    }

    #[test]
    fn test_on_task_complete() {
        let mut engine = EvolutionEngine::new();
        let trigger = engine.on_task_complete(
            "a1",
            "build web app",
            "fullstack-dev",
            &["code_executor"],
            true,
            None,
            100,
        );
        assert!(!trigger); // Not yet at threshold
        assert_eq!(engine.lessons.count(), 1);
        assert_eq!(engine.patterns.count(), 1);
    }

    #[test]
    fn test_evolve() {
        let mut engine = EvolutionEngine::new();
        engine.on_task_complete(
            "a1",
            "t1",
            "rust-kernel",
            &["code_executor"],
            true,
            None,
            100,
        );
        engine.on_task_complete(
            "a2",
            "t2",
            "fullstack-dev",
            &["browser_control"],
            false,
            Some("timeout"),
            500,
        );

        let _proposals = engine.evolve();
        // Should have proposals based on failure analysis
        assert!(engine.version() == "1.1.0");
    }

    #[test]
    fn test_evolution_score() {
        let mut engine = EvolutionEngine::new();
        // All successes = high score
        for i in 0..5 {
            engine.on_task_complete(&format!("a{}", i), "task", "e", &[], true, None, 100);
        }
        assert!(engine.evolution_score() > 50.0);
    }

    #[test]
    fn test_rollback() {
        let mut engine = EvolutionEngine::new();
        engine.evolve(); // 1.1.0
        engine.evolve(); // 1.2.0
        assert!(engine.rollback("1.1.0"));
        assert_eq!(engine.version(), "1.1.0");
    }
}

// UGHI-evolution/src/lib.rs
// Follows self_evolution.md | Self-Evolution Intelligence Engine
// "UGHI is not static. It is a living organism." – self_evolution.md
// Memory: ~16 KB total | All evolution local & safe
//
// Module structure:
// - lessons:    Mistake-driven 4-step reflection
// - patterns:   Usage-driven pattern tracking
// - meta_agent: Meta-evolution proposals + rollback
// - engine:     Unified facade

pub mod engine;
pub mod lessons;
pub mod meta_agent;
pub mod patterns;

pub use engine::{EvolutionEngine, EvolutionMetrics};
pub use lessons::{Lesson, LessonStore};
pub use meta_agent::{MetaAgent, MetaMetrics, Proposal, ProposalCategory, ProposalStatus};
pub use patterns::{PatternTracker, UsagePattern};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_full_pipeline() {
        let mut engine = EvolutionEngine::new();

        // Simulate 10 tasks
        for i in 0..10 {
            let succeeded = i % 3 != 0; // 30% failure rate
            engine.on_task_complete(
                &format!("agent-{}", i),
                &format!("task {}", i),
                "fullstack-dev",
                &["code_executor", "web_search"],
                succeeded,
                if succeeded {
                    None
                } else {
                    Some("timeout error")
                },
                100 + i * 10,
            );
        }

        // Run evolution
        let _proposals = engine.evolve();
        let metrics = engine.metrics();

        assert!(metrics.lessons_total >= 10);
        assert!(metrics.score > 0.0);
        assert_eq!(engine.version(), "1.1.0");
    }
}

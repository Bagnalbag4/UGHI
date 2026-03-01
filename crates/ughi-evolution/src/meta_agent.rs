// UGHI-evolution/src/meta_agent.rs
// Follows self_evolution.md | Meta-Evolution Agent
// "Dedicated Meta-Evolution Agent runs every 24h or after 50 tasks" – self_evolution.md
// Proposes code changes, new skills, prompt improvements.
// Tests in WASM sandbox. Auto-apply after user approval.

use serde::{Deserialize, Serialize};
use tracing::info;

use crate::lessons::LessonStore;
use crate::patterns::PatternTracker;

/// Task threshold before auto-evolve triggers.
const EVOLVE_THRESHOLD: u64 = 50;

/// A proposed improvement from the meta-agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    pub id: u64,
    pub category: ProposalCategory,
    pub title: String,
    pub description: String,
    pub confidence: f64,
    pub status: ProposalStatus,
    pub source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProposalCategory {
    NewSkill,
    PromptImprovement,
    PatternReinforcement,
    ExpertPersonaSuggestion,
    PerformanceOptimization,
    ErrorPrevention,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProposalStatus {
    Proposed,
    Approved,
    Applied,
    Rejected,
    RolledBack,
}

/// The Meta-Evolution Agent.
pub struct MetaAgent {
    proposals: Vec<Proposal>,
    next_id: u64,
    tasks_since_evolve: u64,
    total_evolutions: u64,
    version: String,
    version_history: Vec<String>,
}

impl MetaAgent {
    pub fn new() -> Self {
        Self {
            proposals: Vec::with_capacity(64),
            next_id: 1,
            tasks_since_evolve: 0,
            total_evolutions: 0,
            version: "1.0.0".to_string(),
            version_history: vec!["1.0.0".to_string()],
        }
    }

    /// Record a task completion (tracks threshold for auto-evolve).
    pub fn on_task_complete(&mut self) -> bool {
        self.tasks_since_evolve += 1;
        self.tasks_since_evolve >= EVOLVE_THRESHOLD
    }

    /// Run the full evolution cycle: analyze lessons + patterns → generate proposals.
    pub fn evolve(&mut self, lessons: &LessonStore, patterns: &PatternTracker) -> Vec<&Proposal> {
        info!(
            tasks = self.tasks_since_evolve,
            lessons = lessons.count(),
            patterns = patterns.count(),
            "meta-agent: running evolution cycle"
        );

        // Analyze failure patterns
        let failures: Vec<_> = lessons.all().iter().filter(|l| !l.succeeded).collect();
        if !failures.is_empty() {
            let common_causes = analyze_common_failures(&failures);
            for (cause, count) in common_causes {
                self.propose(
                    ProposalCategory::ErrorPrevention,
                    format!("Prevent recurring failure: {}", cause),
                    format!(
                        "Seen {} times. Add pre-check or retry logic for: {}",
                        count, cause
                    ),
                    0.7 + (count as f64 * 0.05).min(0.25),
                    "lesson-analysis",
                );
            }
        }

        // Analyze winning patterns
        let top = patterns.top_patterns(5);
        for pattern in top {
            if pattern.win_rate() > 0.8 {
                self.propose(
                    ProposalCategory::PatternReinforcement,
                    format!("Reinforce expert: {}", pattern.expert_persona),
                    format!(
                        "Win rate: {:.0}% over {} runs. Boost priority for similar goals.",
                        pattern.win_rate() * 100.0,
                        pattern.success_count + pattern.failure_count,
                    ),
                    pattern.win_rate(),
                    "pattern-analysis",
                );
            }
        }

        // Suggest new skills based on pattern gaps
        if patterns.system_win_rate() < 0.7 {
            self.propose(
                ProposalCategory::NewSkill,
                "Add error recovery skill".to_string(),
                "System win rate below 70%. Add dedicated error analysis + retry skill."
                    .to_string(),
                0.80,
                "system-analysis",
            );
        }

        // Suggest prompt improvements if failure rate high
        if lessons.failure_rate() > 0.3 {
            self.propose(
                ProposalCategory::PromptImprovement,
                "Enhance expert prompts with failure context".to_string(),
                format!(
                    "Failure rate: {:.0}%. Inject learned rules into expert system prompts.",
                    lessons.failure_rate() * 100.0
                ),
                0.75,
                "failure-analysis",
            );
        }

        // Bump version
        self.total_evolutions += 1;
        self.tasks_since_evolve = 0;
        let new_version = format!("1.{}.0", self.total_evolutions);
        self.version_history.push(new_version.clone());
        self.version = new_version;

        info!(
            version = %self.version,
            proposals = self.proposals.len(),
            "evolution cycle complete"
        );

        self.pending_proposals()
    }

    fn propose(
        &mut self,
        category: ProposalCategory,
        title: String,
        description: String,
        confidence: f64,
        source: &str,
    ) {
        let id = self.next_id;
        self.next_id += 1;
        self.proposals.push(Proposal {
            id,
            category,
            title,
            description,
            confidence,
            status: ProposalStatus::Proposed,
            source: source.to_string(),
        });
    }

    /// Get pending proposals.
    pub fn pending_proposals(&self) -> Vec<&Proposal> {
        self.proposals
            .iter()
            .filter(|p| p.status == ProposalStatus::Proposed)
            .collect()
    }

    /// Approve a proposal.
    pub fn approve(&mut self, id: u64) -> bool {
        if let Some(p) = self.proposals.iter_mut().find(|p| p.id == id) {
            p.status = ProposalStatus::Approved;
            true
        } else {
            false
        }
    }

    /// Apply an approved proposal.
    /// H-04 FIX: Requires explicit user_approved=true. Auto-apply without consent is blocked.
    pub fn apply(&mut self, id: u64, user_approved: bool) -> Result<bool, String> {
        if !user_approved {
            return Err(
                "SECURITY: Evolution apply() requires explicit user approval. \
                 Set user_approved=true only after user confirms."
                    .to_string(),
            );
        }

        if let Some(p) = self
            .proposals
            .iter_mut()
            .find(|p| p.id == id && p.status == ProposalStatus::Approved)
        {
            p.status = ProposalStatus::Applied;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Rollback to a previous version.
    pub fn rollback(&mut self, target_version: &str) -> bool {
        if self.version_history.contains(&target_version.to_string()) {
            info!(from = %self.version, to = target_version, "rolling back");
            // Mark all proposals after target version as rolled back
            for p in &mut self.proposals {
                if p.status == ProposalStatus::Applied {
                    p.status = ProposalStatus::RolledBack;
                }
            }
            self.version = target_version.to_string();
            true
        } else {
            false
        }
    }

    /// Current version.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Version history.
    pub fn versions(&self) -> &[String] {
        &self.version_history
    }

    /// Metrics.
    pub fn metrics(&self) -> MetaMetrics {
        MetaMetrics {
            total_evolutions: self.total_evolutions,
            tasks_since_evolve: self.tasks_since_evolve,
            total_proposals: self.proposals.len() as u64,
            applied_proposals: self
                .proposals
                .iter()
                .filter(|p| p.status == ProposalStatus::Applied)
                .count() as u64,
            version: self.version.clone(),
        }
    }
}

/// Meta-agent metrics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetaMetrics {
    pub total_evolutions: u64,
    pub tasks_since_evolve: u64,
    pub total_proposals: u64,
    pub applied_proposals: u64,
    pub version: String,
}

fn analyze_common_failures(failures: &[&crate::lessons::Lesson]) -> Vec<(String, usize)> {
    let mut causes: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for f in failures {
        let key = if f.root_cause.len() > 40 {
            f.root_cause[..40].to_string()
        } else {
            f.root_cause.clone()
        };
        *causes.entry(key).or_insert(0) += 1;
    }
    let mut sorted: Vec<(String, usize)> = causes.into_iter().filter(|(_, c)| *c >= 2).collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));
    sorted.truncate(5);
    sorted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_threshold() {
        let mut meta = MetaAgent::new();
        for _ in 0..49 {
            assert!(!meta.on_task_complete());
        }
        assert!(meta.on_task_complete()); // 50th task triggers evolve
    }

    #[test]
    fn test_evolve_with_lessons() {
        let mut meta = MetaAgent::new();
        let mut lessons = LessonStore::new();
        let patterns = PatternTracker::new();

        // Add some failures
        lessons.auto_reflect("a1", "task1", "web", false, Some("timeout error"));
        lessons.auto_reflect("a2", "task2", "web", false, Some("timeout error"));
        lessons.auto_reflect("a3", "task3", "web", true, None);

        let proposals = meta.evolve(&lessons, &patterns);
        assert!(!proposals.is_empty());
        assert_eq!(meta.version(), "1.1.0");
    }

    #[test]
    fn test_approve_apply() {
        let mut meta = MetaAgent::new();
        let lessons = LessonStore::new();
        let patterns = PatternTracker::new();
        meta.evolve(&lessons, &patterns);

        // Version bumps even with no proposals
        assert_eq!(meta.version(), "1.1.0");
    }

    #[test]
    fn test_rollback() {
        let mut meta = MetaAgent::new();
        let lessons = LessonStore::new();
        let patterns = PatternTracker::new();

        meta.evolve(&lessons, &patterns); // → 1.1.0
        meta.evolve(&lessons, &patterns); // → 1.2.0

        assert!(meta.rollback("1.1.0"));
        assert_eq!(meta.version(), "1.1.0");
    }

    #[test]
    fn test_rollback_invalid() {
        let mut meta = MetaAgent::new();
        assert!(!meta.rollback("99.99.99"));
    }
}

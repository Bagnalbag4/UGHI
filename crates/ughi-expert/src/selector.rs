// UGHI-expert/src/selector.rs
// Follows strict_rules.md | "Every agent MUST spawn with world-class expert persona"
// Auto-selects best expert from goal text using keyword scoring.
// Memory cost: ~64 bytes per selection (stack only)
// Latency: < 1ms (linear scan of 50 static personas)

use crate::persona::{ExpertPersona, EXPERT_PERSONAS};
use tracing::info;

/// Score result for an expert match.
#[derive(Debug)]
struct MatchScore {
    index: usize,
    score: f32,
}

/// Select the best expert persona for a given goal text.
/// Uses keyword frequency + specialization weighting.
/// Memory cost: ~4 KB (stack, 50 scores)
/// Latency: < 1ms
pub fn select_expert(goal: &str) -> &'static ExpertPersona {
    let goal_lower = goal.to_lowercase();

    let mut best_idx = EXPERT_PERSONAS.len() - 1; // Default: GeneralGenius (last)
    let mut best_score: f32 = 0.0;

    for (i, persona) in EXPERT_PERSONAS.iter().enumerate() {
        let mut score: f32 = 0.0;

        for keyword in persona.keywords {
            if goal_lower.contains(keyword) {
                score += 1.0;
            }
        }

        // Weight by specialization
        score *= persona.specialization;

        if score > best_score {
            best_score = score;
            best_idx = i;
        }
    }

    let selected = &EXPERT_PERSONAS[best_idx];
    info!(
        expert = selected.name,
        score = best_score,
        "expert persona selected for goal"
    );
    selected
}

/// Select top-N experts for a complex multi-domain goal.
/// Returns experts sorted by relevance score (highest first).
pub fn select_team(goal: &str, max_experts: usize) -> Vec<&'static ExpertPersona> {
    let goal_lower = goal.to_lowercase();

    let mut scores: Vec<MatchScore> = EXPERT_PERSONAS
        .iter()
        .enumerate()
        .map(|(i, persona)| {
            let mut score: f32 = 0.0;
            for keyword in persona.keywords {
                if goal_lower.contains(keyword) {
                    score += 1.0;
                }
            }
            score *= persona.specialization;
            MatchScore { index: i, score }
        })
        .filter(|m| m.score > 0.0)
        .collect();

    scores.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    scores.truncate(max_experts);

    // Always include at least the general genius
    if scores.is_empty() {
        return vec![&EXPERT_PERSONAS[EXPERT_PERSONAS.len() - 1]];
    }

    scores.iter().map(|m| &EXPERT_PERSONAS[m.index]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_select_rust_expert() {
        let expert = select_expert("Build a high-performance Rust kernel module");
        assert_eq!(expert.id, "rust-kernel");
    }

    #[test]
    fn test_select_web_expert() {
        let expert = select_expert("Create a React frontend with Next.js and TypeScript");
        assert_eq!(expert.id, "fullstack-dev");
    }

    #[test]
    fn test_select_startup_expert() {
        let expert = select_expert("Mera startup plan banao, fundraise karo, investor pitch");
        assert_eq!(expert.id, "startup-advisor");
    }

    #[test]
    fn test_select_fallback() {
        let expert = select_expert("xyz zzz qqq");
        assert_eq!(expert.id, "general-genius");
    }

    #[test]
    fn test_select_team() {
        let team = select_team(
            "Build a web app with API, deploy to cloud, and market it",
            5,
        );
        assert!(team.len() >= 1);
        assert!(team.len() <= 5);
    }

    #[test]
    fn test_select_team_empty() {
        let team = select_team("xyz zzz qqq", 3);
        assert_eq!(team.len(), 1); // Fallback to general genius
    }
}

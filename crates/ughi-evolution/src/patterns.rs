// UGHI-evolution/src/patterns.rs
// Follows self_evolution.md | Usage-Driven Learning
// "Track successful patterns, reinforce winning strategies" – self_evolution.md
// Memory cost: ~128 bytes per pattern

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::info;

/// A tracked usage pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsagePattern {
    pub id: u64,
    pub goal_keywords: Vec<String>,
    pub expert_persona: String,
    pub skills_used: Vec<String>,
    pub success_count: u64,
    pub failure_count: u64,
    pub avg_execution_ms: u64,
    pub last_used_ms: u64,
}

impl UsagePattern {
    /// Win rate for this pattern.
    pub fn win_rate(&self) -> f64 {
        let total = self.success_count + self.failure_count;
        if total == 0 {
            return 0.0;
        }
        self.success_count as f64 / total as f64
    }
}

/// Pattern tracker – learns from usage.
pub struct PatternTracker {
    patterns: Vec<UsagePattern>,
    /// Goal keyword → pattern IDs for fast lookup
    keyword_index: HashMap<String, Vec<u64>>,
    next_id: u64,
}

impl PatternTracker {
    pub fn new() -> Self {
        Self {
            patterns: Vec::with_capacity(128),
            keyword_index: HashMap::new(),
            next_id: 1,
        }
    }

    /// Record a task execution as a pattern.
    pub fn record(
        &mut self,
        goal: &str,
        expert_persona: &str,
        skills_used: &[&str],
        succeeded: bool,
        execution_ms: u64,
    ) {
        let keywords = extract_keywords(goal);

        // Try to find existing pattern with same expert + similar keywords
        if let Some(pattern) = self.find_matching_mut(&keywords, expert_persona) {
            if succeeded {
                pattern.success_count += 1;
            } else {
                pattern.failure_count += 1;
            }
            let total = pattern.success_count + pattern.failure_count;
            pattern.avg_execution_ms =
                (pattern.avg_execution_ms * (total - 1) + execution_ms) / total;
            pattern.last_used_ms = current_time_ms();
            info!(id = pattern.id, expert = expert_persona, "pattern updated");
            return;
        }

        // Create new pattern
        let id = self.next_id;
        self.next_id += 1;

        for kw in &keywords {
            self.keyword_index.entry(kw.clone()).or_default().push(id);
        }

        self.patterns.push(UsagePattern {
            id,
            goal_keywords: keywords,
            expert_persona: expert_persona.to_string(),
            skills_used: skills_used.iter().map(|s| s.to_string()).collect(),
            success_count: if succeeded { 1 } else { 0 },
            failure_count: if succeeded { 0 } else { 1 },
            avg_execution_ms: execution_ms,
            last_used_ms: current_time_ms(),
        });

        info!(id, expert = expert_persona, "new pattern created");
    }

    fn find_matching_mut(
        &mut self,
        keywords: &[String],
        expert: &str,
    ) -> Option<&mut UsagePattern> {
        self.patterns.iter_mut().find(|p| {
            p.expert_persona == expert && keywords.iter().any(|k| p.goal_keywords.contains(k))
        })
    }

    /// Recommend the best expert for a goal based on historical patterns.
    pub fn recommend_expert(&self, goal: &str) -> Option<&str> {
        let keywords = extract_keywords(goal);
        let mut best: Option<(&UsagePattern, f64)> = None;

        for pattern in &self.patterns {
            let keyword_overlap = keywords
                .iter()
                .filter(|k| pattern.goal_keywords.contains(k))
                .count() as f64;

            if keyword_overlap > 0.0 {
                let score =
                    keyword_overlap * pattern.win_rate() * (pattern.success_count as f64).sqrt();
                if best.is_none() || score > best.unwrap().1 {
                    best = Some((pattern, score));
                }
            }
        }

        best.map(|(p, _)| p.expert_persona.as_str())
    }

    /// Get top-N winning patterns (highest win rate with minimum 2 runs).
    pub fn top_patterns(&self, n: usize) -> Vec<&UsagePattern> {
        let mut eligible: Vec<&UsagePattern> = self
            .patterns
            .iter()
            .filter(|p| p.success_count + p.failure_count >= 2)
            .collect();
        eligible.sort_by(|a, b| {
            b.win_rate()
                .partial_cmp(&a.win_rate())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        eligible.truncate(n);
        eligible
    }

    /// Get all patterns.
    pub fn all(&self) -> &[UsagePattern] {
        &self.patterns
    }

    pub fn count(&self) -> usize {
        self.patterns.len()
    }

    /// Overall system win rate.
    pub fn system_win_rate(&self) -> f64 {
        let total_s: u64 = self.patterns.iter().map(|p| p.success_count).sum();
        let total_f: u64 = self.patterns.iter().map(|p| p.failure_count).sum();
        let total = total_s + total_f;
        if total == 0 {
            1.0
        } else {
            total_s as f64 / total as f64
        }
    }
}

/// Extract meaningful keywords from a goal string.
fn extract_keywords(goal: &str) -> Vec<String> {
    let stop_words = [
        "a", "an", "the", "is", "are", "was", "be", "to", "of", "and", "in", "for", "on", "with",
        "at", "by", "from", "it", "this", "that", "my", "mera", "karo", "banao", "kar", "do",
        "hai", "ko", "se", "me", "ke", "ka", "ki",
    ];

    goal.to_lowercase()
        .split_whitespace()
        .filter(|w| w.len() > 2 && !stop_words.contains(w))
        .take(10)
        .map(|w| w.to_string())
        .collect()
}

fn current_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_pattern() {
        let mut tracker = PatternTracker::new();
        tracker.record(
            "build web app",
            "fullstack-dev",
            &["code_executor"],
            true,
            100,
        );
        assert_eq!(tracker.count(), 1);
    }

    #[test]
    fn test_pattern_updates() {
        let mut tracker = PatternTracker::new();
        tracker.record(
            "build web app",
            "fullstack-dev",
            &["code_executor"],
            true,
            100,
        );
        tracker.record(
            "build web frontend",
            "fullstack-dev",
            &["browser_control"],
            true,
            200,
        );
        // Should update existing pattern (same expert + overlapping keywords "build", "web")
        assert_eq!(tracker.count(), 1);
        assert_eq!(tracker.all()[0].success_count, 2);
    }

    #[test]
    fn test_recommend_expert() {
        let mut tracker = PatternTracker::new();
        tracker.record(
            "build rust kernel",
            "rust-kernel",
            &["code_executor"],
            true,
            100,
        );
        tracker.record(
            "build web app",
            "fullstack-dev",
            &["browser_control"],
            true,
            200,
        );

        let rec = tracker.recommend_expert("build a new rust module");
        assert_eq!(rec, Some("rust-kernel"));
    }

    #[test]
    fn test_win_rate() {
        let mut tracker = PatternTracker::new();
        tracker.record("test goal", "general-genius", &[], true, 100);
        tracker.record("test goal again", "general-genius", &[], false, 100);
        assert!((tracker.system_win_rate() - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_extract_keywords() {
        let kw = extract_keywords("Build a web application for my startup");
        assert!(kw.contains(&"build".to_string()));
        assert!(kw.contains(&"web".to_string()));
        assert!(!kw.contains(&"a".to_string())); // stop word
    }
}

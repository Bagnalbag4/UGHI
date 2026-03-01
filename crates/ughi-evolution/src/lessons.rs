// UGHI-evolution/src/lessons.rs
// Follows self_evolution.md | Mistake-Driven Learning
// "Har failed/completed task pe 4-step reflection" – self_evolution.md
// Memory cost: ~256 bytes per lesson

use serde::{Deserialize, Serialize};
use tracing::info;

/// A lesson learned from a task (success or failure).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lesson {
    pub id: u64,
    pub agent_id: String,
    pub goal: String,
    pub domain: String,
    pub succeeded: bool,
    /// Step 1: What went wrong (or right)?
    pub what_happened: String,
    /// Step 2: Why did it happen? (root cause)
    pub root_cause: String,
    /// Step 3: What should be done differently?
    pub improvement: String,
    /// Step 4: Actionable rule to remember
    pub rule: String,
    pub timestamp_ms: u64,
    pub times_applied: u64,
}

/// Lesson store – persists all learnings.
/// Memory cost: ~256 bytes per lesson in memory
pub struct LessonStore {
    lessons: Vec<Lesson>,
    next_id: u64,
}

impl LessonStore {
    pub fn new() -> Self {
        Self {
            lessons: Vec::with_capacity(256),
            next_id: 1,
        }
    }

    /// Record a 4-step reflection from a completed task.
    pub fn record(
        &mut self,
        agent_id: &str,
        goal: &str,
        domain: &str,
        succeeded: bool,
        what_happened: &str,
        root_cause: &str,
        improvement: &str,
        rule: &str,
    ) -> u64 {
        let id = self.next_id;
        self.next_id += 1;

        let lesson = Lesson {
            id,
            agent_id: agent_id.to_string(),
            goal: goal.to_string(),
            domain: domain.to_string(),
            succeeded,
            what_happened: what_happened.to_string(),
            root_cause: root_cause.to_string(),
            improvement: improvement.to_string(),
            rule: rule.to_string(),
            timestamp_ms: current_time_ms(),
            times_applied: 0,
        };

        info!(id, agent_id, succeeded, domain, "lesson recorded");
        self.lessons.push(lesson);
        id
    }

    /// Auto-reflect on a task result (generates 4-step reflection).
    pub fn auto_reflect(
        &mut self,
        agent_id: &str,
        goal: &str,
        domain: &str,
        succeeded: bool,
        error_msg: Option<&str>,
    ) -> u64 {
        let (what, why, how, rule) = if succeeded {
            (
                format!("Task '{}' completed successfully", truncate(goal, 50)),
                "Agent followed correct workflow with expert persona".to_string(),
                "Reinforce this pattern for similar goals".to_string(),
                format!("For '{}' goals, current approach works well", domain),
            )
        } else {
            let err = error_msg.unwrap_or("unknown error");
            (
                format!("Task '{}' failed: {}", truncate(goal, 40), err),
                classify_root_cause(err),
                suggest_improvement(err),
                format!(
                    "Avoid: {} | Fix: {}",
                    truncate(err, 30),
                    suggest_improvement(err)
                ),
            )
        };

        self.record(agent_id, goal, domain, succeeded, &what, &why, &how, &rule)
    }

    /// Search lessons relevant to a goal.
    pub fn search(&self, query: &str) -> Vec<&Lesson> {
        let q = query.to_lowercase();
        self.lessons
            .iter()
            .filter(|l| {
                l.goal.to_lowercase().contains(&q)
                    || l.domain.to_lowercase().contains(&q)
                    || l.rule.to_lowercase().contains(&q)
            })
            .collect()
    }

    /// Get all lessons.
    pub fn all(&self) -> &[Lesson] {
        &self.lessons
    }

    /// Get lessons count.
    pub fn count(&self) -> usize {
        self.lessons.len()
    }

    /// Get failure rate.
    pub fn failure_rate(&self) -> f64 {
        if self.lessons.is_empty() {
            return 0.0;
        }
        let failures = self.lessons.iter().filter(|l| !l.succeeded).count();
        failures as f64 / self.lessons.len() as f64
    }

    /// Get top rules (most applied).
    pub fn top_rules(&self, n: usize) -> Vec<&Lesson> {
        let mut sorted: Vec<&Lesson> = self.lessons.iter().collect();
        sorted.sort_by(|a, b| b.times_applied.cmp(&a.times_applied));
        sorted.truncate(n);
        sorted
    }

    /// Mark a lesson as applied.
    pub fn mark_applied(&mut self, lesson_id: u64) {
        if let Some(lesson) = self.lessons.iter_mut().find(|l| l.id == lesson_id) {
            lesson.times_applied += 1;
        }
    }
}

fn classify_root_cause(err: &str) -> String {
    let e = err.to_lowercase();
    if e.contains("memory") || e.contains("oom") {
        "Memory budget exceeded – reduce working set".to_string()
    } else if e.contains("timeout") {
        "Operation timed out – break into smaller steps".to_string()
    } else if e.contains("sandbox") || e.contains("capability") {
        "Sandbox violation – need additional capabilities".to_string()
    } else if e.contains("not found") || e.contains("missing") {
        "Missing resource – ensure prerequisites exist".to_string()
    } else {
        format!("Unclassified error: {}", truncate(err, 60))
    }
}

fn suggest_improvement(err: &str) -> String {
    let e = err.to_lowercase();
    if e.contains("memory") {
        "Use streaming, reduce batch size".to_string()
    } else if e.contains("timeout") {
        "Add checkpoints, increase timeout, parallelize".to_string()
    } else if e.contains("sandbox") {
        "Request explicit capability tokens before execution".to_string()
    } else {
        "Add error handling, retry with backoff".to_string()
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() > max {
        format!("{}...", &s[..max.saturating_sub(3)])
    } else {
        s.to_string()
    }
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
    fn test_record_lesson() {
        let mut store = LessonStore::new();
        let id = store.record(
            "a1",
            "build web app",
            "web",
            true,
            "worked",
            "good plan",
            "keep it",
            "web apps: plan first",
        );
        assert_eq!(id, 1);
        assert_eq!(store.count(), 1);
    }

    #[test]
    fn test_auto_reflect_success() {
        let mut store = LessonStore::new();
        store.auto_reflect("a1", "deploy server", "devops", true, None);
        assert_eq!(store.count(), 1);
        assert!(store.all()[0].succeeded);
    }

    #[test]
    fn test_auto_reflect_failure() {
        let mut store = LessonStore::new();
        store.auto_reflect(
            "a1",
            "process data",
            "data",
            false,
            Some("memory budget exceeded"),
        );
        let lesson = &store.all()[0];
        assert!(!lesson.succeeded);
        assert!(lesson.root_cause.contains("Memory"));
    }

    #[test]
    fn test_search() {
        let mut store = LessonStore::new();
        store.record(
            "a1",
            "build rust kernel",
            "systems",
            true,
            "ok",
            "ok",
            "ok",
            "systems: benchmark",
        );
        store.record(
            "a2",
            "deploy web app",
            "web",
            true,
            "ok",
            "ok",
            "ok",
            "web: test first",
        );
        assert_eq!(store.search("web").len(), 1);
        assert_eq!(store.search("kernel").len(), 1);
    }

    #[test]
    fn test_failure_rate() {
        let mut store = LessonStore::new();
        store.auto_reflect("a1", "task1", "x", true, None);
        store.auto_reflect("a2", "task2", "x", false, Some("err"));
        assert!((store.failure_rate() - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_mark_applied() {
        let mut store = LessonStore::new();
        let id = store.record("a1", "g", "d", true, "w", "r", "i", "rule");
        store.mark_applied(id);
        store.mark_applied(id);
        assert_eq!(store.all()[0].times_applied, 2);
    }
}

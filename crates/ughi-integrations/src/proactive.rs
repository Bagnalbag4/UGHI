// UGHI-integrations/src/proactive.rs
// Proactive features: heartbeats, daily briefings, background tasks
// Memory: ~512 bytes per scheduled item

use serde::{Deserialize, Serialize};
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyBriefing {
    pub time_hour: u8,
    pub time_minute: u8,
    pub sections: Vec<BriefingSection>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BriefingSection {
    Weather,
    Calendar,
    TaskSummary,
    NewsDigest,
    EvolutionReport,
    CustomQuery(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackgroundTask {
    pub id: u64,
    pub name: String,
    pub cron_expr: String,
    pub goal: String,
    pub expert_persona: String,
    pub enabled: bool,
    pub last_run_ms: u64,
    pub run_count: u64,
    pub status: TaskStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    Idle,
    Running,
    Completed,
    Failed,
}

/// Proactive agent manager — handles heartbeats, briefings, background tasks.
pub struct ProactiveManager {
    briefing: Option<DailyBriefing>,
    tasks: Vec<BackgroundTask>,
    heartbeat_interval_ms: u64,
    last_heartbeat_ms: u64,
    next_id: u64,
}

impl ProactiveManager {
    pub fn new() -> Self {
        Self {
            briefing: None,
            tasks: Vec::with_capacity(32),
            heartbeat_interval_ms: 60_000, // 1 minute
            last_heartbeat_ms: 0,
            next_id: 1,
        }
    }

    /// Configure daily briefing.
    pub fn set_briefing(&mut self, hour: u8, minute: u8, sections: Vec<BriefingSection>) {
        self.briefing = Some(DailyBriefing {
            time_hour: hour,
            time_minute: minute,
            sections,
            enabled: true,
        });
        info!(hour, minute, "daily briefing configured");
    }

    /// Add a background task.
    pub fn add_task(&mut self, name: &str, cron_expr: &str, goal: &str, expert: &str) -> u64 {
        let id = self.next_id;
        self.next_id += 1;

        self.tasks.push(BackgroundTask {
            id,
            name: name.to_string(),
            cron_expr: cron_expr.to_string(),
            goal: goal.to_string(),
            expert_persona: expert.to_string(),
            enabled: true,
            last_run_ms: 0,
            run_count: 0,
            status: TaskStatus::Idle,
        });

        info!(id, name, "background task added");
        id
    }

    /// Check heartbeat — returns true if heartbeat should fire.
    pub fn check_heartbeat(&mut self) -> bool {
        let now = current_time_ms();
        if now - self.last_heartbeat_ms >= self.heartbeat_interval_ms {
            self.last_heartbeat_ms = now;
            true
        } else {
            false
        }
    }

    /// Get tasks due for execution (simplified: all enabled idle tasks).
    pub fn due_tasks(&self) -> Vec<&BackgroundTask> {
        self.tasks
            .iter()
            .filter(|t| t.enabled && t.status == TaskStatus::Idle)
            .collect()
    }

    /// Mark task as running.
    pub fn start_task(&mut self, id: u64) {
        if let Some(t) = self.tasks.iter_mut().find(|t| t.id == id) {
            t.status = TaskStatus::Running;
        }
    }

    /// Mark task as completed.
    pub fn complete_task(&mut self, id: u64, succeeded: bool) {
        if let Some(t) = self.tasks.iter_mut().find(|t| t.id == id) {
            t.status = if succeeded {
                TaskStatus::Completed
            } else {
                TaskStatus::Failed
            };
            t.run_count += 1;
            t.last_run_ms = current_time_ms();
        }
    }

    pub fn task_count(&self) -> usize {
        self.tasks.len()
    }
    pub fn has_briefing(&self) -> bool {
        self.briefing.is_some()
    }

    pub fn metrics(&self) -> ProactiveMetrics {
        ProactiveMetrics {
            background_tasks: self.tasks.len() as u32,
            running_tasks: self
                .tasks
                .iter()
                .filter(|t| t.status == TaskStatus::Running)
                .count() as u32,
            total_runs: self.tasks.iter().map(|t| t.run_count).sum(),
            briefing_enabled: self.briefing.as_ref().map_or(false, |b| b.enabled),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProactiveMetrics {
    pub background_tasks: u32,
    pub running_tasks: u32,
    pub total_runs: u64,
    pub briefing_enabled: bool,
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
    fn test_briefing() {
        let mut pm = ProactiveManager::new();
        pm.set_briefing(
            8,
            0,
            vec![BriefingSection::TaskSummary, BriefingSection::Weather],
        );
        assert!(pm.has_briefing());
    }

    #[test]
    fn test_background_task() {
        let mut pm = ProactiveManager::new();
        let id = pm.add_task(
            "market_research",
            "0 9 * * *",
            "Research market trends",
            "researcher",
        );
        assert_eq!(pm.task_count(), 1);
        assert_eq!(pm.due_tasks().len(), 1);

        pm.start_task(id);
        assert_eq!(pm.due_tasks().len(), 0); // Running, not idle

        pm.complete_task(id, true);
    }

    #[test]
    fn test_heartbeat() {
        let mut pm = ProactiveManager::new();
        assert!(pm.check_heartbeat()); // First call always fires
        assert!(!pm.check_heartbeat()); // Too soon
    }
}

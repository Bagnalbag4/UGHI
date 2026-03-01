// ughi-computer/src/workflow.rs
// Follows strict_rules.md | Long-running workflows (days/weeks/months)
// Memory cost: ~2 KB per workflow (all state serializable for checkpointing)
// Features: checkpoint/resume, crash recovery, live progress, pause/kill

use serde::{Deserialize, Serialize};
use tracing::info;

/// Workflow execution status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkflowStatus {
    Pending,
    Running,
    Paused,
    Checkpointed,
    Completed,
    Failed,
    Killed,
}

impl std::fmt::Display for WorkflowStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// A single step in a workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStep {
    pub id: u32,
    pub name: String,
    pub description: String,
    pub phase: ProjectPhase,
    pub status: WorkflowStatus,
    pub output: Option<String>,
    pub model_used: Option<String>,
    pub duration_ms: u64,
    pub retry_count: u8,
}

/// Project lifecycle phases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectPhase {
    Research,
    Planning,
    Design,
    Code,
    Test,
    Deploy,
    Monitor,
    Iterate,
}

impl std::fmt::Display for ProjectPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// A persistent, resumable workflow.
/// Can run for weeks/months with checkpointing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    pub id: String,
    pub goal: String,
    pub status: WorkflowStatus,
    pub steps: Vec<WorkflowStep>,
    pub current_step: usize,
    pub created_at_ms: u64,
    pub last_checkpoint_ms: u64,
    pub total_duration_ms: u64,
    pub total_cost_usd: f64,
    pub checkpoint_path: String,
}

impl Workflow {
    /// Create a new workflow from a high-level goal.
    pub fn new(id: &str, goal: &str) -> Self {
        Self {
            id: id.to_string(),
            goal: goal.to_string(),
            status: WorkflowStatus::Pending,
            steps: Vec::new(),
            current_step: 0,
            created_at_ms: current_time_ms(),
            last_checkpoint_ms: 0,
            total_duration_ms: 0,
            total_cost_usd: 0.0,
            checkpoint_path: format!("data/workflows/{}.checkpoint.json", id),
        }
    }

    /// Add a step to the workflow.
    pub fn add_step(&mut self, name: &str, desc: &str, phase: ProjectPhase) {
        let id = self.steps.len() as u32 + 1;
        self.steps.push(WorkflowStep {
            id,
            name: name.to_string(),
            description: desc.to_string(),
            phase,
            status: WorkflowStatus::Pending,
            output: None,
            model_used: None,
            duration_ms: 0,
            retry_count: 0,
        });
    }

    /// Progress to next step.
    pub fn progress(&mut self) -> Option<&WorkflowStep> {
        if self.current_step < self.steps.len() {
            self.steps[self.current_step].status = WorkflowStatus::Running;
            self.status = WorkflowStatus::Running;
            Some(&self.steps[self.current_step])
        } else {
            None
        }
    }

    /// Complete current step with output.
    pub fn complete_step(&mut self, output: &str, model: &str, duration_ms: u64, cost_usd: f64) {
        if self.current_step < self.steps.len() {
            let step = &mut self.steps[self.current_step];
            step.status = WorkflowStatus::Completed;
            step.output = Some(output.to_string());
            step.model_used = Some(model.to_string());
            step.duration_ms = duration_ms;
            self.total_duration_ms += duration_ms;
            self.total_cost_usd += cost_usd;
            self.current_step += 1;

            if self.current_step >= self.steps.len() {
                self.status = WorkflowStatus::Completed;
            }
        }
    }

    /// Fail current step (with retry).
    pub fn fail_step(&mut self, error: &str) {
        if self.current_step < self.steps.len() {
            let step = &mut self.steps[self.current_step];
            step.retry_count += 1;
            if step.retry_count >= 3 {
                step.status = WorkflowStatus::Failed;
                step.output = Some(format!("FAILED after 3 retries: {}", error));
                self.status = WorkflowStatus::Failed;
            } else {
                step.status = WorkflowStatus::Pending; // Will retry
            }
        }
    }

    /// Checkpoint workflow to disk (serializable).
    pub fn checkpoint(&mut self) -> String {
        self.last_checkpoint_ms = current_time_ms();
        self.status = if self.current_step < self.steps.len() {
            WorkflowStatus::Checkpointed
        } else {
            WorkflowStatus::Completed
        };
        info!(workflow_id = %self.id, step = self.current_step, "workflow checkpointed");
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// Resume from checkpoint.
    pub fn resume_from_checkpoint(json: &str) -> Option<Self> {
        let mut wf: Self = serde_json::from_str(json).ok()?;
        wf.status = WorkflowStatus::Running;
        info!(workflow_id = %wf.id, step = wf.current_step, "workflow resumed");
        Some(wf)
    }

    /// Pause the workflow.
    pub fn pause(&mut self) {
        self.status = WorkflowStatus::Paused;
        info!(workflow_id = %self.id, "workflow paused");
    }

    /// Kill the workflow.
    pub fn kill(&mut self) {
        self.status = WorkflowStatus::Killed;
        info!(workflow_id = %self.id, "workflow killed");
    }

    /// Completion percentage.
    pub fn progress_pct(&self) -> f32 {
        if self.steps.is_empty() {
            return 0.0;
        }
        (self.current_step as f32 / self.steps.len() as f32) * 100.0
    }

    /// Get completed step count.
    pub fn completed_steps(&self) -> usize {
        self.steps
            .iter()
            .filter(|s| s.status == WorkflowStatus::Completed)
            .count()
    }
}

/// Workflow engine managing multiple concurrent workflows.
pub struct WorkflowEngine {
    pub workflows: Vec<Workflow>,
    pub total_created: u64,
    pub total_completed: u64,
}

impl WorkflowEngine {
    pub fn new() -> Self {
        Self {
            workflows: Vec::with_capacity(16),
            total_created: 0,
            total_completed: 0,
        }
    }

    /// Create a new workflow and return its ID.
    pub fn create(&mut self, goal: &str) -> String {
        self.total_created += 1;
        let id = format!("wf-{:06}", self.total_created);
        let wf = Workflow::new(&id, goal);
        self.workflows.push(wf);
        info!(id = %id, goal, "workflow created");
        id
    }

    /// Get a workflow by ID.
    pub fn get(&self, id: &str) -> Option<&Workflow> {
        self.workflows.iter().find(|w| w.id == id)
    }

    /// Get a mutable workflow by ID.
    pub fn get_mut(&mut self, id: &str) -> Option<&mut Workflow> {
        self.workflows.iter_mut().find(|w| w.id == id)
    }

    /// List active workflows.
    pub fn active(&self) -> Vec<&Workflow> {
        self.workflows
            .iter()
            .filter(|w| {
                matches!(
                    w.status,
                    WorkflowStatus::Running | WorkflowStatus::Paused | WorkflowStatus::Checkpointed
                )
            })
            .collect()
    }

    pub fn metrics(&self) -> WorkflowMetrics {
        WorkflowMetrics {
            total_created: self.total_created,
            total_completed: self.total_completed,
            active: self.active().len() as u32,
            total_cost_usd: self.workflows.iter().map(|w| w.total_cost_usd).sum(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkflowMetrics {
    pub total_created: u64,
    pub total_completed: u64,
    pub active: u32,
    pub total_cost_usd: f64,
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
    fn test_workflow_lifecycle() {
        let mut wf = Workflow::new("wf-001", "Build a todo app");
        wf.add_step("Research", "Find best stack", ProjectPhase::Research);
        wf.add_step("Code", "Implement app", ProjectPhase::Code);
        wf.add_step("Deploy", "Deploy to Vercel", ProjectPhase::Deploy);

        assert_eq!(wf.progress_pct(), 0.0);
        wf.progress();
        assert_eq!(wf.status, WorkflowStatus::Running);

        wf.complete_step("Next.js + Supabase", "Claude-3.5", 5000, 0.03);
        assert_eq!(wf.current_step, 1);
        assert!(wf.progress_pct() > 30.0);
    }

    #[test]
    fn test_checkpoint_resume() {
        let mut wf = Workflow::new("wf-002", "Marketing campaign");
        wf.add_step("Research", "Analyze competitors", ProjectPhase::Research);
        wf.add_step("Create", "Design creatives", ProjectPhase::Design);
        wf.progress();
        wf.complete_step("Done", "Gemini", 3000, 0.01);

        let json = wf.checkpoint();
        assert!(!json.is_empty());

        let resumed = Workflow::resume_from_checkpoint(&json).unwrap();
        assert_eq!(resumed.current_step, 1);
        assert_eq!(resumed.goal, "Marketing campaign");
    }

    #[test]
    fn test_step_retry_and_fail() {
        let mut wf = Workflow::new("wf-003", "Test");
        wf.add_step("Step1", "Flaky step", ProjectPhase::Code);
        wf.progress();

        wf.fail_step("timeout");
        assert_eq!(wf.steps[0].retry_count, 1);
        assert_eq!(wf.steps[0].status, WorkflowStatus::Pending);

        wf.fail_step("timeout");
        wf.fail_step("timeout");
        assert_eq!(wf.steps[0].status, WorkflowStatus::Failed);
        assert_eq!(wf.status, WorkflowStatus::Failed);
    }

    #[test]
    fn test_pause_kill() {
        let mut wf = Workflow::new("wf-004", "Test");
        wf.add_step("S1", "Step", ProjectPhase::Code);
        wf.progress();
        wf.pause();
        assert_eq!(wf.status, WorkflowStatus::Paused);
        wf.kill();
        assert_eq!(wf.status, WorkflowStatus::Killed);
    }

    #[test]
    fn test_workflow_engine() {
        let mut engine = WorkflowEngine::new();
        let id1 = engine.create("Build app");
        let id2 = engine.create("Run campaign");

        assert_eq!(engine.total_created, 2);
        assert!(engine.get(&id1).is_some());
        assert!(engine.get(&id2).is_some());
    }
}

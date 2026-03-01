// UGHI-workflows/src/composer.rs
// Lobster-style composable skill pipelines
// Memory: ~2 KB per pipeline

use serde::{Deserialize, Serialize};
use tracing::info;

/// A single step in a workflow pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineStep {
    pub skill_name: String,
    pub parameters: serde_json::Value,
    pub on_failure: FailureAction,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FailureAction {
    Stop,
    Skip,
    Retry(u8),
}

/// A composable workflow pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pipeline {
    pub id: u64,
    pub name: String,
    pub description: String,
    pub steps: Vec<PipelineStep>,
    pub expert_persona: String,
    pub run_count: u64,
    pub success_count: u64,
}

/// Step execution result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    pub step_index: usize,
    pub skill_name: String,
    pub succeeded: bool,
    pub output: serde_json::Value,
    pub execution_ms: u64,
}

/// Pipeline execution result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineResult {
    pub pipeline_id: u64,
    pub succeeded: bool,
    pub steps: Vec<StepResult>,
    pub total_ms: u64,
}

/// Workflow composer — create and manage skill pipelines.
pub struct WorkflowComposer {
    pipelines: Vec<Pipeline>,
    next_id: u64,
}

impl WorkflowComposer {
    pub fn new() -> Self {
        Self {
            pipelines: Vec::with_capacity(32),
            next_id: 1,
        }
    }

    /// Create a new pipeline.
    pub fn create(&mut self, name: &str, desc: &str, expert: &str) -> &mut Pipeline {
        let id = self.next_id;
        self.next_id += 1;

        self.pipelines.push(Pipeline {
            id,
            name: name.to_string(),
            description: desc.to_string(),
            steps: Vec::new(),
            expert_persona: expert.to_string(),
            run_count: 0,
            success_count: 0,
        });

        info!(id, name, "pipeline created");
        self.pipelines.last_mut().unwrap()
    }

    /// Add a step to a pipeline.
    pub fn add_step(
        &mut self,
        pipeline_id: u64,
        skill: &str,
        params: serde_json::Value,
        on_fail: FailureAction,
    ) -> bool {
        if let Some(p) = self.pipelines.iter_mut().find(|p| p.id == pipeline_id) {
            p.steps.push(PipelineStep {
                skill_name: skill.to_string(),
                parameters: params,
                on_failure: on_fail,
                timeout_ms: 30_000,
            });
            true
        } else {
            false
        }
    }

    /// Simulate executing a pipeline (returns results per step).
    pub fn execute(&mut self, pipeline_id: u64) -> Option<PipelineResult> {
        let pipeline = self.pipelines.iter_mut().find(|p| p.id == pipeline_id)?;
        pipeline.run_count += 1;

        let mut step_results = Vec::new();
        let mut all_succeeded = true;

        for (i, step) in pipeline.steps.iter().enumerate() {
            // Simulated execution — real impl dispatches to SkillExecutor
            let result = StepResult {
                step_index: i,
                skill_name: step.skill_name.clone(),
                succeeded: true,
                output: serde_json::json!({"status": "simulated", "skill": &step.skill_name}),
                execution_ms: 50,
            };

            if !result.succeeded {
                all_succeeded = false;
                match step.on_failure {
                    FailureAction::Stop => {
                        step_results.push(result);
                        break;
                    }
                    FailureAction::Skip => {}
                    FailureAction::Retry(_) => {}
                }
            }
            step_results.push(result);
        }

        if all_succeeded {
            pipeline.success_count += 1;
        }

        let total_ms: u64 = step_results.iter().map(|r| r.execution_ms).sum();

        Some(PipelineResult {
            pipeline_id,
            succeeded: all_succeeded,
            steps: step_results,
            total_ms,
        })
    }

    /// List all pipelines.
    pub fn list(&self) -> &[Pipeline] {
        &self.pipelines
    }
    pub fn count(&self) -> usize {
        self.pipelines.len()
    }

    /// Get pipeline by name.
    pub fn find(&self, name: &str) -> Option<&Pipeline> {
        self.pipelines.iter().find(|p| p.name == name)
    }

    /// Built-in workflow templates.
    pub fn load_templates(&mut self) {
        let p = self.create(
            "daily-briefing",
            "Morning briefing workflow",
            "project-manager",
        );
        let pid = p.id;
        self.add_step(
            pid,
            "web_search",
            serde_json::json!({"query": "today's news"}),
            FailureAction::Skip,
        );
        self.add_step(
            pid,
            "memory_read_write",
            serde_json::json!({"action": "read_tasks"}),
            FailureAction::Skip,
        );
        self.add_step(
            pid,
            "self_critique",
            serde_json::json!({"text": "summarize briefing"}),
            FailureAction::Stop,
        );

        let p2 = self.create("market-research", "Automated market analysis", "researcher");
        let pid2 = p2.id;
        self.add_step(
            pid2,
            "web_search",
            serde_json::json!({"query": "market trends"}),
            FailureAction::Retry(2),
        );
        self.add_step(
            pid2,
            "self_critique",
            serde_json::json!({"text": "analyze findings"}),
            FailureAction::Stop,
        );
        self.add_step(
            pid2,
            "memory_read_write",
            serde_json::json!({"action": "store_results"}),
            FailureAction::Skip,
        );

        info!("loaded {} workflow templates", self.count());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_pipeline() {
        let mut composer = WorkflowComposer::new();
        composer.create("test", "test pipeline", "general-genius");
        assert_eq!(composer.count(), 1);
    }

    #[test]
    fn test_add_steps() {
        let mut composer = WorkflowComposer::new();
        let p = composer.create("test", "desc", "e");
        let id = p.id;
        composer.add_step(id, "web_search", serde_json::json!({}), FailureAction::Stop);
        composer.add_step(
            id,
            "self_critique",
            serde_json::json!({}),
            FailureAction::Skip,
        );
        assert_eq!(composer.list()[0].steps.len(), 2);
    }

    #[test]
    fn test_execute() {
        let mut composer = WorkflowComposer::new();
        let p = composer.create("test", "desc", "e");
        let id = p.id;
        composer.add_step(id, "web_search", serde_json::json!({}), FailureAction::Stop);
        let result = composer.execute(id).unwrap();
        assert!(result.succeeded);
        assert_eq!(result.steps.len(), 1);
    }

    #[test]
    fn test_templates() {
        let mut composer = WorkflowComposer::new();
        composer.load_templates();
        assert_eq!(composer.count(), 2);
        assert!(composer.find("daily-briefing").is_some());
        assert!(composer.find("market-research").is_some());
    }
}

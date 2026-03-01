// UGHI-workflows/src/lib.rs
pub mod composer;

pub use composer::{
    FailureAction, Pipeline, PipelineResult, PipelineStep, StepResult, WorkflowComposer,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_full_workflow() {
        let mut composer = WorkflowComposer::new();
        composer.load_templates();
        let result = composer.execute(1).unwrap();
        assert!(result.succeeded);
    }
}

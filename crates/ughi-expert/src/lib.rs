// UGHI-expert/src/lib.rs
// Follows strict_rules.md + expert_roles.md
// "Every agent MUST spawn with a world-class expert persona" – strict_rules.md #2
// 50 expert templates | Auto-selection | Zero heap for static registry
//
// Module structure:
// - persona:   50 static expert persona templates
// - selector:  Goal-based auto-selection (keyword × specialization)

pub mod persona;
pub mod selector;

pub use persona::{ExpertDomain, ExpertPersona, EXPERT_PERSONAS};
pub use selector::{select_expert, select_team};

/// Get total number of available expert personas.
pub fn expert_count() -> usize {
    EXPERT_PERSONAS.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expert_count() {
        assert_eq!(expert_count(), 50);
    }

    #[test]
    fn test_public_api() {
        let expert = select_expert("build a web application");
        assert!(!expert.system_prompt.is_empty());
        assert!(!expert.skills.is_empty());
    }
}

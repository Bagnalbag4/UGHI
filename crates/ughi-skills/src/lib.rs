// UGHI-skills/src/lib.rs
// Follows strict_rules.md | Per skill call: ≤ 45 MB | Latency: < 420 ms cold, < 80 ms hot
// All skills must return structured JSON + natural language summary.
// Every skill = Rust core trait. Go wrapper + Python SDK are separate layers.
// No panic! in core – all errors via SkillError.
//
// Module structure:
// - executor:  10 built-in skill implementations + dispatch
// - lib:       BuiltinSkill enum, SkillInput/Output, SkillRegistry

pub mod executor;

use serde::{Deserialize, Serialize};
use thiserror::Error;

// Re-export executor
pub use executor::execute_skill;

/// Skill execution errors – all recoverable.
#[derive(Error, Debug)]
pub enum SkillError {
    #[error("skill not found: {name}")]
    NotFound { name: String },

    #[error("skill execution failed: {reason}")]
    ExecutionFailed { reason: String },

    #[error("skill memory budget exceeded: {used_mb} MB > 45 MB")]
    MemoryBudgetExceeded { used_mb: u64 },

    #[error("skill timeout: {name} exceeded {timeout_ms} ms")]
    Timeout { name: String, timeout_ms: u64 },
}

/// Input to a skill execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInput {
    pub skill_name: String,
    pub parameters: serde_json::Value,
    pub capability_token: String,
}

/// Output from a skill execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillOutput {
    pub result: serde_json::Value,
    pub summary: String,
    pub memory_used_bytes: u64,
    pub execution_time_ms: u64,
}

/// Built-in skill identifiers from skills.md.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BuiltinSkill {
    BrowserControl,
    CodeExecutor,
    WebSearch,
    FileSystem,
    MemoryReadWrite,
    Messaging,
    Scheduler,
    SelfCritique,
    CollaborationVote,
    TerminalCommand,
}

impl BuiltinSkill {
    /// Get all built-in skills.
    pub fn all() -> &'static [BuiltinSkill] {
        &[
            BuiltinSkill::BrowserControl,
            BuiltinSkill::CodeExecutor,
            BuiltinSkill::WebSearch,
            BuiltinSkill::FileSystem,
            BuiltinSkill::MemoryReadWrite,
            BuiltinSkill::Messaging,
            BuiltinSkill::Scheduler,
            BuiltinSkill::SelfCritique,
            BuiltinSkill::CollaborationVote,
            BuiltinSkill::TerminalCommand,
        ]
    }

    /// Get the name of this skill.
    pub fn name(&self) -> &'static str {
        match self {
            BuiltinSkill::BrowserControl => "browser_control",
            BuiltinSkill::CodeExecutor => "code_executor",
            BuiltinSkill::WebSearch => "web_search",
            BuiltinSkill::FileSystem => "file_system",
            BuiltinSkill::MemoryReadWrite => "memory_read_write",
            BuiltinSkill::Messaging => "messaging",
            BuiltinSkill::Scheduler => "scheduler",
            BuiltinSkill::SelfCritique => "self_critique",
            BuiltinSkill::CollaborationVote => "collaboration_vote",
            BuiltinSkill::TerminalCommand => "terminal_command",
        }
    }

    /// Look up a skill by name.
    pub fn from_name(name: &str) -> Option<BuiltinSkill> {
        match name {
            "browser_control" => Some(BuiltinSkill::BrowserControl),
            "code_executor" => Some(BuiltinSkill::CodeExecutor),
            "web_search" => Some(BuiltinSkill::WebSearch),
            "file_system" => Some(BuiltinSkill::FileSystem),
            "memory_read_write" => Some(BuiltinSkill::MemoryReadWrite),
            "messaging" => Some(BuiltinSkill::Messaging),
            "scheduler" => Some(BuiltinSkill::Scheduler),
            "self_critique" => Some(BuiltinSkill::SelfCritique),
            "collaboration_vote" => Some(BuiltinSkill::CollaborationVote),
            "terminal_command" => Some(BuiltinSkill::TerminalCommand),
            _ => None,
        }
    }

    /// Memory budget (bytes).
    pub fn memory_budget_bytes(&self) -> u64 {
        45 * 1024 * 1024 // 45 MB for all skills per skills.md
    }
}

/// Skill registry – manages available skills.
pub struct SkillRegistry {
    builtin_skills: Vec<BuiltinSkill>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self {
            builtin_skills: BuiltinSkill::all().to_vec(),
        }
    }

    pub fn count(&self) -> usize {
        self.builtin_skills.len()
    }

    pub fn find(&self, name: &str) -> Option<&BuiltinSkill> {
        self.builtin_skills.iter().find(|s| s.name() == name)
    }

    pub fn validate_budget(skill: &BuiltinSkill, used_bytes: u64) -> Result<(), SkillError> {
        let budget = skill.memory_budget_bytes();
        if used_bytes > budget {
            return Err(SkillError::MemoryBudgetExceeded {
                used_mb: used_bytes / (1024 * 1024),
            });
        }
        Ok(())
    }
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_skills_count() {
        assert_eq!(SkillRegistry::new().count(), 10);
    }

    #[test]
    fn test_find_skill() {
        let reg = SkillRegistry::new();
        assert!(reg.find("web_search").is_some());
        assert!(reg.find("nonexistent").is_none());
    }

    #[test]
    fn test_from_name_roundtrip() {
        for skill in BuiltinSkill::all() {
            let name = skill.name();
            assert_eq!(BuiltinSkill::from_name(name), Some(*skill));
        }
    }

    #[test]
    fn test_budget_validation() {
        assert!(SkillRegistry::validate_budget(&BuiltinSkill::WebSearch, 10 * 1024 * 1024).is_ok());
        assert!(
            SkillRegistry::validate_budget(&BuiltinSkill::WebSearch, 50 * 1024 * 1024).is_err()
        );
    }
}

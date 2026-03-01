// UGHI-wasm/src/skill_registry.rs
// Follows strict_rules.md + skills.md | Pre-approved skills only
// Memory cost: ~2 KB (static registry, 10 skills)
// "New skills added only via PR + benchmark proof." – skills.md

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::capability::CapabilityScope;

/// A registered skill definition.
/// Memory cost: ~256 bytes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDefinition {
    pub name: String,
    pub description: String,
    /// Default capability scope for this skill
    pub default_scope: CapabilityScope,
    /// Maximum memory per call (bytes) – skills.md: ≤ 45 MB
    pub max_memory_bytes: u64,
    /// Cold latency SLA (ms) – skills.md: < 420 ms
    pub cold_latency_ms: u64,
    /// Hot latency SLA (ms) – skills.md: < 80 ms
    pub hot_latency_ms: u64,
    /// Whether this skill requires network access
    pub needs_network: bool,
    /// Whether this skill requires filesystem access
    pub needs_filesystem: bool,
    /// Risk level (1=low, 5=high) – higher = stricter oversight
    pub risk_level: u8,
}

/// Skill registry: all 10 pre-approved skills from skills.md.
/// Memory cost: ~2.5 KB (HashMap with 10 entries)
pub struct SkillRegistry {
    skills: HashMap<String, SkillDefinition>,
}

impl SkillRegistry {
    /// Create registry with all 10 built-in skills from skills.md.
    pub fn new() -> Self {
        let mut skills = HashMap::with_capacity(10);

        // 1. BrowserControl – Ferrum/Playwright, <180 MB for 8 tabs
        skills.insert(
            "browser_control".into(),
            SkillDefinition {
                name: "browser_control".into(),
                description: "Browser automation via headless Chromium".into(),
                default_scope: CapabilityScope {
                    allowed_hosts: vec!["*".into()],
                    max_memory_bytes: 45 * 1024 * 1024,
                    max_time_ms: 420,
                    ..Default::default()
                },
                max_memory_bytes: 45 * 1024 * 1024,
                cold_latency_ms: 420,
                hot_latency_ms: 80,
                needs_network: true,
                needs_filesystem: false,
                risk_level: 4,
            },
        );

        // 2. CodeExecutor – Safe Rust sandbox (wasmtime), Python subset
        skills.insert(
            "code_executor".into(),
            SkillDefinition {
                name: "code_executor".into(),
                description: "Execute code in sandboxed WASM environment".into(),
                default_scope: CapabilityScope {
                    max_memory_bytes: 45 * 1024 * 1024,
                    max_time_ms: 420,
                    ..Default::default()
                },
                max_memory_bytes: 45 * 1024 * 1024,
                cold_latency_ms: 420,
                hot_latency_ms: 80,
                needs_network: false,
                needs_filesystem: false,
                risk_level: 5,
            },
        );

        // 3. WebSearch – Local cache + DuckDuckGo fallback
        skills.insert(
            "web_search".into(),
            SkillDefinition {
                name: "web_search".into(),
                description: "Search the web via DuckDuckGo API".into(),
                default_scope: CapabilityScope {
                    allowed_hosts: vec!["api.duckduckgo.com".into()],
                    max_memory_bytes: 8 * 1024 * 1024,
                    max_time_ms: 420,
                    read_only: true,
                    ..Default::default()
                },
                max_memory_bytes: 8 * 1024 * 1024,
                cold_latency_ms: 420,
                hot_latency_ms: 80,
                needs_network: true,
                needs_filesystem: false,
                risk_level: 2,
            },
        );

        // 4. FileSystem – Virtual FS with capability tokens
        skills.insert(
            "file_system".into(),
            SkillDefinition {
                name: "file_system".into(),
                description: "Virtual filesystem with capability-gated access".into(),
                default_scope: CapabilityScope {
                    allowed_paths: vec!["data/".into(), "tmp/".into()],
                    max_memory_bytes: 16 * 1024 * 1024,
                    max_time_ms: 200,
                    read_only: false,
                    ..Default::default()
                },
                max_memory_bytes: 16 * 1024 * 1024,
                cold_latency_ms: 200,
                hot_latency_ms: 40,
                needs_network: false,
                needs_filesystem: true,
                risk_level: 3,
            },
        );

        // 5. MemoryReadWrite – Vector + SQLite
        skills.insert(
            "memory_read_write".into(),
            SkillDefinition {
                name: "memory_read_write".into(),
                description: "Read/write agent memory (vector + SQLite)".into(),
                default_scope: CapabilityScope {
                    max_memory_bytes: 45 * 1024 * 1024,
                    max_time_ms: 200,
                    ..Default::default()
                },
                max_memory_bytes: 45 * 1024 * 1024,
                cold_latency_ms: 200,
                hot_latency_ms: 40,
                needs_network: false,
                needs_filesystem: true,
                risk_level: 1,
            },
        );

        // 6. Email/Slack/Discord – API only
        skills.insert(
            "messaging".into(),
            SkillDefinition {
                name: "messaging".into(),
                description: "Send messages via Email/Slack/Discord APIs".into(),
                default_scope: CapabilityScope {
                    allowed_hosts: vec![
                        "slack.com".into(),
                        "discord.com".into(),
                        "smtp.gmail.com".into(),
                    ],
                    max_memory_bytes: 4 * 1024 * 1024,
                    max_time_ms: 420,
                    ..Default::default()
                },
                max_memory_bytes: 4 * 1024 * 1024,
                cold_latency_ms: 420,
                hot_latency_ms: 80,
                needs_network: true,
                needs_filesystem: false,
                risk_level: 3,
            },
        );

        // 7. Scheduler – Cron + predictive wake
        skills.insert(
            "scheduler".into(),
            SkillDefinition {
                name: "scheduler".into(),
                description: "Schedule tasks via cron expressions".into(),
                default_scope: CapabilityScope {
                    max_memory_bytes: 2 * 1024 * 1024,
                    max_time_ms: 100,
                    ..Default::default()
                },
                max_memory_bytes: 2 * 1024 * 1024,
                cold_latency_ms: 100,
                hot_latency_ms: 20,
                needs_network: false,
                needs_filesystem: false,
                risk_level: 2,
            },
        );

        // 8. SelfCritique – Calls same SLM with reflection prompt
        skills.insert(
            "self_critique".into(),
            SkillDefinition {
                name: "self_critique".into(),
                description: "Self-review via SLM reflection".into(),
                default_scope: CapabilityScope {
                    max_memory_bytes: 45 * 1024 * 1024,
                    max_time_ms: 420,
                    ..Default::default()
                },
                max_memory_bytes: 45 * 1024 * 1024,
                cold_latency_ms: 420,
                hot_latency_ms: 80,
                needs_network: false,
                needs_filesystem: false,
                risk_level: 1,
            },
        );

        // 9. CollaborationVote – Multi-agent consensus
        skills.insert(
            "collaboration_vote".into(),
            SkillDefinition {
                name: "collaboration_vote".into(),
                description: "Multi-agent consensus voting".into(),
                default_scope: CapabilityScope {
                    max_memory_bytes: 4 * 1024 * 1024,
                    max_time_ms: 200,
                    can_spawn: true,
                    ..Default::default()
                },
                max_memory_bytes: 4 * 1024 * 1024,
                cold_latency_ms: 200,
                hot_latency_ms: 40,
                needs_network: false,
                needs_filesystem: false,
                risk_level: 2,
            },
        );

        // 10. TerminalCommand – SSH-safe subset
        skills.insert(
            "terminal_command".into(),
            SkillDefinition {
                name: "terminal_command".into(),
                description: "Execute safe terminal commands (allowlisted)".into(),
                default_scope: CapabilityScope {
                    max_memory_bytes: 16 * 1024 * 1024,
                    max_time_ms: 420,
                    read_only: false,
                    ..Default::default()
                },
                max_memory_bytes: 16 * 1024 * 1024,
                cold_latency_ms: 420,
                hot_latency_ms: 80,
                needs_network: false,
                needs_filesystem: true,
                risk_level: 5,
            },
        );

        Self { skills }
    }

    /// Get a skill definition by name.
    pub fn get(&self, name: &str) -> Option<&SkillDefinition> {
        self.skills.get(name)
    }

    /// Check if a skill is registered (pre-approved).
    pub fn is_registered(&self, name: &str) -> bool {
        self.skills.contains_key(name)
    }

    /// All registered skill names.
    pub fn list(&self) -> Vec<&str> {
        self.skills.keys().map(|s| s.as_str()).collect()
    }

    /// Number of registered skills.
    pub fn count(&self) -> usize {
        self.skills.len()
    }

    /// Get default scope for a skill.
    pub fn default_scope(&self, name: &str) -> Option<CapabilityScope> {
        self.skills.get(name).map(|s| s.default_scope.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_10_builtin_skills() {
        let reg = SkillRegistry::new();
        assert_eq!(
            reg.count(),
            10,
            "must have exactly 10 built-in skills per skills.md"
        );
    }

    #[test]
    fn test_all_skills_registered() {
        let reg = SkillRegistry::new();
        let expected = [
            "browser_control",
            "code_executor",
            "web_search",
            "file_system",
            "memory_read_write",
            "messaging",
            "scheduler",
            "self_critique",
            "collaboration_vote",
            "terminal_command",
        ];
        for skill in &expected {
            assert!(reg.is_registered(skill), "missing skill: {}", skill);
        }
    }

    #[test]
    fn test_memory_budget_compliance() {
        let reg = SkillRegistry::new();
        for skill in reg.skills.values() {
            assert!(
                skill.max_memory_bytes <= 45 * 1024 * 1024,
                "{}: {} bytes > 45 MB",
                skill.name,
                skill.max_memory_bytes
            );
        }
    }

    #[test]
    fn test_latency_sla_compliance() {
        let reg = SkillRegistry::new();
        for skill in reg.skills.values() {
            assert!(
                skill.cold_latency_ms <= 420,
                "{}: cold {}ms > 420ms",
                skill.name,
                skill.cold_latency_ms
            );
            assert!(
                skill.hot_latency_ms <= 80,
                "{}: hot {}ms > 80ms",
                skill.name,
                skill.hot_latency_ms
            );
        }
    }

    #[test]
    fn test_unregistered_skill_denied() {
        let reg = SkillRegistry::new();
        assert!(!reg.is_registered("hack_the_planet"));
    }
}

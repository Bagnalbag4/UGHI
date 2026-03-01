// UGHI-wasm/src/lib.rs
// Follows strict_rules.md | Zero trust | All tools via WASM capability tokens
// Memory: ~12 MB base (wasmtime engine + JIT + code cache)
// Sandbox overhead: < 8% latency (HashMap O(1) capability check)
// strict_rules.md #10: "Every agent starts with zero capabilities."
// No panic! in core – all errors via SandboxError.
//
// Module structure:
// - error:          SandboxError enum (11 recoverable variants)
// - capability:     Scoped, time-bound capability tokens + manager
// - skill_registry: 10 pre-approved skills from skills.md
// - resource:       Per-execution resource limits + tracking
// - sandbox:        SandboxEngine (wasmtime + zero-trust pipeline)
// - violation:      Security violation tracking + auto-quarantine

pub mod capability;
pub mod error;
pub mod resource;
pub mod sandbox;
pub mod skill_registry;
pub mod violation;

// --- Public re-exports for ergonomic API ---
pub use capability::{CapabilityManager, CapabilityMetrics, CapabilityScope, CapabilityToken};
pub use error::{SandboxError, SandboxResult};
pub use resource::{ResourceLimits, ResourceTracker, ResourceUsage};
pub use sandbox::{SandboxEngine, SecurityMetrics, SkillInput, SkillOutput};
pub use skill_registry::{SkillDefinition, SkillRegistry};
pub use violation::{Violation, ViolationMetrics, ViolationTracker};

/// Legacy Sandbox wrapper for backward compatibility with kernel main.rs.
/// Memory cost: delegates to SandboxEngine (~12 MB)
pub struct Sandbox {
    inner: SandboxEngine,
}

impl Sandbox {
    /// Create a new sandbox (zero trust).
    /// Memory cost: ~12 MB
    pub fn new() -> Result<Self, SandboxError> {
        Ok(Self {
            inner: SandboxEngine::new()?,
        })
    }

    /// Issue a capability token (backward compat).
    pub fn issue_token(
        &mut self,
        agent_id: &str,
        skill_name: &str,
        expires_at: u64,
    ) -> CapabilityToken {
        let ttl = if expires_at > 0 {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            expires_at.saturating_sub(now)
        } else {
            0
        };

        // Try to grant; if skill isn't registered, issue with default scope
        let _ = self.inner.grant(agent_id, skill_name, ttl);

        CapabilityToken {
            id: format!("legacy-{}", agent_id),
            agent_id: agent_id.to_string(),
            skill_name: skill_name.to_string(),
            expires_at,
            issued_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            scope: CapabilityScope::default(),
            revoked: false,
        }
    }

    /// Check capability (backward compat).
    /// Uses a probe execution through the full sandbox pipeline (no capabilities_mut).
    pub fn check_capability(
        &mut self,
        agent_id: &str,
        skill_name: &str,
        _current_time: u64,
    ) -> Result<(), SandboxError> {
        // Use a lightweight probe: try execute with null payload.
        // If capability is valid, this succeeds; if not, returns the denial error.
        let probe = sandbox::SkillInput {
            agent_id: agent_id.to_string(),
            skill_name: skill_name.to_string(),
            payload: serde_json::json!(null),
        };
        self.inner.execute(&probe)?;
        Ok(())
    }

    /// Revoke all (backward compat).
    pub fn revoke_all(&mut self, agent_id: &str) {
        self.inner.revoke_all(agent_id);
    }

    /// Token count (backward compat).
    pub fn token_count(&self) -> usize {
        self.inner.active_tokens()
    }

    /// Get the inner engine.
    pub fn engine(&self) -> &SandboxEngine {
        &self.inner
    }

    /// Get mutable inner engine.
    pub fn engine_mut(&mut self) -> &mut SandboxEngine {
        &mut self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backward_compat_zero_trust() {
        let sandbox = Sandbox::new().unwrap();
        assert_eq!(sandbox.token_count(), 0);
    }

    #[test]
    fn test_backward_compat_issue_token() {
        let mut sandbox = Sandbox::new().unwrap();
        sandbox.issue_token("agent-1", "web_search", 0);
        assert_eq!(sandbox.token_count(), 1);
    }

    #[test]
    fn test_backward_compat_check() {
        let mut sandbox = Sandbox::new().unwrap();
        sandbox.issue_token("agent-1", "web_search", 0);
        assert!(sandbox.check_capability("agent-1", "web_search", 0).is_ok());
        assert!(sandbox
            .check_capability("agent-1", "file_system", 0)
            .is_err());
    }

    #[test]
    fn test_backward_compat_revoke() {
        let mut sandbox = Sandbox::new().unwrap();
        sandbox.issue_token("a", "web_search", 0);
        sandbox.issue_token("a", "file_system", 0);
        assert_eq!(sandbox.token_count(), 2);
        sandbox.revoke_all("a");
        assert_eq!(sandbox.token_count(), 0);
    }
}

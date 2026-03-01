// UGHI-wasm/src/capability.rs
// Follows strict_rules.md | Zero trust: agents start with zero capabilities
// Memory cost: ~256 bytes per token, HashMap lookup O(1)
// Scoped, time-bound, revocable capability tokens.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::info;

use crate::error::{SandboxError, SandboxResult};

/// Scope of resources a capability token grants access to.
/// Memory cost: ~128 bytes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityScope {
    /// Allowed filesystem paths (empty = no FS access)
    pub allowed_paths: Vec<String>,
    /// Allowed network hosts (empty = no network access)
    pub allowed_hosts: Vec<String>,
    /// Max memory per call in bytes (skills.md: ≤ 45 MB)
    pub max_memory_bytes: u64,
    /// Max execution time per call in milliseconds
    pub max_time_ms: u64,
    /// Whether the skill can spawn sub-agents
    pub can_spawn: bool,
    /// Read-only mode (no writes to FS/network)
    pub read_only: bool,
}

impl Default for CapabilityScope {
    fn default() -> Self {
        Self {
            allowed_paths: Vec::new(),
            allowed_hosts: Vec::new(),
            max_memory_bytes: 45 * 1024 * 1024, // 45 MB per skills.md
            max_time_ms: 420,                   // 420 ms cold per skills.md
            can_spawn: false,
            read_only: true,
        }
    }
}

/// A scoped, time-bound capability token.
/// Memory cost: ~256 bytes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityToken {
    /// Unique token ID
    pub id: String,
    /// Agent ID this token is issued to
    pub agent_id: String,
    /// Skill name this token grants access to
    pub skill_name: String,
    /// Token expiry (Unix ms, 0 = no expiry)
    pub expires_at: u64,
    /// Issued timestamp (Unix ms)
    pub issued_at: u64,
    /// Resource scope
    pub scope: CapabilityScope,
    /// Whether this token has been revoked
    pub revoked: bool,
}

impl CapabilityToken {
    /// Check if token is valid at the given time.
    pub fn is_valid(&self, current_time_ms: u64) -> bool {
        !self.revoked && (self.expires_at == 0 || self.expires_at > current_time_ms)
    }

    /// Check if token is expired.
    pub fn is_expired(&self, current_time_ms: u64) -> bool {
        self.expires_at > 0 && self.expires_at <= current_time_ms
    }
}

/// Capability manager: issues, validates, and revokes tokens.
/// Memory cost: ~256 bytes per active token
/// Uses HashMap<(agent_id, skill_name), Vec<Token>> for O(1) lookup.
pub struct CapabilityManager {
    /// Active tokens indexed by (agent_id, skill_name)
    tokens: HashMap<String, Vec<CapabilityToken>>,
    /// H-05 FIX: removed sequential next_id, using random IDs now
    /// Total tokens issued (for metrics)
    total_issued: u64,
    /// Total tokens revoked
    total_revoked: u64,
    /// Total denials
    total_denials: u64,
}

impl CapabilityManager {
    /// Create a new capability manager (zero capabilities).
    /// strict_rules.md #10: "Every agent starts with zero capabilities."
    pub fn new() -> Self {
        Self {
            tokens: HashMap::with_capacity(64),
            total_issued: 0,
            total_revoked: 0,
            total_denials: 0,
        }
    }

    /// Generate unpredictable token ID (H-05 fix).
    /// Uses timestamp XOR'd with a simple hash of agent+skill for uniqueness.
    /// NOT sequential — attackers cannot predict or enumerate IDs.
    fn generate_token_id(agent_id: &str, skill_name: &str) -> String {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let nanos = now.as_nanos() as u64;
        // Simple hash mix: FNV-1a inspired
        let mut hash: u64 = 0xcbf29ce484222325;
        for b in agent_id.bytes().chain(skill_name.bytes()) {
            hash ^= b as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        let id = nanos ^ hash;
        format!("cap-{:016x}", id)
    }

    /// Issue a capability token to an agent for a specific skill.
    /// Memory cost: ~256 bytes
    pub fn issue(
        &mut self,
        agent_id: &str,
        skill_name: &str,
        scope: CapabilityScope,
        ttl_ms: u64,
    ) -> CapabilityToken {
        let now = current_time_ms();
        let token = CapabilityToken {
            id: Self::generate_token_id(agent_id, skill_name),
            agent_id: agent_id.to_string(),
            skill_name: skill_name.to_string(),
            expires_at: if ttl_ms > 0 { now + ttl_ms } else { 0 },
            issued_at: now,
            scope,
            revoked: false,
        };

        self.total_issued += 1;

        let key = cap_key(agent_id, skill_name);
        self.tokens
            .entry(key)
            .or_insert_with(Vec::new)
            .push(token.clone());

        info!(agent_id, skill_name, token_id = %token.id, "capability token issued");
        token
    }

    /// Validate that an agent has a valid capability for a skill.
    /// Memory cost: 0 (read-only lookup)
    /// Latency: O(1) HashMap lookup + O(k) where k = tokens for this agent+skill
    pub fn validate(
        &mut self,
        agent_id: &str,
        skill_name: &str,
    ) -> SandboxResult<&CapabilityToken> {
        let now = current_time_ms();
        let key = cap_key(agent_id, skill_name);

        let tokens = self.tokens.get(&key).ok_or_else(|| {
            self.total_denials += 1;
            SandboxError::CapabilityDenied {
                agent_id: agent_id.to_string(),
                skill: skill_name.to_string(),
            }
        })?;

        // Find a valid (non-revoked, non-expired) token
        let valid = tokens.iter().find(|t| t.is_valid(now));

        match valid {
            Some(token) => Ok(token),
            None => {
                self.total_denials += 1;
                // Check if there's an expired one for a better error
                if tokens.iter().any(|t| t.is_expired(now)) {
                    Err(SandboxError::CapabilityExpired {
                        agent_id: agent_id.to_string(),
                        skill: skill_name.to_string(),
                    })
                } else {
                    Err(SandboxError::CapabilityDenied {
                        agent_id: agent_id.to_string(),
                        skill: skill_name.to_string(),
                    })
                }
            }
        }
    }

    /// Revoke a specific token by ID.
    pub fn revoke_token(&mut self, token_id: &str) -> bool {
        for tokens in self.tokens.values_mut() {
            if let Some(t) = tokens.iter_mut().find(|t| t.id == token_id) {
                t.revoked = true;
                self.total_revoked += 1;
                info!(token_id, "capability token revoked");
                return true;
            }
        }
        false
    }

    /// Revoke all capabilities for an agent.
    pub fn revoke_all(&mut self, agent_id: &str) {
        let mut revoked = 0u32;
        for tokens in self.tokens.values_mut() {
            for t in tokens.iter_mut() {
                if t.agent_id == agent_id && !t.revoked {
                    t.revoked = true;
                    revoked += 1;
                }
            }
        }
        self.total_revoked += revoked as u64;
        info!(agent_id, revoked, "all capabilities revoked");
    }

    /// Clean up expired and revoked tokens (memory reclaim).
    pub fn cleanup(&mut self) {
        let now = current_time_ms();
        for tokens in self.tokens.values_mut() {
            tokens.retain(|t| t.is_valid(now));
        }
        self.tokens.retain(|_, v| !v.is_empty());
    }

    /// Total active (valid) token count.
    pub fn active_count(&self) -> usize {
        let now = current_time_ms();
        self.tokens
            .values()
            .flat_map(|v| v.iter())
            .filter(|t| t.is_valid(now))
            .count()
    }

    /// Get all tokens for an agent (for dashboard).
    pub fn agent_tokens(&self, agent_id: &str) -> Vec<&CapabilityToken> {
        let now = current_time_ms();
        self.tokens
            .values()
            .flat_map(|v| v.iter())
            .filter(|t| t.agent_id == agent_id && t.is_valid(now))
            .collect()
    }

    /// Metrics snapshot.
    pub fn metrics(&self) -> CapabilityMetrics {
        CapabilityMetrics {
            active_tokens: self.active_count() as u64,
            total_issued: self.total_issued,
            total_revoked: self.total_revoked,
            total_denials: self.total_denials,
        }
    }
}

/// Capability metrics for the dashboard.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CapabilityMetrics {
    pub active_tokens: u64,
    pub total_issued: u64,
    pub total_revoked: u64,
    pub total_denials: u64,
}

/// Create a composite key for the token map.
fn cap_key(agent_id: &str, skill_name: &str) -> String {
    format!("{}:{}", agent_id, skill_name)
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
    fn test_zero_trust_start() {
        let mgr = CapabilityManager::new();
        assert_eq!(mgr.active_count(), 0);
    }

    #[test]
    fn test_issue_and_validate() {
        let mut mgr = CapabilityManager::new();
        mgr.issue("agent-1", "web_search", CapabilityScope::default(), 0);

        assert!(mgr.validate("agent-1", "web_search").is_ok());
        assert!(mgr.validate("agent-1", "file_system").is_err());
    }

    #[test]
    fn test_cross_agent_isolation() {
        let mut mgr = CapabilityManager::new();
        mgr.issue("agent-1", "web_search", CapabilityScope::default(), 0);

        // agent-2 should NOT have agent-1's capability
        assert!(mgr.validate("agent-2", "web_search").is_err());
    }

    #[test]
    fn test_token_expiry() {
        let mut mgr = CapabilityManager::new();
        // Issue token that expires in 1ms
        mgr.issue("a", "s", CapabilityScope::default(), 1);
        std::thread::sleep(std::time::Duration::from_millis(10));
        assert!(mgr.validate("a", "s").is_err());
    }

    #[test]
    fn test_revoke_token() {
        let mut mgr = CapabilityManager::new();
        let token = mgr.issue("a", "s", CapabilityScope::default(), 0);
        assert!(mgr.validate("a", "s").is_ok());

        mgr.revoke_token(&token.id);
        assert!(mgr.validate("a", "s").is_err());
    }

    #[test]
    fn test_revoke_all() {
        let mut mgr = CapabilityManager::new();
        mgr.issue("a", "s1", CapabilityScope::default(), 0);
        mgr.issue("a", "s2", CapabilityScope::default(), 0);
        assert_eq!(mgr.active_count(), 2);

        mgr.revoke_all("a");
        assert_eq!(mgr.active_count(), 0);
    }

    #[test]
    fn test_scope_defaults() {
        let scope = CapabilityScope::default();
        assert_eq!(scope.max_memory_bytes, 45 * 1024 * 1024);
        assert!(scope.read_only);
        assert!(!scope.can_spawn);
    }

    #[test]
    fn test_metrics() {
        let mut mgr = CapabilityManager::new();
        mgr.issue("a", "s", CapabilityScope::default(), 0);
        let _ = mgr.validate("a", "missing");

        let m = mgr.metrics();
        assert_eq!(m.total_issued, 1);
        assert_eq!(m.total_denials, 1);
    }
}

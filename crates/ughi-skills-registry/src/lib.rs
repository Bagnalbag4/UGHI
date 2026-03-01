// ughi-skills-registry/src/lib.rs
pub mod client;
pub mod integrity;

pub use client::{
    InstallStatus, InstalledSkill, RegistryMetrics, SafetyScore, SkillCategory, SkillEntry,
    SkillStub, SkillsShClient, SyncResult,
};

pub use integrity::{
    sha256_hex, IntegrityInfo, IntegrityMetrics, IntegrityVerifier, Sha256, VerificationResult,
    VirusTotalStatus,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_full_pipeline() {
        let mut client = SkillsShClient::new();

        // Full catalog = 79,525+
        assert_eq!(client.total_skills(), 79_525);

        // Sync adds new skills
        let sync = client.sync();
        assert!(sync.total_skills > 79_525);

        // Search by multiple criteria
        assert!(!client.search("vercel").is_empty());
        assert!(!client.search("react").is_empty());
        assert!(!client.search("docker").is_empty());

        // Install top skill
        client.install("vercel-react-best-practices").unwrap();
        assert_eq!(client.installed_count(), 1);

        // Leaderboard top 5
        let top = client.leaderboard(5);
        assert_eq!(top.len(), 5);
        assert!(top[0].installs > 40_000);
    }
}

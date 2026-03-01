// UGHI-marketplace/src/lib.rs
pub mod registry;

pub use registry::{
    InstallStatus, InstalledSkill, Marketplace, MarketplaceMetrics, SkillCategory, SkillListing,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_marketplace_e2e() {
        let mut mp = Marketplace::new();
        let results = mp.search("email");
        assert!(!results.is_empty());
        mp.install(results[0].id).unwrap();
        assert_eq!(mp.installed_count(), 1);
    }
}

// UGHI-fleet/src/lib.rs
pub mod manager;

pub use manager::{FleetInstance, FleetManager, FleetMetrics, InstanceStatus};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fleet_e2e() {
        let mut fleet = FleetManager::new();
        let id = fleet.register_local("1.0.0");
        fleet.clone_instance(id, "10.0.0.2", 8081);
        fleet.update_metrics(id, 20, 100, 1024);
        assert_eq!(fleet.count(), 2);
        assert_eq!(fleet.metrics().total_agents_active, 20);
    }
}

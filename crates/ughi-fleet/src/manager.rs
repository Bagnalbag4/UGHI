// UGHI-fleet/src/manager.rs
// Multi-instance cloning + fleet management
// Memory: ~2 KB per instance record

use serde::{Deserialize, Serialize};
use tracing::info;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstanceStatus {
    Running,
    Stopped,
    Cloning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetInstance {
    pub id: u64,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub status: InstanceStatus,
    pub agents_active: u32,
    pub agents_total: u64,
    pub memory_mb: u32,
    pub uptime_ms: u64,
    pub version: String,
}

/// Fleet manager — orchestrate multiple UGHI instances.
pub struct FleetManager {
    instances: Vec<FleetInstance>,
    next_id: u64,
}

impl FleetManager {
    pub fn new() -> Self {
        Self {
            instances: Vec::with_capacity(16),
            next_id: 1,
        }
    }

    /// Register local instance.
    pub fn register_local(&mut self, version: &str) -> u64 {
        let id = self.next_id;
        self.next_id += 1;

        self.instances.push(FleetInstance {
            id,
            name: format!("local-{}", id),
            host: "127.0.0.1".to_string(),
            port: 8080 + (id as u16 - 1),
            status: InstanceStatus::Running,
            agents_active: 0,
            agents_total: 0,
            memory_mb: 180,
            uptime_ms: 0,
            version: version.to_string(),
        });

        info!(id, "local instance registered");
        id
    }

    /// Clone to a new instance (remote or local).
    pub fn clone_instance(
        &mut self,
        source_id: u64,
        target_host: &str,
        target_port: u16,
    ) -> Option<u64> {
        let source = self.instances.iter().find(|i| i.id == source_id)?;
        let version = source.version.clone();

        let id = self.next_id;
        self.next_id += 1;

        self.instances.push(FleetInstance {
            id,
            name: format!("clone-{}", id),
            host: target_host.to_string(),
            port: target_port,
            status: InstanceStatus::Running,
            agents_active: 0,
            agents_total: 0,
            memory_mb: 180,
            uptime_ms: 0,
            version,
        });

        info!(
            id,
            source = source_id,
            host = target_host,
            "instance cloned"
        );
        Some(id)
    }

    /// Update instance metrics.
    pub fn update_metrics(&mut self, id: u64, active: u32, total: u64, memory_mb: u32) {
        if let Some(inst) = self.instances.iter_mut().find(|i| i.id == id) {
            inst.agents_active = active;
            inst.agents_total = total;
            inst.memory_mb = memory_mb;
        }
    }

    /// Stop an instance.
    pub fn stop(&mut self, id: u64) -> bool {
        if let Some(inst) = self.instances.iter_mut().find(|i| i.id == id) {
            inst.status = InstanceStatus::Stopped;
            true
        } else {
            false
        }
    }

    /// Get all instances.
    pub fn list(&self) -> &[FleetInstance] {
        &self.instances
    }
    pub fn count(&self) -> usize {
        self.instances.len()
    }

    /// Get total agents across fleet.
    pub fn fleet_agents(&self) -> (u32, u64) {
        let active: u32 = self
            .instances
            .iter()
            .filter(|i| i.status == InstanceStatus::Running)
            .map(|i| i.agents_active)
            .sum();
        let total: u64 = self.instances.iter().map(|i| i.agents_total).sum();
        (active, total)
    }

    /// Get total memory across fleet.
    pub fn fleet_memory_mb(&self) -> u32 {
        self.instances
            .iter()
            .filter(|i| i.status == InstanceStatus::Running)
            .map(|i| i.memory_mb)
            .sum()
    }

    pub fn metrics(&self) -> FleetMetrics {
        let (active, total) = self.fleet_agents();
        FleetMetrics {
            instances: self.instances.len() as u32,
            running: self
                .instances
                .iter()
                .filter(|i| i.status == InstanceStatus::Running)
                .count() as u32,
            total_agents_active: active,
            total_agents_all: total,
            total_memory_mb: self.fleet_memory_mb(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FleetMetrics {
    pub instances: u32,
    pub running: u32,
    pub total_agents_active: u32,
    pub total_agents_all: u64,
    pub total_memory_mb: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_local() {
        let mut fleet = FleetManager::new();
        let id = fleet.register_local("1.0.0");
        assert_eq!(id, 1);
        assert_eq!(fleet.count(), 1);
    }

    #[test]
    fn test_clone_instance() {
        let mut fleet = FleetManager::new();
        let src = fleet.register_local("1.0.0");
        let clone = fleet.clone_instance(src, "192.168.1.100", 8081);
        assert!(clone.is_some());
        assert_eq!(fleet.count(), 2);
    }

    #[test]
    fn test_fleet_metrics() {
        let mut fleet = FleetManager::new();
        let id1 = fleet.register_local("1.0.0");
        let id2 = fleet.register_local("1.0.0");
        fleet.update_metrics(id1, 10, 50, 512);
        fleet.update_metrics(id2, 5, 30, 256);
        let m = fleet.metrics();
        assert_eq!(m.total_agents_active, 15);
        assert_eq!(m.total_memory_mb, 768);
    }

    #[test]
    fn test_stop_instance() {
        let mut fleet = FleetManager::new();
        let id = fleet.register_local("1.0.0");
        assert!(fleet.stop(id));
        assert_eq!(fleet.list()[0].status, InstanceStatus::Stopped);
    }
}

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// System for backing up and restoring agent state.
/// Follows strict_rules.md | Memory cost: ~128 bytes (struct on stack)
pub struct BackupManager {
    backup_dir: PathBuf,
}

impl BackupManager {
    /// Create a new backup manager storing backups in the specified directory.
    pub fn new<P: AsRef<Path>>(dir: P) -> io::Result<Self> {
        let backup_dir = dir.as_ref().to_path_buf();
        if !backup_dir.exists() {
            fs::create_dir_all(&backup_dir)?;
        }
        Ok(Self { backup_dir })
    }

    /// Takes a snapshot of an agent's state bytes and saves it to a timestamped backup file.
    /// Memory cost: ~Length of data
    pub fn create_backup(&self, agent_id: &str, data: &[u8]) -> io::Result<PathBuf> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let filename = format!("{}_{}.bak", agent_id, timestamp);
        let filepath = self.backup_dir.join(filename);

        // Strict rules: keep IO fast, but we need to write to disk.
        let mut file = fs::File::create(&filepath)?;
        file.write_all(data)?;
        file.sync_all()?;

        Ok(filepath)
    }

    /// Restores the latest state backup for a given agent ID.
    pub fn restore_latest(&self, agent_id: &str) -> io::Result<Option<Vec<u8>>> {
        let mut latest_file = None;
        let mut latest_time = 0;

        for entry in fs::read_dir(&self.backup_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() {
                if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                    if filename.starts_with(agent_id) && filename.ends_with(".bak") {
                        // Extract timestamp from filename format `agent_id_timestamp.bak`
                        let parts: Vec<&str> = filename.split('_').collect();
                        if parts.len() >= 2 {
                            let last_part = parts.last().unwrap();
                            let ts_str = last_part.trim_end_matches(".bak");
                            if let Ok(ts) = ts_str.parse::<u64>() {
                                if ts > latest_time {
                                    latest_time = ts;
                                    latest_file = Some(path);
                                }
                            }
                        }
                    }
                }
            }
        }

        if let Some(path) = latest_file {
            let mut file = fs::File::open(path)?;
            let mut data = Vec::new();
            file.read_to_end(&mut data)?;
            Ok(Some(data))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_backup_restore() {
        let dir = tempdir().unwrap();
        let manager = BackupManager::new(dir.path()).unwrap();

        let agent_id = "agent_backup_test";
        let state_data = b"test_state_123";

        manager.create_backup(agent_id, state_data).unwrap();

        let restored = manager
            .restore_latest(agent_id)
            .unwrap()
            .expect("should find backup");
        assert_eq!(restored, state_data);
    }
}

use rusqlite::Connection;
use std::sync::Arc;
use std::sync::Mutex;

/// Handles SQLite database schema migrations for the UGHI memory store.
/// Memory cost: ~64 bytes (struct on stack)
/// Strict rules: no heavy ORMs, raw SQL via rusqlite
pub struct MigrationRunner {
    db_conn: Arc<Mutex<Connection>>,
}

impl MigrationRunner {
    pub fn new(db_conn: Arc<Mutex<Connection>>) -> Self {
        Self { db_conn }
    }

    /// Runs all pending SQL migrations synchronously.
    /// Memory cost: ~256 bytes for executing queries
    pub fn run_migrations(&self) -> Result<(), rusqlite::Error> {
        let conn = self.db_conn.lock().unwrap();

        // Ensure migrations table exists
        conn.execute(
            "CREATE TABLE IF NOT EXISTS ughi_migrations (
                version INTEGER PRIMARY KEY,
                applied_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        // Fetch current version
        let current_version: i64 = match conn.query_row(
            "SELECT MAX(version) FROM ughi_migrations",
            [],
            |row| -> Result<i64, rusqlite::Error> { row.get(0) },
        ) {
            Ok(v) => v,
            Err(_) => 0, // No migrations run
        };

        // Define migrations (Up)
        let migrations = vec![
            (
                1,
                "CREATE TABLE IF NOT EXISTS memory_nodes (
                    id TEXT PRIMARY KEY,
                    agent_id TEXT NOT NULL,
                    content TEXT NOT NULL,
                    created_at INTEGER NOT NULL
                )",
            ),
            (
                2,
                "CREATE INDEX IF NOT EXISTS idx_memory_agent ON memory_nodes (agent_id)",
            ),
            (
                3,
                "CREATE TABLE IF NOT EXISTS vector_index (
                    id TEXT PRIMARY KEY,
                    vector BLOB NOT NULL,
                    node_id TEXT NOT NULL,
                    FOREIGN KEY(node_id) REFERENCES memory_nodes(id) ON DELETE CASCADE
                )",
            ),
            (
                4,
                "CREATE TABLE IF NOT EXISTS agent_state_backups (
                    id TEXT PRIMARY KEY,
                    agent_id TEXT NOT NULL,
                    snapshot_data BLOB NOT NULL,
                    backup_time INTEGER NOT NULL
                )",
            ),
        ];

        // Apply pending migrations
        for (version, sql) in migrations {
            if version > current_version {
                conn.execute(sql, [])?;
                conn.execute(
                    "INSERT INTO ughi_migrations (version) VALUES (?1)",
                    [version],
                )?;
                tracing::info!("Applied database migration v{}", version);
            }
        }

        Ok(())
    }
}

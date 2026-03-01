// UGHI-memory/src/long_term.rs
// Follows strict_rules.md | File-backed SQLite + BLOB embeddings
// Memory cost: ~8 MB (SQLite connection + page cache)
// Disk budget: ≤ 280 MB total, ≤ 14 MB per agent
// 90-day retention with time-decay pruning.

use rusqlite::{params, Connection};
use tracing::info;

use crate::error::{MemoryError, MemoryResult};
use crate::search::cosine_similarity;
use crate::types::{MemoryEntry, MemoryQuery, MemoryTier, SearchResult, current_time_ms};

/// Per-agent long-term disk budget (14 MB).
const AGENT_DISK_BUDGET_BYTES: u64 = 14 * 1024 * 1024;

/// Retention period (90 days in milliseconds).
const RETENTION_MS: u64 = 90 * 24 * 60 * 60 * 1000;

/// Long-term persistent memory store backed by SQLite.
/// Memory cost: ~8 MB (connection + page cache + prepared stmts)
pub struct LongTermStore {
    conn: Connection,
}

impl LongTermStore {
    /// Open or create the long-term memory database.
    /// Memory cost: ~8 MB (SQLite init + schema)
    /// Pass ":memory:" for in-memory (tests), or a file path for persistence.
    pub fn open(db_path: &str) -> MemoryResult<Self> {
        let conn = if db_path == ":memory:" {
            Connection::open_in_memory()?
        } else {
            // Ensure parent directory exists
            if let Some(parent) = std::path::Path::new(db_path).parent() {
                std::fs::create_dir_all(parent)?;
            }
            Connection::open(db_path)?
        };

        // Performance pragmas for low-latency queries
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA cache_size = -2000;
             PRAGMA temp_store = MEMORY;",
        )?;

        // Create schema
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS memory_entries (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                agent_id TEXT NOT NULL,
                key TEXT NOT NULL,
                value TEXT NOT NULL,
                embedding BLOB,
                created_at INTEGER NOT NULL,
                last_accessed INTEGER NOT NULL,
                access_count INTEGER NOT NULL DEFAULT 0,
                UNIQUE(agent_id, key)
            );

            CREATE INDEX IF NOT EXISTS idx_agent_id ON memory_entries(agent_id);
            CREATE INDEX IF NOT EXISTS idx_agent_key ON memory_entries(agent_id, key);
            CREATE INDEX IF NOT EXISTS idx_last_accessed ON memory_entries(last_accessed);

            CREATE TABLE IF NOT EXISTS agent_usage (
                agent_id TEXT PRIMARY KEY,
                bytes_used INTEGER NOT NULL DEFAULT 0,
                entry_count INTEGER NOT NULL DEFAULT 0
            );",
        )?;

        info!(db_path, "long-term memory store opened");
        Ok(Self { conn })
    }

    /// Store an entry in long-term memory.
    /// Memory cost: proportional to entry
    pub fn put(&self, entry: &MemoryEntry) -> MemoryResult<()> {
        let value_str = serde_json::to_string(&entry.value)
            .map_err(|e| MemoryError::Serialization(e.to_string()))?;

        let embedding_bytes = entry.embedding.as_ref().map(|v| {
            v.iter().flat_map(|f| f.to_le_bytes()).collect::<Vec<u8>>()
        });

        self.conn.execute(
            "INSERT OR REPLACE INTO memory_entries
             (agent_id, key, value, embedding, created_at, last_accessed, access_count)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                entry.agent_id,
                entry.key,
                value_str,
                embedding_bytes,
                entry.created_at,
                entry.last_accessed,
                entry.access_count,
            ],
        )?;

        // Update usage tracking
        let entry_size = value_str.len() as i64
            + embedding_bytes.as_ref().map(|b| b.len() as i64).unwrap_or(0);

        self.conn.execute(
            "INSERT INTO agent_usage (agent_id, bytes_used, entry_count) VALUES (?1, ?2, 1)
             ON CONFLICT(agent_id) DO UPDATE SET
                bytes_used = bytes_used + ?2,
                entry_count = entry_count + 1",
            params![entry.agent_id, entry_size],
        )?;

        Ok(())
    }

    /// Get an entry by agent_id and key.
    pub fn get(&self, agent_id: &str, key: &str) -> MemoryResult<MemoryEntry> {
        let mut stmt = self.conn.prepare(
            "SELECT agent_id, key, value, embedding, created_at, last_accessed, access_count
             FROM memory_entries WHERE agent_id = ?1 AND key = ?2",
        )?;

        let entry = stmt.query_row(params![agent_id, key], |row| {
            Ok(self.row_to_entry(row))
        })?.map_err(|_| MemoryError::KeyNotFound { key: key.to_string() })?;

        // Touch access timestamp
        let now = current_time_ms();
        self.conn.execute(
            "UPDATE memory_entries SET last_accessed = ?1, access_count = access_count + 1
             WHERE agent_id = ?2 AND key = ?3",
            params![now, agent_id, key],
        )?;

        Ok(entry)
    }

    /// Search long-term memory with optional semantic similarity.
    /// Latency target: < 40 ms
    pub fn search(&self, query: &MemoryQuery) -> MemoryResult<Vec<SearchResult>> {
        let sql = if query.key_prefix.is_some() {
            "SELECT agent_id, key, value, embedding, created_at, last_accessed, access_count
             FROM memory_entries WHERE agent_id = ?1 AND key LIKE ?2
             ORDER BY last_accessed DESC LIMIT ?3"
        } else {
            "SELECT agent_id, key, value, embedding, created_at, last_accessed, access_count
             FROM memory_entries WHERE agent_id = ?1
             ORDER BY last_accessed DESC LIMIT ?2"
        };

        let mut stmt = self.conn.prepare(sql)?;

        let entries: Vec<MemoryEntry> = if let Some(ref prefix) = query.key_prefix {
            let pattern = format!("{}%", prefix);
            let rows = stmt.query_map(params![query.agent_id, pattern, query.limit as i64], |row| {
                Ok(self.row_to_entry(row))
            })?;
            rows.filter_map(|r| r.ok()).filter_map(|r| r.ok()).collect()
        } else {
            let rows = stmt.query_map(params![query.agent_id, query.limit as i64], |row| {
                Ok(self.row_to_entry(row))
            })?;
            rows.filter_map(|r| r.ok()).filter_map(|r| r.ok()).collect()
        };

        // Apply semantic similarity if query has embedding
        let mut results: Vec<SearchResult> = entries.into_iter()
            .map(|entry| {
                let similarity = if let (Some(ref q_emb), Some(ref e_emb)) =
                    (&query.embedding, &entry.embedding)
                {
                    Some(cosine_similarity(q_emb, e_emb))
                } else {
                    None
                };
                SearchResult { entry, similarity }
            })
            .filter(|r| {
                r.similarity.map(|s| s >= query.min_similarity).unwrap_or(true)
            })
            .collect();

        // Sort by similarity if semantic search
        if query.embedding.is_some() {
            results.sort_by(|a, b| {
                b.similarity.unwrap_or(0.0)
                    .partial_cmp(&a.similarity.unwrap_or(0.0))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }

        results.truncate(query.limit);
        Ok(results)
    }

    /// Prune entries older than 90 days.
    /// Memory cost: 0 (SQLite DELETE)
    pub fn prune_expired(&self) -> MemoryResult<u64> {
        let cutoff = current_time_ms().saturating_sub(RETENTION_MS);
        let deleted = self.conn.execute(
            "DELETE FROM memory_entries WHERE created_at < ?1",
            params![cutoff],
        )? as u64;

        if deleted > 0 {
            info!(pruned = deleted, "pruned expired long-term entries");
        }
        Ok(deleted)
    }

    /// Prune agent entries to stay under disk budget.
    pub fn prune_agent_budget(&self, agent_id: &str) -> MemoryResult<u64> {
        let usage = self.agent_usage_bytes(agent_id)?;
        if usage <= AGENT_DISK_BUDGET_BYTES {
            return Ok(0);
        }

        // Delete oldest entries until under budget
        let over = usage - AGENT_DISK_BUDGET_BYTES;
        let deleted = self.conn.execute(
            "DELETE FROM memory_entries WHERE id IN (
                SELECT id FROM memory_entries WHERE agent_id = ?1
                ORDER BY last_accessed ASC LIMIT ?2
            )",
            params![agent_id, (over / 512).max(1) as i64], // Rough estimate
        )? as u64;

        info!(agent_id, deleted, over_bytes = over, "pruned agent budget");
        Ok(deleted)
    }

    /// Get disk usage for an agent.
    pub fn agent_usage_bytes(&self, agent_id: &str) -> MemoryResult<u64> {
        let mut stmt = self.conn.prepare(
            "SELECT COALESCE(SUM(LENGTH(value) + COALESCE(LENGTH(embedding), 0)), 0)
             FROM memory_entries WHERE agent_id = ?1",
        )?;
        let usage: u64 = stmt.query_row(params![agent_id], |row| row.get(0))?;
        Ok(usage)
    }

    /// Count entries for an agent.
    pub fn agent_entry_count(&self, agent_id: &str) -> MemoryResult<u64> {
        let mut stmt = self.conn.prepare(
            "SELECT COUNT(*) FROM memory_entries WHERE agent_id = ?1",
        )?;
        let count: u64 = stmt.query_row(params![agent_id], |row| row.get(0))?;
        Ok(count)
    }

    /// Total entries across all agents.
    pub fn total_entries(&self) -> MemoryResult<u64> {
        let mut stmt = self.conn.prepare("SELECT COUNT(*) FROM memory_entries")?;
        let count: u64 = stmt.query_row([], |row| row.get(0))?;
        Ok(count)
    }

    /// Total disk usage (approximate).
    pub fn total_disk_bytes(&self) -> MemoryResult<u64> {
        let mut stmt = self.conn.prepare(
            "SELECT COALESCE(SUM(LENGTH(value) + COALESCE(LENGTH(embedding), 0)), 0)
             FROM memory_entries",
        )?;
        let size: u64 = stmt.query_row([], |row| row.get(0))?;
        Ok(size)
    }

    /// Delete an agent's entire long-term namespace.
    pub fn delete_namespace(&self, agent_id: &str) -> MemoryResult<u64> {
        let deleted = self.conn.execute(
            "DELETE FROM memory_entries WHERE agent_id = ?1",
            params![agent_id],
        )? as u64;
        self.conn.execute(
            "DELETE FROM agent_usage WHERE agent_id = ?1",
            params![agent_id],
        )?;
        info!(agent_id, deleted, "cleared long-term namespace");
        Ok(deleted)
    }

    /// Parse a database row into a MemoryEntry.
    fn row_to_entry(&self, row: &rusqlite::Row) -> Result<MemoryEntry, rusqlite::Error> {
        let value_str: String = row.get(2)?;
        let value: serde_json::Value = serde_json::from_str(&value_str)
            .unwrap_or(serde_json::Value::Null);

        let embedding_blob: Option<Vec<u8>> = row.get(3)?;
        let embedding = embedding_blob.map(|bytes| {
            bytes.chunks_exact(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect()
        });

        Ok(MemoryEntry {
            agent_id: row.get(0)?,
            key: row.get(1)?,
            value,
            embedding,
            created_at: row.get(4)?,
            last_accessed: row.get(5)?,
            access_count: row.get(6)?,
            tier: MemoryTier::LongTerm,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> LongTermStore {
        LongTermStore::open(":memory:").unwrap()
    }

    #[test]
    fn test_put_and_get() {
        let store = test_store();
        let entry = MemoryEntry::new_short("agent-1", "task", serde_json::json!({"goal": "test"}));
        store.put(&entry).unwrap();

        let result = store.get("agent-1", "task").unwrap();
        assert_eq!(result.value["goal"], "test");
        assert_eq!(result.tier, MemoryTier::LongTerm);
    }

    #[test]
    fn test_embedding_roundtrip() {
        let store = test_store();
        let mut entry = MemoryEntry::new_short("a", "emb", serde_json::json!("test"));
        entry.embedding = Some(vec![1.0, 2.0, 3.0, 4.5]);
        store.put(&entry).unwrap();

        let result = store.get("a", "emb").unwrap();
        let emb = result.embedding.unwrap();
        assert_eq!(emb.len(), 4);
        assert!((emb[0] - 1.0).abs() < 1e-5);
        assert!((emb[3] - 4.5).abs() < 1e-5);
    }

    #[test]
    fn test_search_by_prefix() {
        let store = test_store();
        store.put(&MemoryEntry::new_short("a", "task:1", serde_json::json!("t1"))).unwrap();
        store.put(&MemoryEntry::new_short("a", "task:2", serde_json::json!("t2"))).unwrap();
        store.put(&MemoryEntry::new_short("a", "note:1", serde_json::json!("n1"))).unwrap();

        let q = MemoryQuery::by_agent("a").with_key_prefix("task:");
        let results = store.search(&q).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_namespace_isolation() {
        let store = test_store();
        store.put(&MemoryEntry::new_short("a1", "key", serde_json::json!("val1"))).unwrap();
        store.put(&MemoryEntry::new_short("a2", "key", serde_json::json!("val2"))).unwrap();

        assert_eq!(store.get("a1", "key").unwrap().value, serde_json::json!("val1"));
        assert_eq!(store.get("a2", "key").unwrap().value, serde_json::json!("val2"));
    }

    #[test]
    fn test_delete_namespace() {
        let store = test_store();
        store.put(&MemoryEntry::new_short("a", "k1", serde_json::json!("v"))).unwrap();
        store.put(&MemoryEntry::new_short("a", "k2", serde_json::json!("v"))).unwrap();

        let deleted = store.delete_namespace("a").unwrap();
        assert_eq!(deleted, 2);
        assert_eq!(store.agent_entry_count("a").unwrap(), 0);
    }

    #[test]
    fn test_usage_tracking() {
        let store = test_store();
        store.put(&MemoryEntry::new_short("a", "k", serde_json::json!({"x": "y"}))).unwrap();

        let usage = store.agent_usage_bytes("a").unwrap();
        assert!(usage > 0);
    }
}

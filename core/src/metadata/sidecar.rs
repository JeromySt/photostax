use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};

/// Name of the sidecar database file placed alongside photo directories.
pub const SIDECAR_DB_NAME: &str = ".photostax.db";

/// A sidecar SQLite database for storing extended metadata per PhotoStack.
pub struct SidecarDb {
    conn: Connection,
}

impl SidecarDb {
    /// Open (or create) the sidecar database in the given directory.
    pub fn open(directory: &Path) -> Result<Self, SidecarError> {
        let db_path = directory.join(SIDECAR_DB_NAME);
        Self::open_path(&db_path)
    }

    /// Open (or create) the sidecar database at an explicit path.
    pub fn open_path(path: &PathBuf) -> Result<Self, SidecarError> {
        let conn = Connection::open(path)?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    /// Run schema migrations to ensure tables exist.
    fn migrate(&self) -> Result<(), SidecarError> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS stack_metadata (
                stack_id TEXT NOT NULL,
                key      TEXT NOT NULL,
                value    TEXT NOT NULL,
                PRIMARY KEY (stack_id, key)
            );

            CREATE INDEX IF NOT EXISTS idx_stack_metadata_key
                ON stack_metadata (key);
            ",
        )?;
        Ok(())
    }

    /// Get all custom tags for a photo stack.
    pub fn get_tags(
        &self,
        stack_id: &str,
    ) -> Result<HashMap<String, serde_json::Value>, SidecarError> {
        let mut stmt = self
            .conn
            .prepare("SELECT key, value FROM stack_metadata WHERE stack_id = ?1")?;

        let rows = stmt.query_map(params![stack_id], |row| {
            let key: String = row.get(0)?;
            let value_str: String = row.get(1)?;
            Ok((key, value_str))
        })?;

        let mut tags = HashMap::new();
        for row in rows {
            let (key, value_str) = row?;
            let value: serde_json::Value =
                serde_json::from_str(&value_str).unwrap_or(serde_json::Value::String(value_str));
            tags.insert(key, value);
        }
        Ok(tags)
    }

    /// Set a single custom tag for a photo stack (upsert).
    pub fn set_tag(
        &self,
        stack_id: &str,
        key: &str,
        value: &serde_json::Value,
    ) -> Result<(), SidecarError> {
        let value_str = serde_json::to_string(value)
            .map_err(|e| SidecarError::Serialization(e.to_string()))?;

        self.conn.execute(
            "INSERT INTO stack_metadata (stack_id, key, value)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(stack_id, key) DO UPDATE SET value = excluded.value",
            params![stack_id, key, value_str],
        )?;
        Ok(())
    }

    /// Set multiple custom tags for a photo stack at once.
    pub fn set_tags(
        &self,
        stack_id: &str,
        tags: &HashMap<String, serde_json::Value>,
    ) -> Result<(), SidecarError> {
        let tx = self.conn.unchecked_transaction()?;
        for (key, value) in tags {
            let value_str = serde_json::to_string(value)
                .map_err(|e| SidecarError::Serialization(e.to_string()))?;

            tx.execute(
                "INSERT INTO stack_metadata (stack_id, key, value)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(stack_id, key) DO UPDATE SET value = excluded.value",
                params![stack_id, key, value_str],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Remove a single custom tag.
    pub fn remove_tag(&self, stack_id: &str, key: &str) -> Result<bool, SidecarError> {
        let count = self.conn.execute(
            "DELETE FROM stack_metadata WHERE stack_id = ?1 AND key = ?2",
            params![stack_id, key],
        )?;
        Ok(count > 0)
    }

    /// Remove all custom tags for a photo stack.
    pub fn remove_all_tags(&self, stack_id: &str) -> Result<usize, SidecarError> {
        let count = self.conn.execute(
            "DELETE FROM stack_metadata WHERE stack_id = ?1",
            params![stack_id],
        )?;
        Ok(count)
    }

    /// Find all stack IDs that have a specific tag key.
    pub fn find_stacks_by_key(&self, key: &str) -> Result<Vec<String>, SidecarError> {
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT stack_id FROM stack_metadata WHERE key = ?1")?;

        let rows = stmt.query_map(params![key], |row| row.get(0))?;
        let mut ids = Vec::new();
        for row in rows {
            ids.push(row?);
        }
        Ok(ids)
    }

    /// Search for stacks where a tag value contains the given text.
    pub fn search_tags(&self, query: &str) -> Result<Vec<(String, String, String)>, SidecarError> {
        let mut stmt = self.conn.prepare(
            "SELECT stack_id, key, value FROM stack_metadata WHERE value LIKE '%' || ?1 || '%'",
        )?;

        let pattern = query;
        let rows = stmt.query_map(params![pattern], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }
}

/// Errors from sidecar database operations.
#[derive(Debug, thiserror::Error)]
pub enum SidecarError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Serialization error: {0}")]
    Serialization(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_db() -> (TempDir, SidecarDb) {
        let tmp = TempDir::new().unwrap();
        let db = SidecarDb::open(tmp.path()).unwrap();
        (tmp, db)
    }

    #[test]
    fn test_set_and_get_tags() {
        let (_tmp, db) = test_db();

        db.set_tag("IMG_001", "ocr_text", &serde_json::json!("Hello World"))
            .unwrap();
        db.set_tag("IMG_001", "processed", &serde_json::json!(true))
            .unwrap();

        let tags = db.get_tags("IMG_001").unwrap();
        assert_eq!(tags.len(), 2);
        assert_eq!(tags["ocr_text"], serde_json::json!("Hello World"));
        assert_eq!(tags["processed"], serde_json::json!(true));
    }

    #[test]
    fn test_upsert_tag() {
        let (_tmp, db) = test_db();

        db.set_tag("IMG_001", "status", &serde_json::json!("pending"))
            .unwrap();
        db.set_tag("IMG_001", "status", &serde_json::json!("done"))
            .unwrap();

        let tags = db.get_tags("IMG_001").unwrap();
        assert_eq!(tags["status"], serde_json::json!("done"));
    }

    #[test]
    fn test_remove_tag() {
        let (_tmp, db) = test_db();

        db.set_tag("IMG_001", "tag1", &serde_json::json!("value1"))
            .unwrap();
        db.set_tag("IMG_001", "tag2", &serde_json::json!("value2"))
            .unwrap();

        assert!(db.remove_tag("IMG_001", "tag1").unwrap());
        assert!(!db.remove_tag("IMG_001", "nonexistent").unwrap());

        let tags = db.get_tags("IMG_001").unwrap();
        assert_eq!(tags.len(), 1);
        assert!(tags.contains_key("tag2"));
    }

    #[test]
    fn test_batch_set_tags() {
        let (_tmp, db) = test_db();

        let mut tags = HashMap::new();
        tags.insert("key1".to_string(), serde_json::json!("val1"));
        tags.insert("key2".to_string(), serde_json::json!(42));
        tags.insert("key3".to_string(), serde_json::json!(["a", "b"]));

        db.set_tags("IMG_002", &tags).unwrap();

        let result = db.get_tags("IMG_002").unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result["key2"], serde_json::json!(42));
    }

    #[test]
    fn test_find_stacks_by_key() {
        let (_tmp, db) = test_db();

        db.set_tag("IMG_001", "ocr_text", &serde_json::json!("text1"))
            .unwrap();
        db.set_tag("IMG_002", "ocr_text", &serde_json::json!("text2"))
            .unwrap();
        db.set_tag("IMG_003", "other_key", &serde_json::json!("val"))
            .unwrap();

        let ids = db.find_stacks_by_key("ocr_text").unwrap();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"IMG_001".to_string()));
        assert!(ids.contains(&"IMG_002".to_string()));
    }

    #[test]
    fn test_search_tags() {
        let (_tmp, db) = test_db();

        db.set_tag("IMG_001", "ocr_text", &serde_json::json!("Happy Birthday John"))
            .unwrap();
        db.set_tag("IMG_002", "ocr_text", &serde_json::json!("Merry Christmas"))
            .unwrap();

        let results = db.search_tags("Birthday").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "IMG_001");
    }
}

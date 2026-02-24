//! SQLite sidecar database for custom metadata storage.
//!
//! This module provides a SQLite-based sidecar database for storing extended
//! metadata that doesn't fit in EXIF or XMP. The database is stored alongside
//! photo files as `.photostax.db`.
//!
//! ## Database Schema
//!
//! ```sql
//! CREATE TABLE stack_metadata (
//!     stack_id TEXT NOT NULL,    -- PhotoStack ID (e.g., "IMG_001")
//!     key      TEXT NOT NULL,    -- Tag name (e.g., "ocr_text")
//!     value    TEXT NOT NULL,    -- JSON-serialized value
//!     PRIMARY KEY (stack_id, key)
//! );
//!
//! CREATE INDEX idx_stack_metadata_key ON stack_metadata (key);
//! ```
//!
//! ## Use Cases
//!
//! - **OCR results**: Store extracted text from back-of-photo scans
//! - **Albums/collections**: Organize photos into logical groups
//! - **People tags**: Tag people identified in photos
//! - **Processing status**: Track which photos have been processed
//!
//! ## Examples
//!
//! ```rust,no_run
//! use photostax_core::metadata::sidecar::SidecarDb;
//! use std::path::Path;
//!
//! let db = SidecarDb::open(Path::new("/photos"))?;
//!
//! // Store OCR result
//! db.set_tag("IMG_001", "ocr_text", &serde_json::json!("Happy Birthday!"))?;
//!
//! // Retrieve all tags for a stack
//! let tags = db.get_tags("IMG_001")?;
//! # Ok::<(), photostax_core::metadata::sidecar::SidecarError>(())
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};

/// Name of the sidecar database file placed alongside photo directories.
///
/// The database is created automatically when first accessed.
pub const SIDECAR_DB_NAME: &str = ".photostax.db";

/// A sidecar SQLite database for storing extended metadata per [`PhotoStack`].
///
/// The database provides key-value storage for arbitrary JSON metadata,
/// indexed by stack ID. It's designed for data that doesn't fit in standard
/// EXIF/XMP fields, such as OCR results, custom tags, and processing status.
///
/// # Schema
///
/// The database uses a simple key-value schema with composite primary key:
///
/// - `stack_id`: The [`PhotoStack`] ID (e.g., `"IMG_001"`)
/// - `key`: Tag name (e.g., `"ocr_text"`, `"album"`)
/// - `value`: JSON-serialized value
///
/// # Thread Safety
///
/// Each `SidecarDb` instance owns its own connection. For multi-threaded access,
/// create separate instances or use connection pooling.
///
/// [`PhotoStack`]: crate::photo_stack::PhotoStack
pub struct SidecarDb {
    conn: Connection,
}

impl SidecarDb {
    /// Open (or create) the sidecar database in the given directory.
    ///
    /// Creates the database file and schema if they don't exist.
    ///
    /// # Arguments
    ///
    /// * `directory` - Directory containing photo files (database is created here)
    ///
    /// # Errors
    ///
    /// Returns [`SidecarError::Sqlite`] if the database cannot be opened or created.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use photostax_core::metadata::sidecar::SidecarDb;
    /// use std::path::Path;
    ///
    /// let db = SidecarDb::open(Path::new("/photos"))?;
    /// # Ok::<(), photostax_core::metadata::sidecar::SidecarError>(())
    /// ```
    pub fn open(directory: &Path) -> Result<Self, SidecarError> {
        let db_path = directory.join(SIDECAR_DB_NAME);
        Self::open_path(&db_path)
    }

    /// Open (or create) the sidecar database at an explicit path.
    ///
    /// Use this when you need control over the database file location.
    ///
    /// # Arguments
    ///
    /// * `path` - Full path to the database file
    ///
    /// # Errors
    ///
    /// Returns [`SidecarError::Sqlite`] if the database cannot be opened.
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
    ///
    /// # Arguments
    ///
    /// * `stack_id` - The [`PhotoStack`] ID
    ///
    /// # Returns
    ///
    /// A map of tag names to JSON values. Returns empty map if stack has no tags.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use photostax_core::metadata::sidecar::SidecarDb;
    /// use std::path::Path;
    ///
    /// let db = SidecarDb::open(Path::new("/photos"))?;
    /// let tags = db.get_tags("IMG_001")?;
    ///
    /// if let Some(ocr) = tags.get("ocr_text") {
    ///     println!("OCR result: {}", ocr);
    /// }
    /// # Ok::<(), photostax_core::metadata::sidecar::SidecarError>(())
    /// ```
    ///
    /// [`PhotoStack`]: crate::photo_stack::PhotoStack
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
    ///
    /// If the tag already exists, its value is updated.
    ///
    /// # Arguments
    ///
    /// * `stack_id` - The [`PhotoStack`] ID
    /// * `key` - Tag name
    /// * `value` - Tag value (any JSON-serializable value)
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use photostax_core::metadata::sidecar::SidecarDb;
    /// use std::path::Path;
    ///
    /// let db = SidecarDb::open(Path::new("/photos"))?;
    ///
    /// // String value
    /// db.set_tag("IMG_001", "album", &serde_json::json!("Family Reunion"))?;
    ///
    /// // Array value
    /// db.set_tag("IMG_001", "people", &serde_json::json!(["John", "Jane"]))?;
    ///
    /// // Boolean value
    /// db.set_tag("IMG_001", "processed", &serde_json::json!(true))?;
    /// # Ok::<(), photostax_core::metadata::sidecar::SidecarError>(())
    /// ```
    ///
    /// [`PhotoStack`]: crate::photo_stack::PhotoStack
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
    ///
    /// Uses a single transaction for efficiency.
    ///
    /// # Arguments
    ///
    /// * `stack_id` - The [`PhotoStack`] ID
    /// * `tags` - Map of tag names to values
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use photostax_core::metadata::sidecar::SidecarDb;
    /// use std::collections::HashMap;
    /// use std::path::Path;
    ///
    /// let db = SidecarDb::open(Path::new("/photos"))?;
    ///
    /// let mut tags = HashMap::new();
    /// tags.insert("album".to_string(), serde_json::json!("Vacation 2024"));
    /// tags.insert("location".to_string(), serde_json::json!("Hawaii"));
    ///
    /// db.set_tags("IMG_001", &tags)?;
    /// # Ok::<(), photostax_core::metadata::sidecar::SidecarError>(())
    /// ```
    ///
    /// [`PhotoStack`]: crate::photo_stack::PhotoStack
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
    ///
    /// # Arguments
    ///
    /// * `stack_id` - The [`PhotoStack`] ID
    /// * `key` - Tag name to remove
    ///
    /// # Returns
    ///
    /// `true` if the tag existed and was removed, `false` if it didn't exist.
    ///
    /// [`PhotoStack`]: crate::photo_stack::PhotoStack
    pub fn remove_tag(&self, stack_id: &str, key: &str) -> Result<bool, SidecarError> {
        let count = self.conn.execute(
            "DELETE FROM stack_metadata WHERE stack_id = ?1 AND key = ?2",
            params![stack_id, key],
        )?;
        Ok(count > 0)
    }

    /// Remove all custom tags for a photo stack.
    ///
    /// # Arguments
    ///
    /// * `stack_id` - The [`PhotoStack`] ID
    ///
    /// # Returns
    ///
    /// The number of tags that were removed.
    ///
    /// [`PhotoStack`]: crate::photo_stack::PhotoStack
    pub fn remove_all_tags(&self, stack_id: &str) -> Result<usize, SidecarError> {
        let count = self.conn.execute(
            "DELETE FROM stack_metadata WHERE stack_id = ?1",
            params![stack_id],
        )?;
        Ok(count)
    }

    /// Find all stack IDs that have a specific tag key.
    ///
    /// Useful for finding all photos with OCR text, all tagged photos, etc.
    ///
    /// # Arguments
    ///
    /// * `key` - Tag name to search for
    ///
    /// # Returns
    ///
    /// List of stack IDs that have the specified tag.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use photostax_core::metadata::sidecar::SidecarDb;
    /// use std::path::Path;
    ///
    /// let db = SidecarDb::open(Path::new("/photos"))?;
    ///
    /// // Find all photos with OCR text
    /// let ocr_stacks = db.find_stacks_by_key("ocr_text")?;
    /// println!("{} photos have OCR text", ocr_stacks.len());
    /// # Ok::<(), photostax_core::metadata::sidecar::SidecarError>(())
    /// ```
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
    ///
    /// Performs a case-sensitive substring search across all tag values.
    ///
    /// # Arguments
    ///
    /// * `query` - Text to search for in tag values
    ///
    /// # Returns
    ///
    /// List of (stack_id, key, value) tuples for matching tags.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use photostax_core::metadata::sidecar::SidecarDb;
    /// use std::path::Path;
    ///
    /// let db = SidecarDb::open(Path::new("/photos"))?;
    ///
    /// // Search for "birthday" in any tag value
    /// let matches = db.search_tags("birthday")?;
    /// for (stack_id, key, value) in matches {
    ///     println!("{} has '{}' in tag '{}'", stack_id, value, key);
    /// }
    /// # Ok::<(), photostax_core::metadata::sidecar::SidecarError>(())
    /// ```
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
///
/// # Variants
///
/// | Variant | When It Occurs |
/// |---------|----------------|
/// | [`Sqlite`](Self::Sqlite) | Database operation failed (open, query, write) |
/// | [`Serialization`](Self::Serialization) | JSON serialization/deserialization failed |
#[derive(Debug, thiserror::Error)]
pub enum SidecarError {
    /// A SQLite database error occurred.
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// JSON serialization failed.
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

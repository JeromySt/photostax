//! Unified cache manager for multiple photo repositories.
//!
//! [`StackManager`] sits above the [`Repository`] trait and provides:
//! - A flat `HashMap<String, PhotoStack>` cache keyed by opaque stack ID
//! - Multi-repo support with URI-based overlap detection
//! - O(1) `get_stack(id)` lookups
//! - Snapshot creation (frozen cache clones)
//! - Mutation routing to the correct repo via `stack.repo_id`

use std::collections::HashMap;

use crate::events::{CacheEvent, FileVariant, StackEvent};
use crate::hashing::ImageFile;
use crate::photo_stack::{
    Metadata, PhotoStack, Rotation, RotationTarget, ScanProgress, ScannerProfile,
};
use crate::repository::{Repository, RepositoryError};
use crate::search::{paginate_stacks, PaginatedResult, PaginationParams, SearchQuery};
use crate::snapshot::ScanSnapshot;

/// Errors specific to [`StackManager`] operations.
#[derive(Debug, thiserror::Error)]
pub enum StackManagerError {
    /// Two repositories have overlapping locations.
    #[error("Repository locations overlap: '{new}' and '{existing}'")]
    OverlappingRepo {
        /// The location of the newly added repository.
        new: String,
        /// The location of the already-registered repository.
        existing: String,
    },

    /// A repository operation failed.
    #[error(transparent)]
    Repository(#[from] RepositoryError),

    /// The requested repo was not found.
    #[error("Repository not found: {0}")]
    RepoNotFound(String),
}

/// A registered repository with its scan profile.
struct RegisteredRepo {
    repo: Box<dyn Repository>,
    profile: ScannerProfile,
}

/// Unified cache manager for multiple photo repositories.
///
/// Aggregates stacks from N [`Repository`] instances into a single
/// flat `HashMap` cache. All lookups are O(1) by opaque stack ID.
/// Mutation operations (rotate, write metadata) are routed to the
/// correct repository via `PhotoStack::repo_id`.
pub struct StackManager {
    /// Keyed by `repo.location()` since the scanner stores location in `PhotoStack::repo_id`.
    repos: HashMap<String, RegisteredRepo>,
    cache: HashMap<String, PhotoStack>,
}

impl StackManager {
    /// Create an empty `StackManager` with no repositories.
    pub fn new() -> Self {
        Self {
            repos: HashMap::new(),
            cache: HashMap::new(),
        }
    }

    /// Convenience: create a `StackManager` with a single repository.
    pub fn single(
        repo: Box<dyn Repository>,
        profile: ScannerProfile,
    ) -> Result<Self, StackManagerError> {
        let mut mgr = Self::new();
        mgr.add_repo(repo, profile)?;
        Ok(mgr)
    }

    /// Register a repository. Rejects overlapping locations within the same URI scheme.
    pub fn add_repo(
        &mut self,
        repo: Box<dyn Repository>,
        profile: ScannerProfile,
    ) -> Result<&str, StackManagerError> {
        let new_loc = repo.location().to_string();

        for existing in self.repos.values() {
            let existing_loc = existing.repo.location();
            if same_scheme(&new_loc, existing_loc) {
                let new_norm = new_loc.trim_end_matches('/');
                let ext_norm = existing_loc.trim_end_matches('/');
                if new_norm.starts_with(ext_norm) || ext_norm.starts_with(new_norm) {
                    return Err(StackManagerError::OverlappingRepo {
                        new: new_loc,
                        existing: existing_loc.to_string(),
                    });
                }
            }
        }

        let location = repo.location().to_string();
        self.repos
            .insert(location.clone(), RegisteredRepo { repo, profile });
        Ok(self.repos.get(&location).unwrap().repo.id())
    }

    /// Scan all registered repos and populate the cache.
    ///
    /// Clears any existing cache entries first.
    pub fn scan(&mut self) -> Result<usize, StackManagerError> {
        self.cache.clear();
        let mut all_stacks = Vec::new();
        for reg in self.repos.values() {
            let stacks = reg.repo.scan_with_progress(reg.profile, None)?;
            all_stacks.extend(stacks);
        }
        for stack in all_stacks {
            self.cache.insert(stack.id.clone(), stack);
        }
        Ok(self.cache.len())
    }

    /// Scan all repos with a progress callback.
    pub fn scan_with_progress(
        &mut self,
        progress: Option<&mut dyn FnMut(&ScanProgress)>,
    ) -> Result<usize, StackManagerError> {
        self.cache.clear();
        let mut all_stacks = Vec::new();
        if let Some(cb) = progress {
            for reg in self.repos.values() {
                let stacks = reg.repo.scan_with_progress(reg.profile, Some(&mut *cb))?;
                all_stacks.extend(stacks);
            }
        } else {
            for reg in self.repos.values() {
                let stacks = reg.repo.scan_with_progress(reg.profile, None)?;
                all_stacks.extend(stacks);
            }
        }
        for stack in all_stacks {
            self.cache.insert(stack.id.clone(), stack);
        }
        Ok(self.cache.len())
    }

    /// O(1) stack lookup by opaque ID.
    pub fn get_stack(&self, id: &str) -> Option<&PhotoStack> {
        self.cache.get(id)
    }

    /// O(1) mutable stack lookup by opaque ID.
    pub fn get_stack_mut(&mut self, id: &str) -> Option<&mut PhotoStack> {
        self.cache.get_mut(id)
    }

    /// Load metadata for a specific stack, routing to the correct repository.
    pub fn load_metadata(&mut self, id: &str) -> Result<(), StackManagerError> {
        let repo_id = self
            .cache
            .get(id)
            .ok_or_else(|| RepositoryError::NotFound(id.to_string()))?
            .repo_id
            .as_ref()
            .ok_or_else(|| StackManagerError::RepoNotFound(format!("Stack '{id}' has no repo_id")))?
            .clone();

        let reg = self
            .repos
            .get(&repo_id)
            .ok_or(StackManagerError::RepoNotFound(repo_id))?;
        let stack = self.cache.get_mut(id).unwrap();
        reg.repo.load_metadata(stack)?;
        Ok(())
    }

    /// Rotate images in a stack, routing to the correct repository.
    pub fn rotate_stack(
        &mut self,
        id: &str,
        rotation: Rotation,
        target: RotationTarget,
    ) -> Result<&PhotoStack, StackManagerError> {
        let repo_id = self
            .cache
            .get(id)
            .ok_or_else(|| RepositoryError::NotFound(id.to_string()))?
            .repo_id
            .as_ref()
            .ok_or_else(|| StackManagerError::RepoNotFound(format!("Stack '{id}' has no repo_id")))?
            .clone();

        let reg = self
            .repos
            .get(&repo_id)
            .ok_or(StackManagerError::RepoNotFound(repo_id))?;
        let updated = reg.repo.rotate_stack(id, rotation, target)?;
        self.cache.insert(id.to_string(), updated);
        Ok(self.cache.get(id).unwrap())
    }

    /// Write metadata to a stack, routing to the correct repository.
    pub fn write_metadata(&self, id: &str, tags: &Metadata) -> Result<(), StackManagerError> {
        let stack = self
            .cache
            .get(id)
            .ok_or_else(|| RepositoryError::NotFound(id.to_string()))?;
        let repo_id = stack.repo_id.as_ref().ok_or_else(|| {
            StackManagerError::RepoNotFound(format!("Stack '{id}' has no repo_id"))
        })?;
        let reg = self
            .repos
            .get(repo_id)
            .ok_or_else(|| StackManagerError::RepoNotFound(repo_id.clone()))?;

        reg.repo.write_metadata(stack, tags)?;
        Ok(())
    }

    /// Create a snapshot of the entire cache (all repos).
    pub fn snapshot(&self) -> ScanSnapshot {
        let stacks: Vec<PhotoStack> = self.cache.values().cloned().collect();
        ScanSnapshot::from_stacks(stacks)
    }

    /// Create a snapshot of stacks from a specific repo.
    pub fn snapshot_repo(&self, repo_id: &str) -> ScanSnapshot {
        let stacks: Vec<PhotoStack> = self
            .cache
            .values()
            .filter(|s| s.repo_id.as_deref() == Some(repo_id))
            .cloned()
            .collect();
        ScanSnapshot::from_stacks(stacks)
    }

    /// Compare a snapshot against the live cache to detect staleness.
    pub fn check_status(&self, snapshot: &ScanSnapshot) -> crate::snapshot::SnapshotStatus {
        use std::collections::HashSet;

        let current_ids: HashSet<&str> = self.cache.keys().map(|s| s.as_str()).collect();
        let snapshot_ids: HashSet<&str> = snapshot.ids().iter().map(|s| s.as_str()).collect();

        let added = current_ids.difference(&snapshot_ids).count();
        let removed = snapshot_ids.difference(&current_ids).count();

        crate::snapshot::SnapshotStatus {
            is_stale: added > 0 || removed > 0,
            snapshot_count: snapshot.total_count(),
            current_count: self.cache.len(),
            added,
            removed,
        }
    }

    /// Get all stacks in the cache.
    #[deprecated(since = "0.2.1", note = "Use `query()` instead")]
    pub fn stacks(&self) -> Vec<&PhotoStack> {
        self.cache.values().collect()
    }

    /// Query the cache with optional filtering and pagination.
    ///
    /// This is the primary entry point for retrieving stacks. It filters
    /// directly over the cache without cloning, then paginates the results.
    ///
    /// - Empty `SearchQuery` + `None` pagination = all stacks
    /// - `SearchQuery` filters + `None` pagination = all matching stacks
    /// - Any query + `Some(pagination)` = a single page of results
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use photostax_core::stack_manager::StackManager;
    /// # use photostax_core::search::{SearchQuery, PaginationParams};
    /// # let mgr = StackManager::new();
    /// // All stacks, no filter, no pagination
    /// let all = mgr.query(&SearchQuery::new(), None);
    ///
    /// // Filtered, first page of 20
    /// let query = SearchQuery::new().with_has_back(true);
    /// let page1 = mgr.query(&query, Some(&PaginationParams { offset: 0, limit: 20 }));
    ///
    /// // Iterate pages
    /// if let Some(next) = page1.next_page() {
    ///     let page2 = mgr.query(&query, Some(&next));
    /// }
    /// ```
    pub fn query(
        &self,
        query: &SearchQuery,
        pagination: Option<&PaginationParams>,
    ) -> PaginatedResult<PhotoStack> {
        use crate::search::matches_query_ref;

        let filtered: Vec<PhotoStack> = self
            .cache
            .values()
            .filter(|stack| matches_query_ref(stack, query))
            .cloned()
            .collect();

        match pagination {
            Some(params) => paginate_stacks(&filtered, params),
            None => PaginatedResult {
                total_count: filtered.len(),
                offset: 0,
                limit: filtered.len(),
                has_more: false,
                items: filtered,
            },
        }
    }

    /// Total number of stacks in the cache.
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    /// Number of registered repositories.
    pub fn repo_count(&self) -> usize {
        self.repos.len()
    }

    /// Read an image file, trying each registered repo until one succeeds.
    pub fn read_image(
        &self,
        path: &str,
    ) -> Result<Box<dyn crate::file_access::ReadSeek>, RepositoryError> {
        for reg in self.repos.values() {
            if let Ok(reader) = reg.repo.read_image(path) {
                return Ok(reader);
            }
        }
        Err(RepositoryError::NotFound(format!(
            "No repository can read: {path}"
        )))
    }

    /// Scan all repos and load metadata for every stack (slow path).
    pub fn scan_with_metadata(&mut self) -> Result<usize, StackManagerError> {
        self.scan()?;
        let ids: Vec<String> = self.cache.keys().cloned().collect();
        for id in ids {
            self.load_metadata(&id)?;
        }
        Ok(self.cache.len())
    }

    /// Start watching all registered repos for changes.
    /// Returns a receiver for CacheEvents that consumers can listen to.
    pub fn watch(&self) -> Result<std::sync::mpsc::Receiver<CacheEvent>, StackManagerError> {
        let (cache_tx, cache_rx) = std::sync::mpsc::channel();

        for reg in self.repos.values() {
            let repo_rx = reg.repo.watch()?;
            let tx = cache_tx.clone();

            std::thread::spawn(move || {
                for event in repo_rx {
                    let cache_event = match &event {
                        StackEvent::FileChanged { stack_id, .. } => {
                            CacheEvent::StackUpdated(stack_id.clone())
                        }
                        StackEvent::FileRemoved { stack_id, .. } => {
                            CacheEvent::StackUpdated(stack_id.clone())
                        }
                    };
                    if tx.send(cache_event).is_err() {
                        return;
                    }
                }
            });
        }

        Ok(cache_rx)
    }

    /// Process a single StackEvent, updating the cache.
    /// Returns the CacheEvent that resulted, if any.
    pub fn apply_event(&mut self, event: &StackEvent) -> Option<CacheEvent> {
        match event {
            StackEvent::FileChanged {
                stack_id,
                variant,
                path,
                size,
            } => {
                let is_new = !self.cache.contains_key(stack_id);
                let stack = self
                    .cache
                    .entry(stack_id.clone())
                    .or_insert_with(|| PhotoStack::new(stack_id));

                let img = ImageFile::new(path.clone(), *size);
                match variant {
                    FileVariant::Original => stack.original = Some(img),
                    FileVariant::Enhanced => stack.enhanced = Some(img),
                    FileVariant::Back => stack.back = Some(img),
                }

                if is_new {
                    Some(CacheEvent::StackAdded(stack_id.clone()))
                } else {
                    Some(CacheEvent::StackUpdated(stack_id.clone()))
                }
            }
            StackEvent::FileRemoved { stack_id, variant } => {
                if let Some(stack) = self.cache.get_mut(stack_id) {
                    match variant {
                        FileVariant::Original => stack.original = None,
                        FileVariant::Enhanced => stack.enhanced = None,
                        FileVariant::Back => stack.back = None,
                    }

                    if !stack.has_any_image() {
                        self.cache.remove(stack_id);
                        Some(CacheEvent::StackRemoved(stack_id.clone()))
                    } else {
                        Some(CacheEvent::StackUpdated(stack_id.clone()))
                    }
                } else {
                    None
                }
            }
        }
    }
}

impl Default for StackManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if two URIs share the same scheme (e.g., both `file://`).
fn same_scheme(a: &str, b: &str) -> bool {
    let scheme_a = a.split("://").next().unwrap_or("");
    let scheme_b = b.split("://").next().unwrap_or("");
    !scheme_a.is_empty() && scheme_a == scheme_b
}

#[cfg(test)]
#[allow(deprecated)]
mod tests {
    use super::*;
    use crate::backends::local::LocalRepository;
    use crate::repository::Repository;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_jpeg() -> Vec<u8> {
        let mut jpeg = Vec::new();
        jpeg.extend_from_slice(&[0xFF, 0xD8]); // SOI
        jpeg.extend_from_slice(&[0xFF, 0xE0]); // APP0
        let jfif_data = b"JFIF\x00\x01\x01\x00\x00\x01\x00\x01\x00\x00";
        jpeg.extend_from_slice(&((jfif_data.len() + 2) as u16).to_be_bytes());
        jpeg.extend_from_slice(jfif_data);
        jpeg.extend_from_slice(&[0xFF, 0xDB, 0x00, 0x43, 0x00]); // DQT
        jpeg.extend_from_slice(&[16u8; 64]);
        jpeg.extend_from_slice(&[
            0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x01, 0x00, 0x01, 0x01, 0x01, 0x11, 0x00,
        ]); // SOF0
        jpeg.extend_from_slice(&[0xFF, 0xC4, 0x00, 0x1F, 0x00]); // DHT
        jpeg.extend_from_slice(&[
            0x00, 0x01, 0x05, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00,
        ]);
        jpeg.extend_from_slice(&[
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B,
        ]);
        jpeg.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x08, 0x01, 0x01, 0x00, 0x00, 0x3F, 0x00]); // SOS
        jpeg.push(0x7F);
        jpeg.extend_from_slice(&[0xFF, 0xD9]); // EOI
        jpeg
    }

    fn setup_test_dir(tmp: &TempDir, files: &[&str]) {
        let jpeg_data = create_test_jpeg();
        for file in files {
            fs::write(tmp.path().join(file), &jpeg_data).unwrap();
        }
    }

    // ── a) Single repo ──────────────────────────────────────────────────

    #[test]
    fn single_repo_scan_and_get_stack() {
        let tmp = TempDir::new().unwrap();
        setup_test_dir(&tmp, &["IMG_001.jpg", "IMG_001_a.jpg", "IMG_001_b.jpg"]);

        let repo = LocalRepository::new(tmp.path());
        let mut mgr =
            StackManager::single(Box::new(repo), ScannerProfile::EnhancedAndBack).unwrap();

        let count = mgr.scan().unwrap();
        assert_eq!(count, 1);
        assert_eq!(mgr.len(), 1);
        assert!(!mgr.is_empty());
        assert_eq!(mgr.repo_count(), 1);

        let stacks = mgr.stacks();
        assert_eq!(stacks.len(), 1);
        assert_eq!(stacks[0].name, "IMG_001");

        // O(1) lookup by ID
        let id = stacks[0].id.clone();
        let found = mgr.get_stack(&id).unwrap();
        assert_eq!(found.name, "IMG_001");
        assert!(found.original.is_some());
        assert!(found.enhanced.is_some());
        assert!(found.back.is_some());
    }

    // ── b) Multi repo ───────────────────────────────────────────────────

    #[test]
    fn multi_repo_scan_merges_caches() {
        let tmp1 = TempDir::new().unwrap();
        let tmp2 = TempDir::new().unwrap();
        setup_test_dir(&tmp1, &["IMG_001.jpg", "IMG_001_a.jpg", "IMG_001_b.jpg"]);
        setup_test_dir(&tmp2, &["IMG_002.jpg", "IMG_002_a.jpg", "IMG_002_b.jpg"]);

        let repo1 = LocalRepository::new(tmp1.path());
        let repo2 = LocalRepository::new(tmp2.path());
        let repo1_loc = repo1.location().to_string();
        let repo2_loc = repo2.location().to_string();

        let mut mgr = StackManager::new();
        mgr.add_repo(Box::new(repo1), ScannerProfile::EnhancedAndBack)
            .unwrap();
        mgr.add_repo(Box::new(repo2), ScannerProfile::EnhancedAndBack)
            .unwrap();

        let count = mgr.scan().unwrap();
        assert_eq!(count, 2);
        assert_eq!(mgr.repo_count(), 2);

        // Each stack should have a repo_id matching its source location
        let stacks = mgr.stacks();
        let s1 = stacks.iter().find(|s| s.name == "IMG_001").unwrap();
        let s2 = stacks.iter().find(|s| s.name == "IMG_002").unwrap();
        assert_eq!(s1.repo_id.as_deref(), Some(repo1_loc.as_str()));
        assert_eq!(s2.repo_id.as_deref(), Some(repo2_loc.as_str()));
    }

    // ── c) Overlap rejection ────────────────────────────────────────────

    #[test]
    fn overlapping_repos_rejected() {
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("sub");
        fs::create_dir_all(&sub).unwrap();

        let repo1 = LocalRepository::new(tmp.path());
        let repo2 = LocalRepository::new(&sub);

        let mut mgr = StackManager::new();
        mgr.add_repo(Box::new(repo1), ScannerProfile::EnhancedAndBack)
            .unwrap();

        let result = mgr.add_repo(Box::new(repo2), ScannerProfile::EnhancedAndBack);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, StackManagerError::OverlappingRepo { .. }),
            "Expected OverlappingRepo, got: {err:?}"
        );
    }

    // ── d) Cross-scheme no overlap ──────────────────────────────────────

    #[test]
    fn cross_scheme_does_not_overlap() {
        assert!(!same_scheme("file:///photos", "azure://photos"));
        assert!(same_scheme("file:///a", "file:///b"));
        assert!(!same_scheme("no-scheme", "also-no-scheme"));
    }

    // ── e) Snapshot and staleness ───────────────────────────────────────

    #[test]
    fn snapshot_detects_staleness() {
        let tmp = TempDir::new().unwrap();
        setup_test_dir(&tmp, &["IMG_001.jpg", "IMG_001_a.jpg", "IMG_001_b.jpg"]);

        let repo = LocalRepository::new(tmp.path());
        let mut mgr =
            StackManager::single(Box::new(repo), ScannerProfile::EnhancedAndBack).unwrap();
        mgr.scan().unwrap();

        let snapshot = mgr.snapshot();
        assert_eq!(snapshot.total_count(), 1);

        // Status should be fresh
        let status = mgr.check_status(&snapshot);
        assert!(!status.is_stale);
        assert_eq!(status.snapshot_count, 1);
        assert_eq!(status.current_count, 1);
        assert_eq!(status.added, 0);
        assert_eq!(status.removed, 0);

        // Add a new file and rescan
        let jpeg_data = create_test_jpeg();
        fs::write(tmp.path().join("IMG_002.jpg"), &jpeg_data).unwrap();

        // Re-create repo for rescan (scan clears and re-populates)
        let repo2 = LocalRepository::new(tmp.path());
        let mut mgr2 =
            StackManager::single(Box::new(repo2), ScannerProfile::EnhancedAndBack).unwrap();
        mgr2.scan().unwrap();

        // Old snapshot vs new cache should be stale
        let status = mgr2.check_status(&snapshot);
        assert!(status.is_stale);
        assert_eq!(status.snapshot_count, 1);
        assert_eq!(status.current_count, 2);
        assert_eq!(status.added, 1);
    }

    // ── f) get_stack O(1) ───────────────────────────────────────────────

    #[test]
    fn get_stack_returns_correct_stack() {
        let tmp = TempDir::new().unwrap();
        setup_test_dir(
            &tmp,
            &[
                "IMG_001.jpg",
                "IMG_001_a.jpg",
                "IMG_001_b.jpg",
                "IMG_002.jpg",
                "IMG_002_a.jpg",
            ],
        );

        let repo = LocalRepository::new(tmp.path());
        let mut mgr =
            StackManager::single(Box::new(repo), ScannerProfile::EnhancedAndBack).unwrap();
        mgr.scan().unwrap();
        assert_eq!(mgr.len(), 2);

        // Look up each by ID
        for stack in mgr.stacks() {
            let found = mgr.get_stack(&stack.id).unwrap();
            assert_eq!(found.id, stack.id);
            assert_eq!(found.name, stack.name);
        }

        // Non-existent ID returns None
        assert!(mgr.get_stack("nonexistent").is_none());
    }

    // ── g) Metadata loading ─────────────────────────────────────────────

    #[test]
    fn load_metadata_routes_to_correct_repo() {
        let tmp = TempDir::new().unwrap();
        setup_test_dir(&tmp, &["IMG_001.jpg", "IMG_001_a.jpg", "IMG_001_b.jpg"]);

        let repo = LocalRepository::new(tmp.path());
        let mut mgr =
            StackManager::single(Box::new(repo), ScannerProfile::EnhancedAndBack).unwrap();
        mgr.scan().unwrap();

        let id = mgr.stacks()[0].id.clone();

        // load_metadata should succeed (routing to the LocalRepository)
        mgr.load_metadata(&id).unwrap();

        // After metadata load, the stack should still be accessible
        let stack = mgr.get_stack(&id).unwrap();
        assert_eq!(stack.name, "IMG_001");
    }

    // ── h) Rotation routing ─────────────────────────────────────────────

    #[test]
    fn rotate_stack_routes_to_correct_repo() {
        let tmp = TempDir::new().unwrap();

        // Use the image crate to create decodable JPEGs (rotation needs pixel access)
        use image::{ImageBuffer, Rgb};
        let img: ImageBuffer<Rgb<u8>, Vec<u8>> =
            ImageBuffer::from_fn(100, 100, |_, _| Rgb([128, 128, 128]));
        img.save(tmp.path().join("IMG_001.jpg")).unwrap();
        img.save(tmp.path().join("IMG_001_a.jpg")).unwrap();
        img.save(tmp.path().join("IMG_001_b.jpg")).unwrap();

        let repo = LocalRepository::new(tmp.path());
        let mut mgr =
            StackManager::single(Box::new(repo), ScannerProfile::EnhancedAndBack).unwrap();
        mgr.scan().unwrap();

        let id = mgr.stacks()[0].id.clone();

        // rotate_stack should succeed, returning the updated stack
        let rotated = mgr
            .rotate_stack(&id, Rotation::Cw90, RotationTarget::All)
            .unwrap();
        assert_eq!(rotated.id, id);
        assert_eq!(rotated.name, "IMG_001");

        // The cache should contain the updated stack
        let cached = mgr.get_stack(&id).unwrap();
        assert_eq!(cached.id, id);
    }

    // ── Additional edge-case tests ──────────────────────────────────────

    #[test]
    fn empty_manager() {
        let mgr = StackManager::new();
        assert!(mgr.is_empty());
        assert_eq!(mgr.len(), 0);
        assert_eq!(mgr.repo_count(), 0);
        assert!(mgr.get_stack("any").is_none());
    }

    #[test]
    fn default_creates_empty_manager() {
        let mgr = StackManager::default();
        assert!(mgr.is_empty());
        assert_eq!(mgr.repo_count(), 0);
    }

    #[test]
    fn snapshot_repo_filters_by_repo_id() {
        let tmp1 = TempDir::new().unwrap();
        let tmp2 = TempDir::new().unwrap();
        setup_test_dir(&tmp1, &["IMG_001.jpg"]);
        setup_test_dir(&tmp2, &["IMG_002.jpg"]);

        let repo1 = LocalRepository::new(tmp1.path());
        let repo2 = LocalRepository::new(tmp2.path());
        let repo1_loc = repo1.location().to_string();
        let repo2_loc = repo2.location().to_string();

        let mut mgr = StackManager::new();
        mgr.add_repo(Box::new(repo1), ScannerProfile::EnhancedAndBack)
            .unwrap();
        mgr.add_repo(Box::new(repo2), ScannerProfile::EnhancedAndBack)
            .unwrap();
        mgr.scan().unwrap();

        let snap1 = mgr.snapshot_repo(&repo1_loc);
        let snap2 = mgr.snapshot_repo(&repo2_loc);
        assert_eq!(snap1.total_count(), 1);
        assert_eq!(snap2.total_count(), 1);

        // Full snapshot has both
        let snap_all = mgr.snapshot();
        assert_eq!(snap_all.total_count(), 2);
    }

    #[test]
    fn scan_with_progress_reports_progress() {
        let tmp = TempDir::new().unwrap();
        setup_test_dir(&tmp, &["IMG_001.jpg", "IMG_001_a.jpg", "IMG_001_b.jpg"]);

        let repo = LocalRepository::new(tmp.path());
        let mut mgr =
            StackManager::single(Box::new(repo), ScannerProfile::EnhancedAndBack).unwrap();

        let mut progress_called = false;
        let count = mgr
            .scan_with_progress(Some(&mut |_progress: &ScanProgress| {
                progress_called = true;
            }))
            .unwrap();
        assert_eq!(count, 1);
        assert!(progress_called);
    }

    #[test]
    fn get_stack_mut_allows_mutation() {
        let tmp = TempDir::new().unwrap();
        setup_test_dir(&tmp, &["IMG_001.jpg"]);

        let repo = LocalRepository::new(tmp.path());
        let mut mgr =
            StackManager::single(Box::new(repo), ScannerProfile::EnhancedAndBack).unwrap();
        mgr.scan().unwrap();

        let id = mgr.stacks()[0].id.clone();
        let stack = mgr.get_stack_mut(&id).unwrap();
        stack.name = "RENAMED".to_string();

        let stack = mgr.get_stack(&id).unwrap();
        assert_eq!(stack.name, "RENAMED");
    }

    // ── apply_event tests ───────────────────────────────────────────────

    #[test]
    fn test_apply_file_changed_creates_stack() {
        let mut mgr = StackManager::new();
        let event = StackEvent::FileChanged {
            stack_id: "abc123".to_string(),
            variant: FileVariant::Original,
            path: "/photos/IMG_001.jpg".to_string(),
            size: 1000,
        };
        let result = mgr.apply_event(&event);
        assert_eq!(result, Some(CacheEvent::StackAdded("abc123".to_string())));
        assert!(mgr.get_stack("abc123").is_some());
    }

    #[test]
    fn test_apply_file_changed_updates_existing() {
        let mut mgr = StackManager::new();
        mgr.apply_event(&StackEvent::FileChanged {
            stack_id: "abc123".to_string(),
            variant: FileVariant::Original,
            path: "/photos/IMG_001.jpg".to_string(),
            size: 1000,
        });
        let result = mgr.apply_event(&StackEvent::FileChanged {
            stack_id: "abc123".to_string(),
            variant: FileVariant::Enhanced,
            path: "/photos/IMG_001_a.jpg".to_string(),
            size: 2000,
        });
        assert_eq!(result, Some(CacheEvent::StackUpdated("abc123".to_string())));
        let stack = mgr.get_stack("abc123").unwrap();
        assert!(stack.original.is_some());
        assert!(stack.enhanced.is_some());
    }

    #[test]
    fn test_apply_file_removed_nulls_slot() {
        let mut mgr = StackManager::new();
        mgr.apply_event(&StackEvent::FileChanged {
            stack_id: "abc123".to_string(),
            variant: FileVariant::Original,
            path: "/photos/IMG_001.jpg".to_string(),
            size: 1000,
        });
        mgr.apply_event(&StackEvent::FileChanged {
            stack_id: "abc123".to_string(),
            variant: FileVariant::Enhanced,
            path: "/photos/IMG_001_a.jpg".to_string(),
            size: 2000,
        });
        let result = mgr.apply_event(&StackEvent::FileRemoved {
            stack_id: "abc123".to_string(),
            variant: FileVariant::Original,
        });
        assert_eq!(result, Some(CacheEvent::StackUpdated("abc123".to_string())));
        let stack = mgr.get_stack("abc123").unwrap();
        assert!(stack.original.is_none());
        assert!(stack.enhanced.is_some());
    }

    #[test]
    fn test_apply_file_removed_removes_empty_stack() {
        let mut mgr = StackManager::new();
        mgr.apply_event(&StackEvent::FileChanged {
            stack_id: "abc123".to_string(),
            variant: FileVariant::Original,
            path: "/photos/IMG_001.jpg".to_string(),
            size: 1000,
        });
        let result = mgr.apply_event(&StackEvent::FileRemoved {
            stack_id: "abc123".to_string(),
            variant: FileVariant::Original,
        });
        assert_eq!(result, Some(CacheEvent::StackRemoved("abc123".to_string())));
        assert!(mgr.get_stack("abc123").is_none());
    }

    #[test]
    fn test_apply_file_removed_nonexistent_stack() {
        let mut mgr = StackManager::new();
        let result = mgr.apply_event(&StackEvent::FileRemoved {
            stack_id: "nonexistent".to_string(),
            variant: FileVariant::Original,
        });
        assert_eq!(result, None);
    }

    // ── Coverage-boost tests ────────────────────────────────────────────

    fn create_real_jpeg(dir: &std::path::Path, name: &str) {
        use image::{ImageBuffer, Rgb};
        let img: ImageBuffer<Rgb<u8>, Vec<u8>> =
            ImageBuffer::from_fn(100, 100, |_, _| Rgb([128, 128, 128]));
        img.save(dir.join(name)).unwrap();
    }

    #[test]
    fn scan_with_progress_none_callback() {
        let tmp = TempDir::new().unwrap();
        setup_test_dir(&tmp, &["IMG_001.jpg", "IMG_001_a.jpg"]);

        let repo = LocalRepository::new(tmp.path());
        let mut mgr =
            StackManager::single(Box::new(repo), ScannerProfile::EnhancedAndBack).unwrap();

        let count = mgr.scan_with_progress(None).unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn load_metadata_nonexistent_stack() {
        let tmp = TempDir::new().unwrap();
        setup_test_dir(&tmp, &["IMG_001.jpg"]);

        let repo = LocalRepository::new(tmp.path());
        let mut mgr =
            StackManager::single(Box::new(repo), ScannerProfile::EnhancedAndBack).unwrap();
        mgr.scan().unwrap();

        let result = mgr.load_metadata("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn load_metadata_stack_no_repo_id() {
        let mut mgr = StackManager::new();
        mgr.cache
            .insert("orphan".to_string(), PhotoStack::new("orphan"));

        let result = mgr.load_metadata("orphan");
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("no repo_id"));
    }

    #[test]
    fn load_metadata_repo_id_not_registered() {
        let mut mgr = StackManager::new();
        let mut stack = PhotoStack::new("orphan");
        stack.repo_id = Some("file:///nonexistent".to_string());
        mgr.cache.insert("orphan".to_string(), stack);

        let result = mgr.load_metadata("orphan");
        assert!(result.is_err());
    }

    #[test]
    fn rotate_stack_nonexistent() {
        let tmp = TempDir::new().unwrap();
        setup_test_dir(&tmp, &["IMG_001.jpg"]);

        let repo = LocalRepository::new(tmp.path());
        let mut mgr =
            StackManager::single(Box::new(repo), ScannerProfile::EnhancedAndBack).unwrap();
        mgr.scan().unwrap();

        let result = mgr.rotate_stack("nonexistent", Rotation::Cw90, RotationTarget::All);
        assert!(result.is_err());
    }

    #[test]
    fn rotate_stack_no_repo_id() {
        let mut mgr = StackManager::new();
        mgr.cache
            .insert("orphan".to_string(), PhotoStack::new("orphan"));

        let result = mgr.rotate_stack("orphan", Rotation::Cw90, RotationTarget::All);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("no repo_id"));
    }

    #[test]
    fn rotate_stack_repo_id_not_registered() {
        let mut mgr = StackManager::new();
        let mut stack = PhotoStack::new("orphan");
        stack.repo_id = Some("file:///nonexistent".to_string());
        mgr.cache.insert("orphan".to_string(), stack);

        let result = mgr.rotate_stack("orphan", Rotation::Cw90, RotationTarget::All);
        assert!(result.is_err());
    }

    #[test]
    fn write_metadata_nonexistent_stack() {
        let tmp = TempDir::new().unwrap();
        setup_test_dir(&tmp, &["IMG_001.jpg"]);

        let repo = LocalRepository::new(tmp.path());
        let mgr = StackManager::single(Box::new(repo), ScannerProfile::EnhancedAndBack).unwrap();

        let result = mgr.write_metadata("nonexistent", &Metadata::default());
        assert!(result.is_err());
    }

    #[test]
    fn write_metadata_no_repo_id() {
        let mut mgr = StackManager::new();
        mgr.cache
            .insert("orphan".to_string(), PhotoStack::new("orphan"));

        let result = mgr.write_metadata("orphan", &Metadata::default());
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("no repo_id"));
    }

    #[test]
    fn write_metadata_repo_id_not_registered() {
        let mut mgr = StackManager::new();
        let mut stack = PhotoStack::new("orphan");
        stack.repo_id = Some("file:///nonexistent".to_string());
        mgr.cache.insert("orphan".to_string(), stack);

        let result = mgr.write_metadata("orphan", &Metadata::default());
        assert!(result.is_err());
    }

    #[test]
    fn write_metadata_routes_to_repo() {
        let tmp = TempDir::new().unwrap();
        create_real_jpeg(tmp.path(), "IMG_001.jpg");
        create_real_jpeg(tmp.path(), "IMG_001_a.jpg");
        create_real_jpeg(tmp.path(), "IMG_001_b.jpg");

        let repo = LocalRepository::new(tmp.path());
        let mut mgr =
            StackManager::single(Box::new(repo), ScannerProfile::EnhancedAndBack).unwrap();
        mgr.scan().unwrap();

        let id = mgr.stacks()[0].id.clone();
        let tags = Metadata::default();
        let result = mgr.write_metadata(&id, &tags);
        assert!(result.is_ok());
    }

    #[test]
    fn rotate_stack_front_only() {
        let tmp = TempDir::new().unwrap();
        create_real_jpeg(tmp.path(), "IMG_001.jpg");
        create_real_jpeg(tmp.path(), "IMG_001_a.jpg");
        create_real_jpeg(tmp.path(), "IMG_001_b.jpg");

        let repo = LocalRepository::new(tmp.path());
        let mut mgr =
            StackManager::single(Box::new(repo), ScannerProfile::EnhancedAndBack).unwrap();
        mgr.scan().unwrap();

        let id = mgr.stacks()[0].id.clone();
        let rotated = mgr
            .rotate_stack(&id, Rotation::Cw90, RotationTarget::Front)
            .unwrap();
        assert_eq!(rotated.name, "IMG_001");
    }

    #[test]
    fn rotate_stack_back_only() {
        let tmp = TempDir::new().unwrap();
        create_real_jpeg(tmp.path(), "IMG_001.jpg");
        create_real_jpeg(tmp.path(), "IMG_001_a.jpg");
        create_real_jpeg(tmp.path(), "IMG_001_b.jpg");

        let repo = LocalRepository::new(tmp.path());
        let mut mgr =
            StackManager::single(Box::new(repo), ScannerProfile::EnhancedAndBack).unwrap();
        mgr.scan().unwrap();

        let id = mgr.stacks()[0].id.clone();
        let rotated = mgr
            .rotate_stack(&id, Rotation::Ccw90, RotationTarget::Back)
            .unwrap();
        assert_eq!(rotated.name, "IMG_001");
    }

    #[test]
    fn rotate_stack_180() {
        let tmp = TempDir::new().unwrap();
        create_real_jpeg(tmp.path(), "IMG_001.jpg");
        create_real_jpeg(tmp.path(), "IMG_001_a.jpg");

        let repo = LocalRepository::new(tmp.path());
        let mut mgr =
            StackManager::single(Box::new(repo), ScannerProfile::EnhancedAndBack).unwrap();
        mgr.scan().unwrap();

        let id = mgr.stacks()[0].id.clone();
        let rotated = mgr
            .rotate_stack(&id, Rotation::Cw180, RotationTarget::All)
            .unwrap();
        assert_eq!(rotated.name, "IMG_001");
    }

    #[test]
    fn snapshot_repo_nonexistent_repo() {
        let tmp = TempDir::new().unwrap();
        setup_test_dir(&tmp, &["IMG_001.jpg"]);

        let repo = LocalRepository::new(tmp.path());
        let mut mgr =
            StackManager::single(Box::new(repo), ScannerProfile::EnhancedAndBack).unwrap();
        mgr.scan().unwrap();

        let snap = mgr.snapshot_repo("file:///nonexistent");
        assert_eq!(snap.total_count(), 0);
    }

    #[test]
    fn check_status_after_removal() {
        let tmp = TempDir::new().unwrap();
        setup_test_dir(&tmp, &["IMG_001.jpg", "IMG_002.jpg"]);

        let repo = LocalRepository::new(tmp.path());
        let mut mgr =
            StackManager::single(Box::new(repo), ScannerProfile::EnhancedAndBack).unwrap();
        mgr.scan().unwrap();
        assert_eq!(mgr.len(), 2);

        let snapshot = mgr.snapshot();

        // Remove a file and rescan
        fs::remove_file(tmp.path().join("IMG_002.jpg")).unwrap();
        let repo2 = LocalRepository::new(tmp.path());
        let mut mgr2 =
            StackManager::single(Box::new(repo2), ScannerProfile::EnhancedAndBack).unwrap();
        mgr2.scan().unwrap();

        let status = mgr2.check_status(&snapshot);
        assert!(status.is_stale);
        assert_eq!(status.removed, 1);
        assert_eq!(status.current_count, 1);
    }

    #[test]
    fn read_image_routes_to_repo() {
        let tmp = TempDir::new().unwrap();
        create_real_jpeg(tmp.path(), "IMG_001.jpg");

        let repo = LocalRepository::new(tmp.path());
        let mut mgr =
            StackManager::single(Box::new(repo), ScannerProfile::EnhancedAndBack).unwrap();
        mgr.scan().unwrap();

        let stack = mgr.stacks()[0];
        let path = stack.original.as_ref().unwrap().path.clone();
        let reader = mgr.read_image(&path);
        assert!(reader.is_ok());
    }

    #[test]
    fn read_image_no_repo_can_read() {
        let mgr = StackManager::new();
        let result = mgr.read_image("/nonexistent/IMG_001.jpg");
        assert!(result.is_err());
        let err = result.err().unwrap();
        let err_msg = format!("{}", err);
        assert!(err_msg.contains("No repository can read"));
    }

    #[test]
    fn scan_with_metadata_loads_all() {
        let tmp = TempDir::new().unwrap();
        create_real_jpeg(tmp.path(), "IMG_001.jpg");
        create_real_jpeg(tmp.path(), "IMG_001_a.jpg");
        create_real_jpeg(tmp.path(), "IMG_002.jpg");

        let repo = LocalRepository::new(tmp.path());
        let mut mgr =
            StackManager::single(Box::new(repo), ScannerProfile::EnhancedAndBack).unwrap();

        let count = mgr.scan_with_metadata().unwrap();
        assert_eq!(count, 2);

        // All stacks should be in the cache
        for stack in mgr.stacks() {
            assert!(mgr.get_stack(&stack.id).is_some());
        }
    }

    #[test]
    fn watch_returns_receiver() {
        let tmp = TempDir::new().unwrap();
        setup_test_dir(&tmp, &["IMG_001.jpg"]);

        let repo = LocalRepository::new(tmp.path());
        let mgr = StackManager::single(Box::new(repo), ScannerProfile::EnhancedAndBack).unwrap();

        let rx = mgr.watch();
        assert!(rx.is_ok());
    }

    #[test]
    fn same_scheme_edge_cases() {
        // Both empty
        assert!(!same_scheme("", ""));
        // One has scheme, other doesn't
        assert!(!same_scheme("file:///a", "no-scheme"));
        // Same scheme
        assert!(same_scheme("azure://a/b", "azure://c/d"));
        // Different schemes
        assert!(!same_scheme("file:///a", "http:///b"));
        // No :// — split("://").next() returns full string, both equal → true
        assert!(same_scheme("plain", "plain"));
        // No :// — different strings → false
        assert!(!same_scheme("abc", "def"));
    }

    #[test]
    fn apply_event_back_variant() {
        let mut mgr = StackManager::new();
        let event = StackEvent::FileChanged {
            stack_id: "test_stack".to_string(),
            variant: FileVariant::Back,
            path: "/photos/IMG_001_b.jpg".to_string(),
            size: 500,
        };
        let result = mgr.apply_event(&event);
        assert_eq!(
            result,
            Some(CacheEvent::StackAdded("test_stack".to_string()))
        );
        let stack = mgr.get_stack("test_stack").unwrap();
        assert!(stack.back.is_some());
        assert!(stack.original.is_none());
        assert!(stack.enhanced.is_none());
    }

    #[test]
    fn apply_event_remove_enhanced_slot() {
        let mut mgr = StackManager::new();
        mgr.apply_event(&StackEvent::FileChanged {
            stack_id: "s1".to_string(),
            variant: FileVariant::Original,
            path: "/p/o.jpg".to_string(),
            size: 100,
        });
        mgr.apply_event(&StackEvent::FileChanged {
            stack_id: "s1".to_string(),
            variant: FileVariant::Enhanced,
            path: "/p/e.jpg".to_string(),
            size: 200,
        });
        mgr.apply_event(&StackEvent::FileChanged {
            stack_id: "s1".to_string(),
            variant: FileVariant::Back,
            path: "/p/b.jpg".to_string(),
            size: 300,
        });

        // Remove enhanced
        let result = mgr.apply_event(&StackEvent::FileRemoved {
            stack_id: "s1".to_string(),
            variant: FileVariant::Enhanced,
        });
        assert_eq!(result, Some(CacheEvent::StackUpdated("s1".to_string())));
        let stack = mgr.get_stack("s1").unwrap();
        assert!(stack.enhanced.is_none());
        assert!(stack.original.is_some());
        assert!(stack.back.is_some());

        // Remove back
        let result = mgr.apply_event(&StackEvent::FileRemoved {
            stack_id: "s1".to_string(),
            variant: FileVariant::Back,
        });
        assert_eq!(result, Some(CacheEvent::StackUpdated("s1".to_string())));
        let stack = mgr.get_stack("s1").unwrap();
        assert!(stack.back.is_none());
    }

    #[test]
    fn error_display_overlapping_repo() {
        let err = StackManagerError::OverlappingRepo {
            new: "file:///a".to_string(),
            existing: "file:///b".to_string(),
        };
        let display = format!("{}", err);
        assert!(display.contains("overlap"));
    }

    #[test]
    fn error_display_repo_not_found() {
        let err = StackManagerError::RepoNotFound("missing".to_string());
        let display = format!("{}", err);
        assert!(display.contains("missing"));
    }

    #[test]
    fn error_from_repository_error() {
        let repo_err = RepositoryError::NotFound("stack1".to_string());
        let mgr_err: StackManagerError = repo_err.into();
        let display = format!("{}", mgr_err);
        assert!(display.contains("stack1"));
    }

    // ── query() tests ───────────────────────────────────────────────────

    #[test]
    fn query_all_no_pagination() {
        let tmp = TempDir::new().unwrap();
        setup_test_dir(&tmp, &["IMG_001.jpg", "IMG_002.jpg", "IMG_003.jpg"]);

        let repo = LocalRepository::new(tmp.path());
        let mut mgr =
            StackManager::single(Box::new(repo), ScannerProfile::EnhancedAndBack).unwrap();
        mgr.scan().unwrap();

        let result = mgr.query(&SearchQuery::new(), None);
        assert_eq!(result.total_count, 3);
        assert_eq!(result.items.len(), 3);
        assert!(!result.has_more);
    }

    #[test]
    fn query_with_pagination() {
        let tmp = TempDir::new().unwrap();
        setup_test_dir(
            &tmp,
            &[
                "IMG_001.jpg",
                "IMG_002.jpg",
                "IMG_003.jpg",
                "IMG_004.jpg",
                "IMG_005.jpg",
            ],
        );

        let repo = LocalRepository::new(tmp.path());
        let mut mgr =
            StackManager::single(Box::new(repo), ScannerProfile::EnhancedAndBack).unwrap();
        mgr.scan().unwrap();

        let page1 = mgr.query(
            &SearchQuery::new(),
            Some(&PaginationParams {
                offset: 0,
                limit: 2,
            }),
        );
        assert_eq!(page1.items.len(), 2);
        assert_eq!(page1.total_count, 5);
        assert!(page1.has_more);

        let next = page1.next_page().unwrap();
        assert_eq!(next.offset, 2);
        assert_eq!(next.limit, 2);

        let page2 = mgr.query(&SearchQuery::new(), Some(&next));
        assert_eq!(page2.items.len(), 2);
        assert!(page2.has_more);

        let page3 = mgr.query(&SearchQuery::new(), Some(&page2.next_page().unwrap()));
        assert_eq!(page3.items.len(), 1);
        assert!(!page3.has_more);
        assert!(page3.next_page().is_none());
    }

    #[test]
    fn query_with_filter() {
        let tmp = TempDir::new().unwrap();
        setup_test_dir(&tmp, &["IMG_001.jpg", "IMG_001_b.jpg", "IMG_002.jpg"]);

        let repo = LocalRepository::new(tmp.path());
        let mut mgr =
            StackManager::single(Box::new(repo), ScannerProfile::EnhancedAndBack).unwrap();
        mgr.scan().unwrap();

        let query = SearchQuery::new().with_has_back(true);
        let result = mgr.query(&query, None);
        assert_eq!(result.total_count, 1);
        assert_eq!(result.items[0].name, "IMG_001");
    }

    #[test]
    fn query_with_filter_and_pagination() {
        let tmp = TempDir::new().unwrap();
        setup_test_dir(
            &tmp,
            &[
                "A_001.jpg",
                "A_001_b.jpg",
                "A_002.jpg",
                "A_002_b.jpg",
                "A_003.jpg",
                "A_003_b.jpg",
                "A_004.jpg",
            ],
        );

        let repo = LocalRepository::new(tmp.path());
        let mut mgr =
            StackManager::single(Box::new(repo), ScannerProfile::EnhancedAndBack).unwrap();
        mgr.scan().unwrap();

        let query = SearchQuery::new().with_has_back(true);
        let page = mgr.query(
            &query,
            Some(&PaginationParams {
                offset: 0,
                limit: 2,
            }),
        );
        assert_eq!(page.items.len(), 2);
        assert_eq!(page.total_count, 3);
        assert!(page.has_more);
    }

    #[test]
    fn query_empty_cache() {
        let mgr = StackManager::new();
        let result = mgr.query(&SearchQuery::new(), None);
        assert_eq!(result.total_count, 0);
        assert!(result.items.is_empty());
        assert!(!result.has_more);
    }
}

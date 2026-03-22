//! Unified cache manager for multiple photo repositories.
//!
//! [`StackManager`] sits above the [`Repository`] trait and provides:
//! - A flat `HashMap<String, PhotoStack>` cache keyed by opaque stack ID
//! - Multi-repo support with URI-based overlap detection
//! - `query()` as the sole entry point for cache lookup and filtering
//! - Snapshot creation (frozen cache clones)
//! - Classifier injection into repositories at registration time

use std::collections::HashMap;
use std::sync::{mpsc as std_mpsc, Arc, Mutex};

use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::backends::local_handles::LocalImageHandle;
use crate::classifier::{DefaultClassifier, ImageClassifier};
use crate::events::{CacheEvent, FileVariant, StackEvent};
use crate::image_handle::ImageRef;
use crate::photo_stack::{PhotoStack, ScanProgress, ScannerProfile};
use crate::query_result::QueryResult;
use crate::repository::{Repository, RepositoryError};
use crate::search::SearchQuery;
use crate::snapshot::ScanSnapshot;

/// Type alias: `SessionManager` is the new name for `StackManager`.
pub type SessionManager = StackManager;

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
/// Per-stack I/O (read, rotate, write metadata) is handled via
/// the handles embedded in each [`PhotoStack`].
pub struct StackManager {
    /// Keyed by `repo.location()` since the scanner stores location in `PhotoStack::repo_id`.
    repos: HashMap<String, RegisteredRepo>,
    cache: HashMap<String, PhotoStack>,
    /// Image classifier shared with all registered repos.
    classifier: Arc<dyn ImageClassifier>,
    /// Broadcast sender for cache events (reactive notifications).
    event_tx: broadcast::Sender<CacheEvent>,
    /// Receives StackEvents queued by the watcher threads. Wrapped in
    /// Arc<Mutex> so watch() (which takes &self) can hand clones to threads.
    pending_events_rx: Arc<Mutex<std_mpsc::Receiver<StackEvent>>>,
    /// Sender half; cloned into each watcher thread.
    pending_events_tx: std_mpsc::Sender<StackEvent>,
}

impl StackManager {
    /// Create an empty `StackManager` with no repositories.
    pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(256);
        let (pending_events_tx, pending_events_rx) = std_mpsc::channel();
        Self {
            repos: HashMap::new(),
            cache: HashMap::new(),
            classifier: Arc::new(DefaultClassifier),
            event_tx,
            pending_events_rx: Arc::new(Mutex::new(pending_events_rx)),
            pending_events_tx,
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
    ///
    /// Injects the session's classifier into the repository at registration time.
    pub fn add_repo(
        &mut self,
        mut repo: Box<dyn Repository>,
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

        repo.set_classifier(self.classifier.clone());

        let location = repo.location().to_string();
        self.repos
            .insert(location.clone(), RegisteredRepo { repo, profile });
        Ok(self.repos.get(&location).unwrap().repo.id())
    }

    /// Remove a repository by its location URI.
    ///
    /// Removes the repository and all its stacks from the cache.
    /// Broadcasts [`CacheEvent::StackRemoved`] for each evicted stack.
    ///
    /// # Errors
    ///
    /// Returns [`StackManagerError::RepoNotFound`] if no repository
    /// matches the given location.
    pub fn remove_repo(&mut self, location: &str) -> Result<(), StackManagerError> {
        if self.repos.remove(location).is_none() {
            return Err(StackManagerError::RepoNotFound(location.to_string()));
        }

        // Evict stacks belonging to this repo
        let to_remove: Vec<String> = self
            .cache
            .iter()
            .filter(|(_, stack)| stack.repo_id().as_deref() == Some(location))
            .map(|(id, _)| id.clone())
            .collect();

        for id in to_remove {
            self.cache.remove(&id);
            let _ = self.event_tx.send(CacheEvent::StackRemoved(id));
        }

        Ok(())
    }

    /// Scan all registered repos and populate the cache.
    ///
    /// When there are multiple repos and no progress callback, repos are
    /// scanned in parallel using OS threads. With a progress callback
    /// (which is `&mut` and not `Send`), scanning falls back to sequential.
    fn scan_repos(
        &mut self,
        progress: Option<&mut dyn FnMut(&ScanProgress)>,
        cancel: Option<&CancellationToken>,
    ) -> Result<usize, StackManagerError> {
        self.cache.clear();

        let all_stacks = if progress.is_some() || self.repos.len() <= 1 {
            // Sequential path: required when progress callback is present
            // (FnMut is not Send) or there's only one repo.
            let mut stacks = Vec::new();
            if let Some(cb) = progress {
                for reg in self.repos.values() {
                    if let Some(token) = cancel {
                        if token.is_cancelled() {
                            return Err(StackManagerError::Repository(RepositoryError::Cancelled));
                        }
                    }
                    let s = reg.repo.scan_with_progress(reg.profile, Some(&mut *cb))?;
                    stacks.extend(s);
                }
            } else {
                for reg in self.repos.values() {
                    if let Some(token) = cancel {
                        if token.is_cancelled() {
                            return Err(StackManagerError::Repository(RepositoryError::Cancelled));
                        }
                    }
                    let s = reg.repo.scan_with_progress(reg.profile, None)?;
                    stacks.extend(s);
                }
            }
            stacks
        } else {
            // Parallel path: scan all repos on separate threads.
            let repos: Vec<&RegisteredRepo> = self.repos.values().collect();
            let cancel_ref = cancel;

            std::thread::scope(|scope| {
                let handles: Vec<_> = repos
                    .iter()
                    .map(|reg| {
                        scope.spawn(move || {
                            if let Some(token) = cancel_ref {
                                if token.is_cancelled() {
                                    return Err(StackManagerError::Repository(
                                        RepositoryError::Cancelled,
                                    ));
                                }
                            }
                            reg.repo
                                .scan_with_progress(reg.profile, None)
                                .map_err(StackManagerError::from)
                        })
                    })
                    .collect();

                let mut stacks = Vec::new();
                for h in handles {
                    stacks.extend(h.join().expect("scan thread panicked")?);
                }
                Ok::<Vec<PhotoStack>, StackManagerError>(stacks)
            })?
        };

        for stack in all_stacks {
            let id = stack.id();
            self.cache.insert(id.clone(), stack);
            let _ = self.event_tx.send(CacheEvent::StackAdded(id));
        }
        Ok(self.cache.len())
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

    /// Query the cache with optional filtering, returning a [`QueryResult`].
    ///
    /// This is the **primary entry point** for retrieving stacks. If the
    /// cache is empty (no scan has been performed yet), it automatically
    /// scans all registered repos first. Pass a progress callback to
    /// receive scan progress notifications.
    ///
    /// - `query: None` = scan + return all stacks
    /// - `query: Some(&SearchQuery)` = scan (if needed) + filter + return matching
    /// - `page_size: None` = single page with all results (`usize::MAX`)
    /// - `page_size: Some(n)` = paginate into pages of `n` stacks
    /// - `cancel: None` = no cancellation support
    /// - `cancel: Some(token)` = check token before scanning each repo
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use photostax_core::stack_manager::StackManager;
    /// # use photostax_core::search::SearchQuery;
    /// # let mut mgr = StackManager::new();
    /// // All stacks (auto-scans on first call)
    /// let result = mgr.query(None, None, None, None).unwrap();
    ///
    /// // Filtered + paginated + progress feedback
    /// let query = SearchQuery::new().with_has_back(true);
    /// let mut result = mgr.query(Some(&query), Some(20), Some(&mut |p| {
    ///     println!("Phase {:?}: {}/{}", p.phase, p.current, p.total);
    /// }), None).unwrap();
    /// for stack in result.current_page() {
    ///     println!("{}", stack.name());
    /// }
    /// ```
    pub fn query(
        &mut self,
        query: Option<&SearchQuery>,
        page_size: Option<usize>,
        progress: Option<&mut dyn FnMut(&ScanProgress)>,
        cancel: Option<CancellationToken>,
    ) -> Result<QueryResult, StackManagerError> {
        if let Some(ref token) = cancel {
            if token.is_cancelled() {
                return Err(StackManagerError::Repository(RepositoryError::Cancelled));
            }
        }

        // Auto-scan if cache is empty and we have repos
        if self.cache.is_empty() && !self.repos.is_empty() {
            self.scan_repos(progress, cancel.as_ref())?;
        }

        let default_query = SearchQuery::new();
        let q = query.unwrap_or(&default_query);

        use crate::search::matches_query_ref;
        let filtered: Vec<PhotoStack> = self
            .cache
            .values()
            .filter(|stack| matches_query_ref(stack, q))
            .cloned()
            .collect();

        let event_rx = self.event_tx.subscribe();
        Ok(QueryResult::new_with_events(
            ScanSnapshot::from_stacks(filtered),
            page_size.unwrap_or(usize::MAX),
            event_rx,
        ))
    }

    /// Clear the cache, forcing the next [`query()`](Self::query) call to
    /// re-scan all registered repositories.
    ///
    /// Use this when you know the underlying files have changed and want
    /// fresh data on the next query.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use photostax_core::stack_manager::StackManager;
    /// # let mut mgr = StackManager::new();
    /// mgr.invalidate_cache();
    /// let result = mgr.query(None, None, None, None).unwrap(); // re-scans
    /// ```
    pub fn invalidate_cache(&mut self) {
        self.cache.clear();
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

    /// Update the scanner profile for all registered repositories.
    ///
    /// The new profile takes effect on the next scan (via
    /// [`invalidate_cache`](Self::invalidate_cache) followed by
    /// [`query`](Self::query)).
    pub fn set_profile(&mut self, profile: ScannerProfile) {
        for reg in self.repos.values_mut() {
            reg.profile = profile;
        }
    }

    /// Subscribe to cache-level events (stack added/updated/removed).
    ///
    /// Returns a broadcast receiver for [`CacheEvent`]s. Drop it to unsubscribe.
    /// Multiple subscribers can listen concurrently.
    pub fn subscribe_cache_events(&self) -> broadcast::Receiver<CacheEvent> {
        self.event_tx.subscribe()
    }

    /// Start watching all registered repos for changes.
    ///
    /// Spawns a background thread per repository that forwards
    /// [`StackEvent`]s into an internal queue. Call
    /// [`apply_pending_events()`](Self::apply_pending_events) to drain
    /// the queue, update the cache, and broadcast correct [`CacheEvent`]s.
    ///
    /// Also returns a broadcast receiver so callers can subscribe to
    /// the resulting `CacheEvent`s without polling.
    pub fn watch(&self) -> Result<broadcast::Receiver<CacheEvent>, StackManagerError> {
        let rx = self.event_tx.subscribe();

        for reg in self.repos.values() {
            let repo_rx = reg.repo.watch()?;
            let tx = self.pending_events_tx.clone();

            std::thread::spawn(move || {
                for event in repo_rx {
                    if tx.send(event).is_err() {
                        return; // receiver dropped
                    }
                }
            });
        }

        Ok(rx)
    }

    /// Drain pending filesystem events, update the cache, and broadcast
    /// the resulting [`CacheEvent`]s.
    ///
    /// Returns the list of cache events that were applied. This should be
    /// called periodically (e.g. before rendering a UI frame) or in
    /// response to a wakeup from [`watch()`](Self::watch).
    pub fn apply_pending_events(&mut self) -> Vec<CacheEvent> {
        let events: Vec<StackEvent> = {
            let rx = self.pending_events_rx.lock().unwrap();
            rx.try_iter().collect()
        };

        let mut cache_events = Vec::new();
        for event in &events {
            if let Some(ce) = self.apply_event(event) {
                cache_events.push(ce);
            }
        }
        cache_events
    }

    /// Process a single StackEvent, updating the cache.
    /// Returns the CacheEvent that resulted, if any.
    /// Also broadcasts the event to all subscribers.
    pub fn apply_event(&mut self, event: &StackEvent) -> Option<CacheEvent> {
        let cache_event = match event {
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

                let handle = Arc::new(LocalImageHandle::new(path, *size));
                let image_ref = ImageRef::new(handle);
                {
                    let mut inner = stack.inner.write().unwrap();
                    match variant {
                        FileVariant::Original => inner.original = image_ref,
                        FileVariant::Enhanced => inner.enhanced = image_ref,
                        FileVariant::Back => inner.back = image_ref,
                    }
                }

                if is_new {
                    Some(CacheEvent::StackAdded(stack_id.clone()))
                } else {
                    Some(CacheEvent::StackUpdated(stack_id.clone()))
                }
            }
            StackEvent::FileRemoved { stack_id, variant } => {
                if let Some(stack) = self.cache.get_mut(stack_id) {
                    {
                        let mut inner = stack.inner.write().unwrap();
                        match variant {
                            FileVariant::Original => inner.original = ImageRef::absent(),
                            FileVariant::Enhanced => inner.enhanced = ImageRef::absent(),
                            FileVariant::Back => inner.back = ImageRef::absent(),
                        }
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
        };

        if let Some(ref evt) = cache_event {
            let _ = self.event_tx.send(evt.clone());
        }

        cache_event
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
mod tests {
    use super::*;
    use crate::backends::local::LocalRepository;
    use crate::repository::Repository;
    use crate::search::SearchQuery;
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
    fn single_repo_scan_and_query() {
        let tmp = TempDir::new().unwrap();
        setup_test_dir(&tmp, &["IMG_001.jpg", "IMG_001_a.jpg", "IMG_001_b.jpg"]);

        let repo = LocalRepository::new(tmp.path());
        let mut mgr =
            StackManager::single(Box::new(repo), ScannerProfile::EnhancedAndBack).unwrap();

        let result = mgr.query(None, None, None, None).unwrap();
        assert_eq!(result.total_count(), 1);
        assert_eq!(mgr.len(), 1);
        assert!(!mgr.is_empty());
        assert_eq!(mgr.repo_count(), 1);

        let stacks = result.all_stacks();
        assert_eq!(stacks.len(), 1);
        assert_eq!(stacks[0].name(), "IMG_001");
        assert!(stacks[0].original().is_present());
        assert!(stacks[0].enhanced().is_present());
        assert!(stacks[0].back().is_present());
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

        let result = mgr.query(None, None, None, None).unwrap();
        assert_eq!(result.total_count(), 2);
        assert_eq!(mgr.repo_count(), 2);

        // Each stack should have a repo_id matching its source location
        let stacks = result.all_stacks();
        let s1 = stacks.iter().find(|s| s.name() == "IMG_001").unwrap();
        let s2 = stacks.iter().find(|s| s.name() == "IMG_002").unwrap();
        assert_eq!(s1.repo_id().as_deref(), Some(repo1_loc.as_str()));
        assert_eq!(s2.repo_id().as_deref(), Some(repo2_loc.as_str()));
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

        let result = mgr.query(None, None, None, None).unwrap();
        let snapshot = result.into_snapshot();
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
        let _ = mgr2.query(None, None, None, None).unwrap();

        // Old snapshot vs new cache should be stale
        let status = mgr2.check_status(&snapshot);
        assert!(status.is_stale);
        assert_eq!(status.snapshot_count, 1);
        assert_eq!(status.current_count, 2);
        assert_eq!(status.added, 1);
    }

    // ── f) Query by ID ────────────────────────────────────────────────

    #[test]
    fn query_by_id_returns_correct_stack() {
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

        let result = mgr.query(None, None, None, None).unwrap();
        assert_eq!(mgr.len(), 2);

        // Look up each by ID via query
        for stack in result.all_stacks() {
            let id_query = SearchQuery::new().with_ids(vec![stack.id()]);
            let found = mgr.query(Some(&id_query), None, None, None).unwrap();
            assert_eq!(found.total_count(), 1);
            assert_eq!(found.all_stacks()[0].id(), stack.id());
            assert_eq!(found.all_stacks()[0].name(), stack.name());
        }

        // Non-existent ID returns empty
        let missing = SearchQuery::new().with_ids(vec!["nonexistent".to_string()]);
        let result = mgr.query(Some(&missing), None, None, None).unwrap();
        assert_eq!(result.total_count(), 0);
    }

    // ── g) Metadata loading via handles ──────────────────────────────────

    #[test]
    fn metadata_load_via_handle() {
        let tmp = TempDir::new().unwrap();
        setup_test_dir(&tmp, &["IMG_001.jpg", "IMG_001_a.jpg", "IMG_001_b.jpg"]);

        let repo = LocalRepository::new(tmp.path());
        let mut mgr =
            StackManager::single(Box::new(repo), ScannerProfile::EnhancedAndBack).unwrap();

        let result = mgr.query(None, None, None, None).unwrap();
        let stack = &result.all_stacks()[0];

        // Metadata is loaded via the handle (Arc<RwLock> — no mutable manager access needed)
        let _meta = stack.metadata().read().unwrap();

        // After metadata load, the stack should still be accessible
        assert_eq!(stack.name(), "IMG_001");
    }

    // ── h) Rotation via handles ─────────────────────────────────────────

    #[test]
    fn rotate_via_handles() {
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

        let result = mgr.query(None, None, None, None).unwrap();

        // Rotation via ImageRef handle — use stack from query result directly
        let stack = &result.all_stacks()[0];
        stack
            .original()
            .rotate(crate::photo_stack::Rotation::Cw90)
            .unwrap();
        assert_eq!(stack.name(), "IMG_001");
    }

    // ── Additional edge-case tests ──────────────────────────────────────

    #[test]
    fn empty_manager() {
        let mut mgr = StackManager::new();
        assert!(mgr.is_empty());
        assert_eq!(mgr.len(), 0);
        assert_eq!(mgr.repo_count(), 0);
        let result = mgr.query(None, None, None, None).unwrap();
        assert_eq!(result.total_count(), 0);
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

        // Query with repo_id filter for each repo
        let query1 = SearchQuery::new().with_repo_id(&repo1_loc);
        let snap1 = mgr.query(Some(&query1), None, None, None).unwrap();
        let query2 = SearchQuery::new().with_repo_id(&repo2_loc);
        let snap2 = mgr.query(Some(&query2), None, None, None).unwrap();
        assert_eq!(snap1.total_count(), 1);
        assert_eq!(snap2.total_count(), 1);

        // Full query has both
        let snap_all = mgr.query(None, None, None, None).unwrap();
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
        let result = mgr
            .query(
                None,
                None,
                Some(&mut |_progress: &ScanProgress| {
                    progress_called = true;
                }),
                None,
            )
            .unwrap();
        assert_eq!(result.total_count(), 1);
        assert!(progress_called);
    }

    #[test]
    fn arc_shared_mutation_visible_across_clones() {
        let tmp = TempDir::new().unwrap();
        setup_test_dir(&tmp, &["IMG_001.jpg"]);

        let repo = LocalRepository::new(tmp.path());
        let mut mgr =
            StackManager::single(Box::new(repo), ScannerProfile::EnhancedAndBack).unwrap();

        let result = mgr.query(None, None, None, None).unwrap();
        let stack = result.all_stacks()[0].clone(); // Arc clone — same underlying data

        // Mutate through the Arc<RwLock> inner
        stack.inner.write().unwrap().name = "RENAMED".to_string();

        // Visible through a fresh query (same Arc in cache)
        let result2 = mgr.query(None, None, None, None).unwrap();
        assert_eq!(result2.all_stacks()[0].name(), "RENAMED");
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
        let found = mgr
            .query(
                Some(&SearchQuery::new().with_ids(vec!["abc123".to_string()])),
                None,
                None,
                None,
            )
            .unwrap();
        assert_eq!(found.total_count(), 1);
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
        let found = mgr
            .query(
                Some(&SearchQuery::new().with_ids(vec!["abc123".to_string()])),
                None,
                None,
                None,
            )
            .unwrap();
        let stack = &found.all_stacks()[0];
        assert!(stack.original().is_present());
        assert!(stack.enhanced().is_present());
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
        let found = mgr
            .query(
                Some(&SearchQuery::new().with_ids(vec!["abc123".to_string()])),
                None,
                None,
                None,
            )
            .unwrap();
        let stack = &found.all_stacks()[0];
        assert!(!stack.original().is_present());
        assert!(stack.enhanced().is_present());
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
        let found = mgr
            .query(
                Some(&SearchQuery::new().with_ids(vec!["abc123".to_string()])),
                None,
                None,
                None,
            )
            .unwrap();
        assert_eq!(found.total_count(), 0);
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

        let result = mgr.query(None, None, None, None).unwrap();
        assert_eq!(result.total_count(), 1);
    }

    #[test]
    fn snapshot_repo_nonexistent_repo() {
        let tmp = TempDir::new().unwrap();
        setup_test_dir(&tmp, &["IMG_001.jpg"]);

        let repo = LocalRepository::new(tmp.path());
        let mut mgr =
            StackManager::single(Box::new(repo), ScannerProfile::EnhancedAndBack).unwrap();
        mgr.query(None, None, None, None).unwrap();

        let query = SearchQuery::new().with_repo_id("file:///nonexistent");
        let snap = mgr.query(Some(&query), None, None, None).unwrap();
        assert_eq!(snap.total_count(), 0);
    }

    #[test]
    fn check_status_after_removal() {
        let tmp = TempDir::new().unwrap();
        setup_test_dir(&tmp, &["IMG_001.jpg", "IMG_002.jpg"]);

        let repo = LocalRepository::new(tmp.path());
        let mut mgr =
            StackManager::single(Box::new(repo), ScannerProfile::EnhancedAndBack).unwrap();

        let result = mgr.query(None, None, None, None).unwrap();
        assert_eq!(mgr.len(), 2);
        let snapshot = result.into_snapshot();

        // Remove a file and rescan
        fs::remove_file(tmp.path().join("IMG_002.jpg")).unwrap();
        let repo2 = LocalRepository::new(tmp.path());
        let mut mgr2 =
            StackManager::single(Box::new(repo2), ScannerProfile::EnhancedAndBack).unwrap();
        let _ = mgr2.query(None, None, None, None).unwrap();

        let status = mgr2.check_status(&snapshot);
        assert!(status.is_stale);
        assert_eq!(status.removed, 1);
        assert_eq!(status.current_count, 1);
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

        let result = mgr.query(None, None, None, None).unwrap();
        assert_eq!(result.total_count(), 2);

        // Load metadata directly through Arc stacks (no manager mutation needed)
        for stack in result.all_stacks() {
            let _ = stack.metadata().read();
        }

        // All stacks should still be accessible via a new query
        let result2 = mgr.query(None, None, None, None).unwrap();
        assert_eq!(result2.total_count(), 2);
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
        let stack = mgr
            .query(
                Some(&SearchQuery::new().with_ids(vec!["test_stack".to_string()])),
                None,
                None,
                None,
            )
            .unwrap();
        let s = &stack.all_stacks()[0];
        assert!(s.back().is_present());
        assert!(!s.original().is_present());
        assert!(!s.enhanced().is_present());
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
        let found = mgr
            .query(
                Some(&SearchQuery::new().with_ids(vec!["s1".to_string()])),
                None,
                None,
                None,
            )
            .unwrap();
        let stack = &found.all_stacks()[0];
        assert!(!stack.enhanced().is_present());
        assert!(stack.original().is_present());
        assert!(stack.back().is_present());

        // Remove back
        let result = mgr.apply_event(&StackEvent::FileRemoved {
            stack_id: "s1".to_string(),
            variant: FileVariant::Back,
        });
        assert_eq!(result, Some(CacheEvent::StackUpdated("s1".to_string())));
        let found = mgr
            .query(
                Some(&SearchQuery::new().with_ids(vec!["s1".to_string()])),
                None,
                None,
                None,
            )
            .unwrap();
        let stack = &found.all_stacks()[0];
        assert!(!stack.back().is_present());
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
    fn query_all() {
        let tmp = TempDir::new().unwrap();
        setup_test_dir(&tmp, &["IMG_001.jpg", "IMG_002.jpg", "IMG_003.jpg"]);

        let repo = LocalRepository::new(tmp.path());
        let mut mgr =
            StackManager::single(Box::new(repo), ScannerProfile::EnhancedAndBack).unwrap();

        let snap = mgr
            .query(Some(&SearchQuery::new()), None, None, None)
            .unwrap();
        assert_eq!(snap.total_count(), 3);
        assert_eq!(snap.all_stacks().len(), 3);
    }

    #[test]
    fn query_with_pagination_via_snapshot() {
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

        let snap = mgr
            .query(Some(&SearchQuery::new()), None, None, None)
            .unwrap();
        assert_eq!(snap.total_count(), 5);

        let page1 = snap.snapshot().get_page(0, 2);
        assert_eq!(page1.items.len(), 2);
        assert_eq!(page1.total_count, 5);
        assert!(page1.has_more);

        let page2 = snap.snapshot().get_page(2, 2);
        assert_eq!(page2.items.len(), 2);
        assert!(page2.has_more);

        let page3 = snap.snapshot().get_page(4, 2);
        assert_eq!(page3.items.len(), 1);
        assert!(!page3.has_more);
    }

    #[test]
    fn query_with_filter() {
        let tmp = TempDir::new().unwrap();
        setup_test_dir(&tmp, &["IMG_001.jpg", "IMG_001_b.jpg", "IMG_002.jpg"]);

        let repo = LocalRepository::new(tmp.path());
        let mut mgr =
            StackManager::single(Box::new(repo), ScannerProfile::EnhancedAndBack).unwrap();

        let query = SearchQuery::new().with_has_back(true);
        let snap = mgr.query(Some(&query), None, None, None).unwrap();
        assert_eq!(snap.total_count(), 1);
        assert_eq!(snap.all_stacks()[0].name(), "IMG_001");
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

        let query = SearchQuery::new().with_has_back(true);
        let snap = mgr.query(Some(&query), None, None, None).unwrap();
        assert_eq!(snap.total_count(), 3);

        let page = snap.snapshot().get_page(0, 2);
        assert_eq!(page.items.len(), 2);
        assert_eq!(page.total_count, 3);
        assert!(page.has_more);
    }

    #[test]
    fn query_empty_cache() {
        let mut mgr = StackManager::new();
        let snap = mgr
            .query(Some(&SearchQuery::new()), None, None, None)
            .unwrap();
        assert_eq!(snap.total_count(), 0);
        assert!(snap.all_stacks().is_empty());
    }

    #[test]
    fn test_query_auto_scans_on_first_call() {
        let tmp = TempDir::new().unwrap();
        setup_test_dir(&tmp, &["IMG_001.jpg", "IMG_002.jpg"]);

        let repo = LocalRepository::new(tmp.path());
        let mut mgr =
            StackManager::single(Box::new(repo), ScannerProfile::EnhancedAndBack).unwrap();

        // Do NOT call rescan(); query should auto-scan
        let snap = mgr.query(None, None, None, None).unwrap();
        assert!(snap.total_count() > 0);
        assert_eq!(snap.total_count(), 2);
    }

    #[test]
    fn test_query_with_text_filter() {
        let tmp = TempDir::new().unwrap();
        setup_test_dir(&tmp, &["IMG_001.jpg", "IMG_002.jpg", "IMG_003.jpg"]);

        let repo = LocalRepository::new(tmp.path());
        let mut mgr =
            StackManager::single(Box::new(repo), ScannerProfile::EnhancedAndBack).unwrap();

        let query = SearchQuery::new().with_text("IMG_001");
        let snap = mgr.query(Some(&query), None, None, None).unwrap();
        assert_eq!(snap.total_count(), 1);
        assert_eq!(snap.all_stacks()[0].name(), "IMG_001");
    }

    #[test]
    fn test_query_with_progress() {
        let tmp = TempDir::new().unwrap();
        setup_test_dir(&tmp, &["IMG_001.jpg", "IMG_002.jpg"]);

        let repo = LocalRepository::new(tmp.path());
        let mut mgr =
            StackManager::single(Box::new(repo), ScannerProfile::EnhancedAndBack).unwrap();

        let mut call_count = 0usize;
        let snap = mgr
            .query(
                None,
                None,
                Some(&mut |_p: &ScanProgress| {
                    call_count += 1;
                }),
                None,
            )
            .unwrap();
        assert!(snap.total_count() > 0);
        assert!(call_count > 0);
    }

    #[test]
    fn test_invalidate_and_requery() {
        let tmp = TempDir::new().unwrap();
        setup_test_dir(&tmp, &["IMG_001.jpg", "IMG_002.jpg"]);

        let repo = LocalRepository::new(tmp.path());
        let mut mgr =
            StackManager::single(Box::new(repo), ScannerProfile::EnhancedAndBack).unwrap();

        let result = mgr.query(None, None, None, None).unwrap();
        assert_eq!(result.total_count(), 2);
        assert_eq!(mgr.len(), 2);

        // Invalidate and re-query to repopulate
        mgr.invalidate_cache();
        let result = mgr.query(None, None, None, None).unwrap();
        assert_eq!(result.total_count(), 2);
        assert_eq!(mgr.len(), 2);
    }

    #[test]
    fn test_is_empty_and_len() {
        let tmp = TempDir::new().unwrap();
        setup_test_dir(&tmp, &["IMG_001.jpg"]);

        let repo = LocalRepository::new(tmp.path());
        let mut mgr =
            StackManager::single(Box::new(repo), ScannerProfile::EnhancedAndBack).unwrap();

        // Before scan
        assert!(mgr.is_empty());
        assert_eq!(mgr.len(), 0);

        // After scan (query auto-scans)
        mgr.query(None, None, None, None).unwrap();
        assert!(!mgr.is_empty());
        assert_eq!(mgr.len(), 1);
    }
}

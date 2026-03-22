//! Point-in-time snapshot for consistent pagination.
//!
//! A [`ScanSnapshot`] captures the result of a scan at a specific moment,
//! allowing callers to page through a stable set of stacks even as the
//! underlying filesystem changes. Stacks cannot appear, disappear, or
//! reorder between page requests within the same snapshot.
//!
//! ## Creating a Snapshot
//!
//! ```rust,no_run
//! use photostax_core::backends::local::LocalRepository;
//! use photostax_core::repository::Repository;
//! use photostax_core::snapshot::ScanSnapshot;
//!
//! let repo = LocalRepository::new("/photos");
//! let snapshot = ScanSnapshot::from_scan(&repo)?;
//!
//! let page1 = snapshot.get_page(0, 20);
//! let page2 = snapshot.get_page(20, 20);
//! assert_eq!(page1.total_count, page2.total_count); // always consistent
//! # Ok::<(), photostax_core::repository::RepositoryError>(())
//! ```
//!
//! ## Staleness Detection
//!
//! ```rust,no_run
//! # use photostax_core::backends::local::LocalRepository;
//! # use photostax_core::repository::Repository;
//! # use photostax_core::snapshot::ScanSnapshot;
//! # let repo = LocalRepository::new("/photos");
//! # let snapshot = ScanSnapshot::from_scan(&repo)?;
//! let status = snapshot.check_status(&repo)?;
//! if status.is_stale {
//!     println!("{} added, {} removed — consider refreshing", status.added, status.removed);
//! }
//! # Ok::<(), photostax_core::repository::RepositoryError>(())
//! ```

use std::collections::{HashMap, HashSet};

use crate::photo_stack::{PhotoStack, ScanProgress, ScannerProfile};
use crate::repository::{Repository, RepositoryError};
use crate::search::{
    filter_stacks, paginate_stacks, PaginatedResult, PaginationParams, SearchQuery,
};

/// A point-in-time snapshot of scanned photo stacks.
///
/// Created via [`ScanSnapshot::from_scan`] (lightweight) or
/// [`ScanSnapshot::from_scan_with_metadata`] (includes EXIF/XMP/sidecar).
/// Once created, the snapshot is fully in-memory and never touches the
/// filesystem again — `get_page()` is infallible and O(1) per item.
#[derive(Debug, Clone)]
pub struct ScanSnapshot {
    stacks: Vec<PhotoStack>,
    ids: HashSet<String>,
    /// Per-repo generation counters captured at snapshot time.
    repo_generations: HashMap<String, u64>,
}

/// Result of checking a snapshot against the current repository state.
///
/// Returned by [`ScanSnapshot::check_status`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotStatus {
    /// `true` when the filesystem no longer matches the snapshot.
    pub is_stale: bool,

    /// Number of stacks in the snapshot.
    pub snapshot_count: usize,

    /// Number of stacks currently on disk.
    pub current_count: usize,

    /// Number of new stacks found on disk but absent from the snapshot.
    pub added: usize,

    /// Number of snapshot stacks no longer present on disk.
    pub removed: usize,
}

impl ScanSnapshot {
    /// Create a snapshot from a lightweight scan (no file-based metadata).
    ///
    /// This is the fast path — equivalent to [`Repository::scan`].
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::Io`] if the scan fails.
    pub fn from_scan(repo: &dyn Repository) -> Result<Self, RepositoryError> {
        Self::from_scan_with_progress(repo, ScannerProfile::Auto, false, None)
    }

    /// Create a snapshot with full metadata loaded for every stack.
    ///
    /// This is the slow path — reads EXIF, XMP, and sidecar data for all
    /// stacks. Use when you need to [`filter`](Self::filter) by metadata.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::Io`] if the scan or metadata loading fails.
    pub fn from_scan_with_metadata(repo: &dyn Repository) -> Result<Self, RepositoryError> {
        Self::from_scan_with_progress(repo, ScannerProfile::Auto, true, None)
    }

    /// Create a snapshot with a [`ScannerProfile`] and optional progress callback.
    ///
    /// Combines scanning, classification, optional metadata loading, and
    /// snapshot creation into a single pass.
    ///
    /// # Parameters
    ///
    /// - `profile` — FastFoto scanner configuration (controls classification I/O)
    /// - `load_metadata` — if `true`, EXIF/XMP/sidecar is loaded for every stack
    /// - `progress` — optional callback invoked with [`ScanProgress`] per step
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::Io`] if the scan or metadata loading fails.
    pub fn from_scan_with_progress(
        repo: &dyn Repository,
        profile: ScannerProfile,
        load_metadata: bool,
        progress: Option<&mut dyn FnMut(&ScanProgress)>,
    ) -> Result<Self, RepositoryError> {
        let mut stacks = repo.scan_with_progress(profile, progress)?;
        if load_metadata {
            for stack in &mut stacks {
                let _ = stack.inner.write().unwrap().metadata.read()?;
            }
        }
        let ids = stacks.iter().map(|s| s.id()).collect();
        Ok(Self {
            stacks,
            ids,
            repo_generations: HashMap::new(),
        })
    }

    /// Create a snapshot from a pre-existing vector of stacks.
    ///
    /// Useful for creating filtered sub-snapshots or testing.
    pub fn from_stacks(stacks: Vec<PhotoStack>) -> Self {
        let ids = stacks.iter().map(|s| s.id()).collect();
        Self {
            stacks,
            ids,
            repo_generations: HashMap::new(),
        }
    }

    /// Total number of stacks in the snapshot.
    pub fn total_count(&self) -> usize {
        self.stacks.len()
    }

    /// Get a page of stacks from the snapshot.
    ///
    /// This is a pure in-memory operation and never fails. It returns a
    /// consistent page regardless of filesystem changes since creation.
    pub fn get_page(&self, offset: usize, limit: usize) -> PaginatedResult<PhotoStack> {
        paginate_stacks(&self.stacks, &PaginationParams { offset, limit })
    }

    /// Filter the snapshot by a search query, returning a new snapshot.
    ///
    /// The resulting snapshot contains only stacks matching the query.
    /// All page counts are recalculated against the filtered set.
    pub fn filter(&self, query: &SearchQuery) -> Self {
        let filtered = filter_stacks(&self.stacks, query);
        Self::from_stacks(filtered)
    }

    /// Borrow the full list of stacks in the snapshot.
    pub fn stacks(&self) -> &[PhotoStack] {
        &self.stacks
    }

    /// The set of stack IDs captured at snapshot time.
    pub fn ids(&self) -> &HashSet<String> {
        &self.ids
    }

    /// Returns `true` if any repo generation has advanced since this snapshot
    /// was created, meaning cached data may be out of date.
    pub fn is_stale(&self, current_generations: &HashMap<String, u64>) -> bool {
        for (repo_id, &snap_gen) in &self.repo_generations {
            if let Some(&current_gen) = current_generations.get(repo_id) {
                if current_gen > snap_gen {
                    return true;
                }
            }
        }
        false
    }

    /// Set the repo generation map (used by `StackManager` when building a snapshot).
    pub fn set_repo_generations(&mut self, gens: HashMap<String, u64>) {
        self.repo_generations = gens;
    }

    /// Check whether the snapshot is still current.
    ///
    /// Performs a fast scan (no metadata I/O) and compares the resulting
    /// IDs against those captured at snapshot time.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::Io`] if the re-scan fails.
    pub fn check_status(&self, repo: &dyn Repository) -> Result<SnapshotStatus, RepositoryError> {
        let current_stacks = repo.scan()?;
        let current_ids: HashSet<String> = current_stacks.iter().map(|s| s.id()).collect();

        let added = current_ids.difference(&self.ids).count();
        let removed = self.ids.difference(&current_ids).count();

        Ok(SnapshotStatus {
            is_stale: added > 0 || removed > 0,
            snapshot_count: self.stacks.len(),
            current_count: current_stacks.len(),
            added,
            removed,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image_handle::ImageRef;
    use std::sync::Arc;

    struct MockImg;
    impl crate::image_handle::ImageHandle for MockImg {
        fn read(
            &self,
        ) -> Result<Box<dyn crate::file_access::ReadSeek>, crate::repository::RepositoryError>
        {
            Ok(Box::new(std::io::Cursor::new(vec![])))
        }
        fn stream(
            &self,
        ) -> Result<
            crate::hashing::HashingReader<Box<dyn std::io::Read + Send>>,
            crate::repository::RepositoryError,
        > {
            Ok(crate::hashing::HashingReader::new(Box::new(
                std::io::Cursor::new(vec![]),
            )))
        }
        fn hash(&self) -> Result<String, crate::repository::RepositoryError> {
            Ok("0000000000000000".into())
        }
        fn dimensions(&self) -> Result<(u32, u32), crate::repository::RepositoryError> {
            Ok((1, 1))
        }
        fn size(&self) -> u64 {
            0
        }
        fn rotate(
            &self,
            _: crate::photo_stack::Rotation,
        ) -> Result<(), crate::repository::RepositoryError> {
            Ok(())
        }
        fn is_valid(&self) -> bool {
            true
        }
        fn invalidate(&self) {}
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    fn make_stack(id: &str) -> PhotoStack {
        let stack = PhotoStack::new(id);
        stack.inner.write().unwrap().original = ImageRef::new(Arc::new(MockImg));
        stack
    }

    fn make_stacks(n: usize) -> Vec<PhotoStack> {
        (0..n).map(|i| make_stack(&format!("IMG_{i:03}"))).collect()
    }

    #[test]
    fn test_from_stacks() {
        let stacks = make_stacks(5);
        let snap = ScanSnapshot::from_stacks(stacks);
        assert_eq!(snap.total_count(), 5);
        assert_eq!(snap.ids().len(), 5);
        assert!(snap.ids().contains("IMG_000"));
        assert!(snap.ids().contains("IMG_004"));
    }

    #[test]
    fn test_get_page_consistency() {
        let snap = ScanSnapshot::from_stacks(make_stacks(10));

        let page1 = snap.get_page(0, 3);
        let page2 = snap.get_page(3, 3);
        let page3 = snap.get_page(6, 3);

        // total_count is always the same
        assert_eq!(page1.total_count, 10);
        assert_eq!(page2.total_count, 10);
        assert_eq!(page3.total_count, 10);

        // pages don't overlap
        assert_eq!(page1.items[0].id(), "IMG_000");
        assert_eq!(page2.items[0].id(), "IMG_003");
        assert_eq!(page3.items[0].id(), "IMG_006");

        // has_more is correct
        assert!(page1.has_more);
        assert!(page2.has_more);
        assert!(page3.has_more); // 6+3=9 < 10
    }

    #[test]
    fn test_get_page_last_partial() {
        let snap = ScanSnapshot::from_stacks(make_stacks(5));
        let page = snap.get_page(3, 10);
        assert_eq!(page.items.len(), 2);
        assert_eq!(page.total_count, 5);
        assert!(!page.has_more);
    }

    #[test]
    fn test_get_page_beyond_end() {
        let snap = ScanSnapshot::from_stacks(make_stacks(5));
        let page = snap.get_page(100, 10);
        assert!(page.items.is_empty());
        assert_eq!(page.total_count, 5);
        assert!(!page.has_more);
    }

    #[test]
    fn test_get_page_empty_snapshot() {
        let snap = ScanSnapshot::from_stacks(vec![]);
        let page = snap.get_page(0, 10);
        assert!(page.items.is_empty());
        assert_eq!(page.total_count, 0);
        assert!(!page.has_more);
    }

    #[test]
    fn test_filter_returns_subset() {
        let stacks = make_stacks(4);
        stacks[0].inner.write().unwrap().back = ImageRef::new(Arc::new(MockImg));
        stacks[2].inner.write().unwrap().back = ImageRef::new(Arc::new(MockImg));

        let snap = ScanSnapshot::from_stacks(stacks);
        let filtered = snap.filter(&SearchQuery::new().with_has_back(true));

        assert_eq!(filtered.total_count(), 2);
        assert_eq!(filtered.stacks()[0].id(), "IMG_000");
        assert_eq!(filtered.stacks()[1].id(), "IMG_002");
    }

    #[test]
    fn test_filter_then_page() {
        let mut stacks = make_stacks(10);
        for s in &mut stacks {
            s.inner.write().unwrap().back = ImageRef::new(Arc::new(MockImg));
        }
        // Remove back from two stacks
        stacks[3].inner.write().unwrap().back = ImageRef::absent();
        stacks[7].inner.write().unwrap().back = ImageRef::absent();

        let snap = ScanSnapshot::from_stacks(stacks);
        let filtered = snap.filter(&SearchQuery::new().with_has_back(true));
        assert_eq!(filtered.total_count(), 8);

        let page = filtered.get_page(0, 3);
        assert_eq!(page.items.len(), 3);
        assert_eq!(page.total_count, 8);
        assert!(page.has_more);
    }

    #[test]
    fn test_filter_by_ids() {
        let snap = ScanSnapshot::from_stacks(make_stacks(5));
        let filtered = snap.filter(
            &SearchQuery::new().with_ids(vec!["IMG_001".to_string(), "IMG_003".to_string()]),
        );
        assert_eq!(filtered.total_count(), 2);
        assert_eq!(filtered.stacks()[0].id(), "IMG_001");
        assert_eq!(filtered.stacks()[1].id(), "IMG_003");
    }

    #[test]
    fn test_stacks_borrow() {
        let snap = ScanSnapshot::from_stacks(make_stacks(3));
        assert_eq!(snap.stacks().len(), 3);
        assert_eq!(snap.stacks()[0].id(), "IMG_000");
    }

    // Integration tests with real repo are in backends/local.rs
}

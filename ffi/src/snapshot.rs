//! Snapshot FFI functions.
//!
//! Provides C-compatible access to [`ScanSnapshot`] for consistent pagination.
//! A snapshot captures the scan result at a point in time so that page
//! requests always see the same total count and ordering.

use std::os::raw::{c_char, c_void};
use std::panic::{self, AssertUnwindSafe};
use std::ptr;

use photostax_core::photo_stack::ScannerProfile;
use photostax_core::search::SearchQuery;
use photostax_core::snapshot::ScanSnapshot;

use crate::repository::ScanProgressFn;
use crate::types::{FfiPaginatedResult, FfiPhotoStack, PhotostaxRepo};

/// Opaque handle to a scan snapshot.
pub struct PhotostaxSnapshot {
    inner: ScanSnapshot,
}

/// Staleness information returned by [`photostax_snapshot_check_status`].
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FfiSnapshotStatus {
    /// `true` when the filesystem no longer matches the snapshot.
    pub is_stale: bool,
    /// Number of stacks captured in the snapshot.
    pub snapshot_count: usize,
    /// Number of stacks currently on disk.
    pub current_count: usize,
    /// New stacks on disk that were not in the snapshot.
    pub added: usize,
    /// Snapshot stacks no longer present on disk.
    pub removed: usize,
}

// ── Helpers ─────────────────────────────────────────────────────

fn photo_stack_to_ffi(stack: &photostax_core::photo_stack::PhotoStack) -> FfiPhotoStack {
    use std::ffi::CString;

    let id = CString::new(stack.id.clone())
        .map(|s| s.into_raw())
        .unwrap_or(ptr::null_mut());

    let name = CString::new(stack.name.clone())
        .map(|s| s.into_raw())
        .unwrap_or(ptr::null_mut());

    let path_to_c_string = |img: &Option<photostax_core::hashing::ImageFile>| -> *mut c_char {
        match img {
            Some(f) => {
                let s = f.path.clone();
                CString::new(s)
                    .map(|cs| cs.into_raw())
                    .unwrap_or(ptr::null_mut())
            }
            None => ptr::null_mut(),
        }
    };

    let metadata_json = serde_json::json!({
        "exif_tags": stack.metadata.exif_tags,
        "xmp_tags": stack.metadata.xmp_tags,
        "custom_tags": stack.metadata.custom_tags,
    });
    let metadata_str = serde_json::to_string(&metadata_json).unwrap_or_else(|_| "{}".to_string());
    let metadata_json_ptr = std::ffi::CString::new(metadata_str)
        .map(|s| s.into_raw())
        .unwrap_or(ptr::null_mut());

    FfiPhotoStack {
        id,
        name,
        folder: stack
            .folder
            .as_deref()
            .and_then(|f| CString::new(f).ok())
            .map(|s| s.into_raw())
            .unwrap_or(ptr::null_mut()),
        original: path_to_c_string(&stack.original),
        enhanced: path_to_c_string(&stack.enhanced),
        back: path_to_c_string(&stack.back),
        metadata_json: metadata_json_ptr,
    }
}

// ── Public FFI Functions ────────────────────────────────────────

/// Create a snapshot from a lightweight scan (no file-based metadata).
///
/// # Safety
///
/// - `repo` must be a valid pointer from [`photostax_repo_open`]
/// - Returns null on error
/// - Caller owns the returned pointer and must call [`photostax_snapshot_free`]
///
/// [`photostax_repo_open`]: crate::repository::photostax_repo_open
#[no_mangle]
pub unsafe extern "C" fn photostax_create_snapshot(
    repo: *const PhotostaxRepo,
    load_metadata: bool,
) -> *mut PhotostaxSnapshot {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if repo.is_null() {
            return ptr::null_mut();
        }

        let repo_ref = unsafe { &*repo };
        let mut mgr = repo_ref.inner.borrow_mut();
        let scan_result = if load_metadata {
            mgr.scan_with_metadata()
        } else {
            mgr.scan()
        };
        if scan_result.is_err() {
            return ptr::null_mut();
        }
        let snap = mgr.snapshot();
        drop(mgr);

        Box::into_raw(Box::new(PhotostaxSnapshot { inner: snap }))
    }));

    result.unwrap_or(ptr::null_mut())
}

/// Create a snapshot with a scanner profile and optional progress callback.
///
/// Combines scanning, classification, optional metadata loading, and
/// snapshot creation in a single pass — no redundant re-scanning.
///
/// # Parameters
///
/// - `profile` — scanner profile (0=Auto, 1=EnhancedAndBack, 2=EnhancedOnly, 3=OriginalOnly)
/// - `load_metadata` — if true, EXIF/XMP/sidecar is loaded for every stack
/// - `callback` — optional progress callback (may be null)
/// - `user_data` — opaque pointer forwarded to callback (may be null)
///
/// # Safety
///
/// - `repo` must be a valid pointer from [`photostax_repo_open`]
/// - `callback` and `user_data` must be valid for the duration of the call
/// - Returns null on error
/// - Caller owns the returned pointer and must call [`photostax_snapshot_free`]
#[no_mangle]
pub unsafe extern "C" fn photostax_create_snapshot_with_progress(
    repo: *const PhotostaxRepo,
    profile: i32,
    load_metadata: bool,
    callback: ScanProgressFn,
    user_data: *mut c_void,
) -> *mut PhotostaxSnapshot {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if repo.is_null() {
            return ptr::null_mut();
        }

        let repo_ref = unsafe { &*repo };
        let _scanner_profile = ScannerProfile::from_int(profile).unwrap_or_default();

        let mut cb_wrapper;
        let progress: Option<&mut dyn FnMut(&photostax_core::photo_stack::ScanProgress)> =
            if let Some(cb_fn) = callback {
                cb_wrapper = move |p: &photostax_core::photo_stack::ScanProgress| unsafe {
                    cb_fn(p.phase as i32, p.current, p.total, user_data);
                };
                Some(&mut cb_wrapper)
            } else {
                None
            };

        let mut mgr = repo_ref.inner.borrow_mut();
        if mgr.scan_with_progress(progress).is_err() {
            return ptr::null_mut();
        }
        if load_metadata {
            let all = mgr.query(&photostax_core::search::SearchQuery::new(), None);
            for stack in &all.items {
                let _ = mgr.load_metadata(&stack.id);
            }
        }
        let snap = mgr.snapshot();
        drop(mgr);

        Box::into_raw(Box::new(PhotostaxSnapshot { inner: snap }))
    }));

    result.unwrap_or(ptr::null_mut())
}

/// Get the total number of stacks in the snapshot.
///
/// # Safety
///
/// - `snapshot` must be a valid pointer from [`photostax_create_snapshot`]
/// - Returns 0 on null pointer
#[no_mangle]
pub unsafe extern "C" fn photostax_snapshot_total_count(
    snapshot: *const PhotostaxSnapshot,
) -> usize {
    if snapshot.is_null() {
        return 0;
    }
    let snap = unsafe { &*snapshot };
    snap.inner.total_count()
}

/// Get a page of stacks from the snapshot.
///
/// This is a pure in-memory operation — it never accesses the filesystem
/// and always returns a consistent page.
///
/// # Safety
///
/// - `snapshot` must be a valid pointer from [`photostax_create_snapshot`]
/// - Returns empty result on null pointer
/// - Caller owns the returned result and must call [`photostax_paginated_result_free`]
///
/// [`photostax_paginated_result_free`]: crate::repository::photostax_paginated_result_free
#[no_mangle]
pub unsafe extern "C" fn photostax_snapshot_get_page(
    snapshot: *const PhotostaxSnapshot,
    offset: usize,
    limit: usize,
) -> FfiPaginatedResult {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if snapshot.is_null() {
            return FfiPaginatedResult::empty(offset, limit);
        }

        let snap = unsafe { &*snapshot };
        let paginated = snap.inner.get_page(offset, limit);

        if paginated.items.is_empty() {
            return FfiPaginatedResult {
                data: ptr::null_mut(),
                len: 0,
                total_count: paginated.total_count,
                offset: paginated.offset,
                limit: paginated.limit,
                has_more: paginated.has_more,
            };
        }

        let ffi_stacks: Vec<FfiPhotoStack> =
            paginated.items.iter().map(photo_stack_to_ffi).collect();
        let len = ffi_stacks.len();
        let boxed_slice = ffi_stacks.into_boxed_slice();
        let data = Box::into_raw(boxed_slice) as *mut FfiPhotoStack;

        FfiPaginatedResult {
            data,
            len,
            total_count: paginated.total_count,
            offset: paginated.offset,
            limit: paginated.limit,
            has_more: paginated.has_more,
        }
    }));

    result.unwrap_or_else(|_| FfiPaginatedResult::empty(offset, limit))
}

/// Check whether a snapshot is still current.
///
/// Performs a fast re-scan (no metadata I/O) and compares against the
/// snapshot to report added/removed stacks.
///
/// # Safety
///
/// - `repo` must be a valid pointer from [`photostax_repo_open`]
/// - `snapshot` must be a valid pointer from [`photostax_create_snapshot`]
/// - Returns a zeroed status with `is_stale = true` on error
///
/// [`photostax_repo_open`]: crate::repository::photostax_repo_open
#[no_mangle]
pub unsafe extern "C" fn photostax_snapshot_check_status(
    repo: *const PhotostaxRepo,
    snapshot: *const PhotostaxSnapshot,
) -> FfiSnapshotStatus {
    let error_status = FfiSnapshotStatus {
        is_stale: true,
        snapshot_count: 0,
        current_count: 0,
        added: 0,
        removed: 0,
    };

    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if repo.is_null() || snapshot.is_null() {
            return error_status;
        }

        let repo_ref = unsafe { &*repo };
        let snap = unsafe { &*snapshot };

        // Re-scan to get the current state for comparison
        let mut mgr = repo_ref.inner.borrow_mut();
        if mgr.scan().is_err() {
            return error_status;
        }
        let status = mgr.check_status(&snap.inner);
        drop(mgr);

        FfiSnapshotStatus {
            is_stale: status.is_stale,
            snapshot_count: status.snapshot_count,
            current_count: status.current_count,
            added: status.added,
            removed: status.removed,
        }
    }));

    result.unwrap_or(error_status)
}

/// Create a new snapshot by filtering an existing one.
///
/// The `query_json` format is the same as [`photostax_search`].
/// Returns a new snapshot containing only matching stacks.
///
/// # Safety
///
/// - `snapshot` must be a valid pointer from [`photostax_create_snapshot`]
/// - `query_json` must be a valid null-terminated JSON string
/// - Returns null on error
/// - Caller owns the returned pointer and must call [`photostax_snapshot_free`]
///
/// [`photostax_search`]: crate::search::photostax_search
#[no_mangle]
pub unsafe extern "C" fn photostax_snapshot_filter(
    snapshot: *const PhotostaxSnapshot,
    query_json: *const c_char,
) -> *mut PhotostaxSnapshot {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if snapshot.is_null() || query_json.is_null() {
            return ptr::null_mut();
        }

        let snap = unsafe { &*snapshot };
        let query_str = match unsafe { std::ffi::CStr::from_ptr(query_json) }.to_str() {
            Ok(s) => s,
            Err(_) => return ptr::null_mut(),
        };

        #[derive(serde::Deserialize, Default)]
        struct QueryInput {
            #[serde(default)]
            exif_filters: Vec<(String, String)>,
            #[serde(default)]
            custom_filters: Vec<(String, String)>,
            #[serde(default)]
            text_query: Option<String>,
            #[serde(default)]
            has_back: Option<bool>,
            #[serde(default)]
            has_enhanced: Option<bool>,
            #[serde(default)]
            stack_ids: Option<Vec<String>>,
        }

        let input: QueryInput = match serde_json::from_str(query_str) {
            Ok(q) => q,
            Err(_) => return ptr::null_mut(),
        };

        let mut query = SearchQuery::new();
        for (key, value) in input.exif_filters {
            query = query.with_exif_filter(key, value);
        }
        for (key, value) in input.custom_filters {
            query = query.with_custom_filter(key, value);
        }
        if let Some(text) = input.text_query {
            query = query.with_text(text);
        }
        if let Some(has_back) = input.has_back {
            query = query.with_has_back(has_back);
        }
        if let Some(has_enhanced) = input.has_enhanced {
            query = query.with_has_enhanced(has_enhanced);
        }
        if let Some(ids) = input.stack_ids {
            query = query.with_ids(ids);
        }

        let filtered = snap.inner.filter(&query);
        Box::into_raw(Box::new(PhotostaxSnapshot { inner: filtered }))
    }));

    result.unwrap_or(ptr::null_mut())
}

/// Free a snapshot handle.
///
/// # Safety
///
/// - `snapshot` must be a valid pointer from [`photostax_create_snapshot`]
///   or [`photostax_snapshot_filter`], or null (no-op).
#[no_mangle]
pub unsafe extern "C" fn photostax_snapshot_free(snapshot: *mut PhotostaxSnapshot) {
    if !snapshot.is_null() {
        drop(unsafe { Box::from_raw(snapshot) });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::{
        photostax_paginated_result_free, photostax_repo_free, photostax_repo_open,
    };
    use std::ffi::{CStr, CString};

    fn testdata_path() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("core")
            .join("tests")
            .join("testdata")
    }

    fn open_testdata_repo() -> *mut PhotostaxRepo {
        let path = CString::new(testdata_path().to_str().unwrap()).unwrap();
        let repo = unsafe { photostax_repo_open(path.as_ptr()) };
        assert!(!repo.is_null());
        repo
    }

    #[test]
    fn test_create_snapshot_null_repo() {
        let snap = unsafe { photostax_create_snapshot(ptr::null(), false) };
        assert!(snap.is_null());
    }

    #[test]
    fn test_create_snapshot_basic() {
        let repo = open_testdata_repo();
        let snap = unsafe { photostax_create_snapshot(repo, false) };
        assert!(!snap.is_null());

        let count = unsafe { photostax_snapshot_total_count(snap) };
        assert!(count > 0);

        unsafe { photostax_snapshot_free(snap) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_create_snapshot_with_metadata() {
        let repo = open_testdata_repo();
        let snap = unsafe { photostax_create_snapshot(repo, true) };
        assert!(!snap.is_null());

        let count = unsafe { photostax_snapshot_total_count(snap) };
        assert!(count > 0);

        unsafe { photostax_snapshot_free(snap) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_snapshot_total_count_null() {
        assert_eq!(unsafe { photostax_snapshot_total_count(ptr::null()) }, 0);
    }

    #[test]
    fn test_snapshot_get_page() {
        let repo = open_testdata_repo();
        let snap = unsafe { photostax_create_snapshot(repo, false) };
        assert!(!snap.is_null());

        let total = unsafe { photostax_snapshot_total_count(snap) };

        let page1 = unsafe { photostax_snapshot_get_page(snap, 0, 2) };
        assert!(page1.len <= 2);
        assert_eq!(page1.total_count, total);

        let page2 = unsafe { photostax_snapshot_get_page(snap, 2, 2) };
        assert_eq!(page2.total_count, total); // consistent!

        unsafe { photostax_paginated_result_free(page1) };
        unsafe { photostax_paginated_result_free(page2) };
        unsafe { photostax_snapshot_free(snap) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_snapshot_get_page_null() {
        let page = unsafe { photostax_snapshot_get_page(ptr::null(), 0, 10) };
        assert!(page.data.is_null());
        assert_eq!(page.len, 0);
    }

    #[test]
    fn test_snapshot_check_status_unchanged() {
        let repo = open_testdata_repo();
        let snap = unsafe { photostax_create_snapshot(repo, false) };
        assert!(!snap.is_null());

        let status = unsafe { photostax_snapshot_check_status(repo, snap) };
        assert!(!status.is_stale);
        assert_eq!(status.added, 0);
        assert_eq!(status.removed, 0);
        assert_eq!(status.snapshot_count, status.current_count);

        unsafe { photostax_snapshot_free(snap) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_snapshot_check_status_null() {
        let repo = open_testdata_repo();
        let status = unsafe { photostax_snapshot_check_status(repo, ptr::null()) };
        assert!(status.is_stale); // error → stale
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_snapshot_check_status_after_change() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        std::fs::write(dir.join("IMG_001.jpg"), b"fake").unwrap();

        let path = CString::new(dir.to_str().unwrap()).unwrap();
        let repo = unsafe { photostax_repo_open(path.as_ptr()) };
        let snap = unsafe { photostax_create_snapshot(repo, false) };
        assert!(!snap.is_null());

        // Add a file
        std::fs::write(dir.join("IMG_002.jpg"), b"fake2").unwrap();

        let status = unsafe { photostax_snapshot_check_status(repo, snap) };
        assert!(status.is_stale);
        assert_eq!(status.added, 1);
        assert_eq!(status.removed, 0);
        assert_eq!(status.snapshot_count, 1);
        assert_eq!(status.current_count, 2);

        // Pages still work
        let page = unsafe { photostax_snapshot_get_page(snap, 0, 10) };
        assert_eq!(page.total_count, 1); // snapshot is frozen
        unsafe { photostax_paginated_result_free(page) };

        unsafe { photostax_snapshot_free(snap) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_snapshot_filter() {
        let repo = open_testdata_repo();
        let snap = unsafe { photostax_create_snapshot(repo, true) };
        assert!(!snap.is_null());

        let total_before = unsafe { photostax_snapshot_total_count(snap) };

        let query = CString::new(r#"{"text_query":"FamilyPhotos"}"#).unwrap();
        let filtered = unsafe { photostax_snapshot_filter(snap, query.as_ptr()) };
        assert!(!filtered.is_null());

        let filtered_count = unsafe { photostax_snapshot_total_count(filtered) };
        assert!(filtered_count > 0);
        assert!(filtered_count <= total_before);

        // Verify page items match filter
        let page = unsafe { photostax_snapshot_get_page(filtered, 0, 100) };
        for i in 0..page.len {
            let item = unsafe { &*page.data.add(i) };
            let name = unsafe { CStr::from_ptr(item.name) }.to_str().unwrap();
            assert!(
                name.contains("FamilyPhotos"),
                "expected FamilyPhotos in {name}"
            );
        }

        unsafe { photostax_paginated_result_free(page) };
        unsafe { photostax_snapshot_free(filtered) };
        unsafe { photostax_snapshot_free(snap) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_snapshot_filter_null() {
        let query = CString::new("{}").unwrap();
        let filtered = unsafe { photostax_snapshot_filter(ptr::null(), query.as_ptr()) };
        assert!(filtered.is_null());
    }

    #[test]
    fn test_snapshot_free_null() {
        unsafe { photostax_snapshot_free(ptr::null_mut()) };
    }
}

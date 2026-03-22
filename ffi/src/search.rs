//! Search FFI functions.
//!
//! Provides C-compatible access to photostax-core search functionality.

use std::ffi::CStr;
use std::os::raw::c_char;
use std::panic::{self, AssertUnwindSafe};

use photostax_core::search::SearchQuery;
use serde::Deserialize;

use crate::repository::{stacks_to_handle_array, stacks_to_paginated_handles};
use crate::types::{FfiPaginatedHandleResult, FfiStackHandleArray, PhotostaxRepo};

/// Search/filter stacks. `query_json` is a JSON-serialized SearchQuery.
///
/// # Query JSON Format
///
/// ```json
/// {
///   "exif_filters": [["Make", "EPSON"], ["Model", "FastFoto"]],
///   "custom_filters": [["album", "Family"]],
///   "text_query": "birthday",
///   "has_back": true,
///   "has_enhanced": null
/// }
/// ```
///
/// All fields are optional. An empty object `{}` matches all stacks.
///
/// # Safety
///
/// - `repo` must be a valid pointer from [`photostax_repo_open`]
/// - `query_json` must be a valid null-terminated JSON string
/// - Returns empty array on null pointers or errors
/// - Caller owns the returned array and must call [`photostax_stack_handle_array_free`]
///
/// [`photostax_repo_open`]: crate::repository::photostax_repo_open
/// [`photostax_stack_handle_array_free`]: crate::repository::photostax_stack_handle_array_free
#[no_mangle]
pub unsafe extern "C" fn photostax_search(
    repo: *const PhotostaxRepo,
    query_json: *const c_char,
) -> FfiStackHandleArray {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if repo.is_null() || query_json.is_null() {
            return FfiStackHandleArray::empty();
        }

        let repo_ref = unsafe { &*repo };
        let query_str = match unsafe { CStr::from_ptr(query_json) }.to_str() {
            Ok(s) => s,
            Err(_) => return FfiStackHandleArray::empty(),
        };

        // Parse the query JSON
        #[derive(Deserialize, Default)]
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
            Err(_) => return FfiStackHandleArray::empty(),
        };

        // Build the SearchQuery
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

        // Get all stacks with metadata (search needs metadata to filter)
        let mut mgr = repo_ref.inner.borrow_mut();
        mgr.invalidate_cache();
        let initial = match mgr.query(None, None, None) {
            Ok(r) => r,
            Err(_) => return FfiStackHandleArray::empty(),
        };
        for stack in initial.all_stacks() {
            let _ = stack.metadata().read();
        }

        // Apply the filter using query()
        let filtered = match mgr.query(Some(&query), None, None) {
            Ok(snap) => snap,
            Err(_) => return FfiStackHandleArray::empty(),
        };
        drop(mgr);

        stacks_to_handle_array(filtered.all_stacks())
    }));

    result.unwrap_or_else(|_| FfiStackHandleArray::empty())
}

/// Search/filter stacks with pagination. `query_json` is a JSON-serialized SearchQuery.
///
/// # Query JSON Format
///
/// Same as [`photostax_search`], but results are paginated.
///
/// # Safety
///
/// - `repo` must be a valid pointer from [`photostax_repo_open`]
/// - `query_json` must be a valid null-terminated JSON string
/// - Returns empty result on null pointers or errors
/// - Caller owns the returned result and must call [`photostax_paginated_handle_result_free`]
///
/// [`photostax_repo_open`]: crate::repository::photostax_repo_open
/// [`photostax_paginated_handle_result_free`]: crate::repository::photostax_paginated_handle_result_free
#[no_mangle]
pub unsafe extern "C" fn photostax_search_paginated(
    repo: *const PhotostaxRepo,
    query_json: *const c_char,
    offset: usize,
    limit: usize,
) -> FfiPaginatedHandleResult {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if repo.is_null() || query_json.is_null() {
            return FfiPaginatedHandleResult::empty(offset, limit);
        }

        let repo_ref = unsafe { &*repo };
        let query_str = match unsafe { CStr::from_ptr(query_json) }.to_str() {
            Ok(s) => s,
            Err(_) => return FfiPaginatedHandleResult::empty(offset, limit),
        };

        #[derive(Deserialize, Default)]
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
            Err(_) => return FfiPaginatedHandleResult::empty(offset, limit),
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

        let mut mgr = repo_ref.inner.borrow_mut();
        mgr.invalidate_cache();
        let initial = match mgr.query(None, None, None) {
            Ok(r) => r,
            Err(_) => return FfiPaginatedHandleResult::empty(offset, limit),
        };
        for stack in initial.all_stacks() {
            let _ = stack.metadata().read();
        }

        let snapshot = match mgr.query(Some(&query), None, None) {
            Ok(snap) => snap,
            Err(_) => return FfiPaginatedHandleResult::empty(offset, limit),
        };
        drop(mgr);

        let paginated = snapshot.snapshot().get_page(offset, limit);
        stacks_to_paginated_handles(
            &paginated.items,
            paginated.total_count,
            paginated.offset,
            paginated.limit,
            paginated.has_more,
        )
    }));

    result.unwrap_or_else(|_| FfiPaginatedHandleResult::empty(offset, limit))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::{
        photostax_paginated_handle_result_free, photostax_repo_free, photostax_repo_open,
        photostax_stack_handle_array_free, photostax_stack_id, photostax_string_free,
    };
    use std::ffi::{CStr, CString};
    use std::ptr;

    fn testdata_path() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("core")
            .join("tests")
            .join("testdata")
    }

    fn open_testdata_repo() -> *mut crate::types::PhotostaxRepo {
        let path = CString::new(testdata_path().to_str().unwrap()).unwrap();
        let repo = unsafe { photostax_repo_open(path.as_ptr()) };
        assert!(!repo.is_null());
        repo
    }

    #[test]
    fn test_search_null_repo() {
        let query = CString::new("{}").unwrap();
        let result = unsafe { photostax_search(ptr::null(), query.as_ptr()) };
        assert!(result.handles.is_null());
        assert_eq!(result.len, 0);
    }

    #[test]
    fn test_search_null_query() {
        let path = CString::new(".").unwrap();
        let repo = unsafe { photostax_repo_open(path.as_ptr()) };
        assert!(!repo.is_null());

        let result = unsafe { photostax_search(repo, ptr::null()) };
        assert!(result.handles.is_null());
        assert_eq!(result.len, 0);

        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_search_invalid_json() {
        let path = CString::new(".").unwrap();
        let repo = unsafe { photostax_repo_open(path.as_ptr()) };
        assert!(!repo.is_null());

        let query = CString::new("not valid json").unwrap();
        let result = unsafe { photostax_search(repo, query.as_ptr()) };
        assert!(result.handles.is_null());
        assert_eq!(result.len, 0);

        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_search_empty_query_testdata() {
        let repo = open_testdata_repo();
        let query = CString::new("{}").unwrap();
        let result = unsafe { photostax_search(repo, query.as_ptr()) };

        // Empty query returns all stacks from testdata
        assert!(result.len > 0, "Expected stacks from testdata");
        assert!(!result.handles.is_null());

        // Verify first result has valid data
        let first_handle = unsafe { *result.handles };
        assert!(!first_handle.is_null());
        let id_ptr = unsafe { photostax_stack_id(first_handle) };
        assert!(!id_ptr.is_null());
        let id_str = unsafe { CStr::from_ptr(id_ptr) }.to_str().unwrap();
        assert!(!id_str.is_empty());
        unsafe { photostax_string_free(id_ptr) };

        unsafe { photostax_stack_handle_array_free(result) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_search_with_text_query() {
        let repo = open_testdata_repo();
        let query = CString::new(r#"{"text_query":"FamilyPhotos"}"#).unwrap();
        let result = unsafe { photostax_search(repo, query.as_ptr()) };

        assert!(result.len > 0, "Should find FamilyPhotos stacks");

        unsafe { photostax_stack_handle_array_free(result) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_search_with_text_no_match() {
        let repo = open_testdata_repo();
        let query = CString::new(r#"{"text_query":"zzz_nonexistent_zzz"}"#).unwrap();
        let result = unsafe { photostax_search(repo, query.as_ptr()) };

        assert_eq!(result.len, 0);
        assert!(result.handles.is_null());

        unsafe { photostax_stack_handle_array_free(result) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_search_with_has_back_filter() {
        let repo = open_testdata_repo();
        let query = CString::new(r#"{"has_back":true}"#).unwrap();
        let result = unsafe { photostax_search(repo, query.as_ptr()) };

        // Some stacks in testdata have _b files
        if result.len > 0 {
            let first_handle = unsafe { *result.handles };
            assert!(!first_handle.is_null());
        }

        unsafe { photostax_stack_handle_array_free(result) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_search_with_has_enhanced_filter() {
        let repo = open_testdata_repo();
        let query = CString::new(r#"{"has_enhanced":true}"#).unwrap();
        let result = unsafe { photostax_search(repo, query.as_ptr()) };

        if result.len > 0 {
            let first_handle = unsafe { *result.handles };
            assert!(!first_handle.is_null());
        }

        unsafe { photostax_stack_handle_array_free(result) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_search_with_exif_filter() {
        let repo = open_testdata_repo();
        let query = CString::new(r#"{"exif_filters":[["Make","EPSON"]]}"#).unwrap();
        let result = unsafe { photostax_search(repo, query.as_ptr()) };

        // Testdata has EPSON EXIF tags
        if result.len > 0 {
            let first_handle = unsafe { *result.handles };
            assert!(!first_handle.is_null());
        }

        unsafe { photostax_stack_handle_array_free(result) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_search_with_custom_filter() {
        let repo = open_testdata_repo();
        let query = CString::new(r#"{"custom_filters":[["album","Family"]]}"#).unwrap();
        let result = unsafe { photostax_search(repo, query.as_ptr()) };
        // Likely no match but should not crash
        unsafe { photostax_stack_handle_array_free(result) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_search_combined_filters() {
        let repo = open_testdata_repo();
        let query = CString::new(
            r#"{
            "text_query": "FamilyPhotos",
            "has_back": true,
            "has_enhanced": true
        }"#,
        )
        .unwrap();
        let result = unsafe { photostax_search(repo, query.as_ptr()) };
        unsafe { photostax_stack_handle_array_free(result) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_search_invalid_utf8_query() {
        let repo = open_testdata_repo();
        let invalid: &[u8] = &[0xff, 0x00];
        let result = unsafe { photostax_search(repo, invalid.as_ptr() as *const c_char) };
        assert_eq!(result.len, 0);
        assert!(result.handles.is_null());
        unsafe { crate::repository::photostax_repo_free(repo) };
    }

    #[test]
    fn test_search_scan_error() {
        let path = CString::new("/nonexistent/search/dir").unwrap();
        let repo = unsafe { crate::repository::photostax_repo_open(path.as_ptr()) };
        assert!(!repo.is_null());
        let query = CString::new(r#"{"text":"test"}"#).unwrap();
        let result = unsafe { photostax_search(repo, query.as_ptr()) };
        // Should return empty (scan failed or returned empty)
        assert!(result.len == 0);
        unsafe { crate::repository::photostax_repo_free(repo) };
    }

    // ── Paginated search tests ──────────────────────────────────

    #[test]
    fn test_search_paginated_null_repo() {
        let query = CString::new("{}").unwrap();
        let result = unsafe { photostax_search_paginated(ptr::null(), query.as_ptr(), 0, 10) };
        assert!(result.handles.is_null());
        assert_eq!(result.len, 0);
        assert_eq!(result.total_count, 0);
        assert_eq!(result.offset, 0);
        assert_eq!(result.limit, 10);
        assert!(!result.has_more);
    }

    #[test]
    fn test_search_paginated_null_query() {
        let path = CString::new(".").unwrap();
        let repo = unsafe { photostax_repo_open(path.as_ptr()) };
        assert!(!repo.is_null());

        let result = unsafe { photostax_search_paginated(repo, ptr::null(), 0, 10) };
        assert!(result.handles.is_null());
        assert_eq!(result.len, 0);

        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_search_paginated_testdata() {
        let repo = open_testdata_repo();
        let query = CString::new("{}").unwrap();

        // Get first page of 2
        let page1 = unsafe { photostax_search_paginated(repo, query.as_ptr(), 0, 2) };
        assert!(page1.total_count > 0, "Expected stacks from testdata");
        assert!(page1.len <= 2);
        assert_eq!(page1.offset, 0);
        assert_eq!(page1.limit, 2);

        if page1.total_count > 2 {
            assert!(page1.has_more);
        }

        unsafe { photostax_paginated_handle_result_free(page1) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_search_paginated_with_text_filter() {
        let repo = open_testdata_repo();
        let query = CString::new(r#"{"text_query":"FamilyPhotos"}"#).unwrap();

        let result = unsafe { photostax_search_paginated(repo, query.as_ptr(), 0, 100) };
        // total_count should reflect filtered set
        assert!(result.total_count >= result.len);

        unsafe { photostax_paginated_handle_result_free(result) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_search_paginated_offset_beyond_end() {
        let repo = open_testdata_repo();
        let query = CString::new("{}").unwrap();

        let result = unsafe { photostax_search_paginated(repo, query.as_ptr(), 10000, 10) };
        assert_eq!(result.len, 0);
        assert!(result.handles.is_null());
        // total_count is still the full count
        assert!(!result.has_more);

        unsafe { photostax_paginated_handle_result_free(result) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_search_with_stack_ids_filter() {
        let repo = open_testdata_repo();
        // Get all stacks first to find valid IDs
        let all_query = CString::new("{}").unwrap();
        let all_result = unsafe { photostax_search(repo, all_query.as_ptr()) };
        assert!(all_result.len > 0, "Expected stacks from testdata");

        let first_handle = unsafe { *all_result.handles };
        let id_ptr = unsafe { photostax_stack_id(first_handle) };
        let first_id = unsafe { CStr::from_ptr(id_ptr) }
            .to_str()
            .unwrap()
            .to_string();
        unsafe { photostax_string_free(id_ptr) };
        unsafe { photostax_stack_handle_array_free(all_result) };

        // Search with stack_ids containing only the first ID
        let query_json = format!(r#"{{"stack_ids":["{}"]}}"#, first_id);
        let query = CString::new(query_json).unwrap();
        let result = unsafe { photostax_search(repo, query.as_ptr()) };

        assert_eq!(result.len, 1);
        let result_handle = unsafe { *result.handles };
        let result_id_ptr = unsafe { photostax_stack_id(result_handle) };
        let result_id = unsafe { CStr::from_ptr(result_id_ptr) }.to_str().unwrap();
        assert_eq!(result_id, first_id);
        unsafe { photostax_string_free(result_id_ptr) };

        unsafe { photostax_stack_handle_array_free(result) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_search_with_stack_ids_no_match() {
        let repo = open_testdata_repo();
        let query = CString::new(r#"{"stack_ids":["NONEXISTENT_ID"]}"#).unwrap();
        let result = unsafe { photostax_search(repo, query.as_ptr()) };

        assert_eq!(result.len, 0);
        assert!(result.handles.is_null());

        unsafe { photostax_stack_handle_array_free(result) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_search_paginated_with_stack_ids() {
        let repo = open_testdata_repo();
        // Get all stacks first
        let all_query = CString::new("{}").unwrap();
        let all_result = unsafe { photostax_search(repo, all_query.as_ptr()) };
        assert!(all_result.len > 0);

        let first_handle = unsafe { *all_result.handles };
        let id_ptr = unsafe { photostax_stack_id(first_handle) };
        let first_id = unsafe { CStr::from_ptr(id_ptr) }
            .to_str()
            .unwrap()
            .to_string();
        unsafe { photostax_string_free(id_ptr) };
        unsafe { photostax_stack_handle_array_free(all_result) };

        // Paginated search with stack_ids
        let query_json = format!(r#"{{"stack_ids":["{}"]}}"#, first_id);
        let query = CString::new(query_json).unwrap();
        let result = unsafe { photostax_search_paginated(repo, query.as_ptr(), 0, 10) };

        assert_eq!(result.total_count, 1);
        assert_eq!(result.len, 1);
        assert!(!result.has_more);

        unsafe { photostax_paginated_handle_result_free(result) };
        unsafe { photostax_repo_free(repo) };
    }
}

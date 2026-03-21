//! Search FFI functions.
//!
//! Provides C-compatible access to photostax-core search functionality.

use std::ffi::CStr;
use std::os::raw::c_char;
use std::panic::{self, AssertUnwindSafe};

use photostax_core::search::SearchQuery;
use serde::Deserialize;

use crate::types::{FfiPaginatedResult, FfiPhotoStack, FfiPhotoStackArray, PhotostaxRepo};

/// Helper to convert a PhotoStack to an FfiPhotoStack.
fn photo_stack_to_ffi(stack: &photostax_core::photo_stack::PhotoStack) -> FfiPhotoStack {
    use std::ffi::CString;
    use std::ptr;

    let id = CString::new(stack.id.clone())
        .map(|s| s.into_raw())
        .unwrap_or(ptr::null_mut());

    let name = CString::new(stack.name.clone())
        .map(|s| s.into_raw())
        .unwrap_or(ptr::null_mut());

    let image_ref_to_c = |img: &photostax_core::image_handle::ImageRef| -> *mut c_char {
        if img.is_present() {
            CString::new("present")
                .map(|cs| cs.into_raw())
                .unwrap_or(ptr::null_mut())
        } else {
            ptr::null_mut()
        }
    };

    let metadata_json = match stack.metadata.cached() {
        Some(m) => serde_json::json!({
            "exif_tags": m.exif_tags,
            "xmp_tags": m.xmp_tags,
            "custom_tags": m.custom_tags,
        }),
        None => serde_json::json!({
            "exif_tags": {},
            "xmp_tags": {},
            "custom_tags": {},
        }),
    };
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
        original: image_ref_to_c(&stack.original),
        enhanced: image_ref_to_c(&stack.enhanced),
        back: image_ref_to_c(&stack.back),
        metadata_json: metadata_json_ptr,
    }
}

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
/// - Caller owns the returned array and must call [`photostax_stack_array_free`]
///
/// [`photostax_repo_open`]: crate::repository::photostax_repo_open
/// [`photostax_stack_array_free`]: crate::repository::photostax_stack_array_free
#[no_mangle]
pub unsafe extern "C" fn photostax_search(
    repo: *const PhotostaxRepo,
    query_json: *const c_char,
) -> FfiPhotoStackArray {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if repo.is_null() || query_json.is_null() {
            return FfiPhotoStackArray::empty();
        }

        let repo_ref = unsafe { &*repo };
        let query_str = match unsafe { CStr::from_ptr(query_json) }.to_str() {
            Ok(s) => s,
            Err(_) => return FfiPhotoStackArray::empty(),
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
            Err(_) => return FfiPhotoStackArray::empty(),
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
        if mgr.rescan(None).is_err() {
            return FfiPhotoStackArray::empty();
        }
        let ids: Vec<String> = mgr.stacks().iter().map(|s| s.id.clone()).collect();
        for id in &ids {
            if let Some(s) = mgr.get_stack_mut(id) {
                let _ = s.metadata.read();
            }
        }

        // Apply the filter using query()
        let filtered = match mgr.query(Some(&query), None, None) {
            Ok(snap) => snap,
            Err(_) => return FfiPhotoStackArray::empty(),
        };
        drop(mgr);

        if filtered.all_stacks().is_empty() {
            return FfiPhotoStackArray::empty();
        }

        let ffi_stacks: Vec<FfiPhotoStack> = filtered
            .all_stacks()
            .iter()
            .map(photo_stack_to_ffi)
            .collect();
        let len = ffi_stacks.len();
        let boxed_slice = ffi_stacks.into_boxed_slice();
        let data = Box::into_raw(boxed_slice) as *mut FfiPhotoStack;

        FfiPhotoStackArray { data, len }
    }));

    result.unwrap_or_else(|_| FfiPhotoStackArray::empty())
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
/// - Caller owns the returned result and must call [`photostax_paginated_result_free`]
///
/// [`photostax_repo_open`]: crate::repository::photostax_repo_open
/// [`photostax_paginated_result_free`]: crate::repository::photostax_paginated_result_free
#[no_mangle]
pub unsafe extern "C" fn photostax_search_paginated(
    repo: *const PhotostaxRepo,
    query_json: *const c_char,
    offset: usize,
    limit: usize,
) -> FfiPaginatedResult {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if repo.is_null() || query_json.is_null() {
            return FfiPaginatedResult::empty(offset, limit);
        }

        let repo_ref = unsafe { &*repo };
        let query_str = match unsafe { CStr::from_ptr(query_json) }.to_str() {
            Ok(s) => s,
            Err(_) => return FfiPaginatedResult::empty(offset, limit),
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
            Err(_) => return FfiPaginatedResult::empty(offset, limit),
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
        if mgr.rescan(None).is_err() {
            return FfiPaginatedResult::empty(offset, limit);
        }
        let ids: Vec<String> = mgr.stacks().iter().map(|s| s.id.clone()).collect();
        for id in &ids {
            if let Some(s) = mgr.get_stack_mut(id) {
                let _ = s.metadata.read();
            }
        }

        let snapshot = match mgr.query(Some(&query), None, None) {
            Ok(snap) => snap,
            Err(_) => return FfiPaginatedResult::empty(offset, limit),
        };
        drop(mgr);

        let paginated = snapshot.snapshot().get_page(offset, limit);

        if paginated.items.is_empty() {
            return FfiPaginatedResult {
                data: std::ptr::null_mut(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::{
        photostax_paginated_result_free, photostax_repo_free, photostax_repo_open,
        photostax_stack_array_free,
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
        assert!(result.data.is_null());
        assert_eq!(result.len, 0);
    }

    #[test]
    fn test_search_null_query() {
        let path = CString::new(".").unwrap();
        let repo = unsafe { photostax_repo_open(path.as_ptr()) };
        assert!(!repo.is_null());

        let result = unsafe { photostax_search(repo, ptr::null()) };
        assert!(result.data.is_null());
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
        assert!(result.data.is_null());
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
        assert!(!result.data.is_null());

        // Verify first result has valid data
        let first = unsafe { &*result.data };
        assert!(!first.id.is_null());
        let id_str = unsafe { CStr::from_ptr(first.id) }.to_str().unwrap();
        assert!(!id_str.is_empty());

        unsafe { photostax_stack_array_free(result) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_search_with_text_query() {
        let repo = open_testdata_repo();
        let query = CString::new(r#"{"text_query":"FamilyPhotos"}"#).unwrap();
        let result = unsafe { photostax_search(repo, query.as_ptr()) };

        assert!(result.len > 0, "Should find FamilyPhotos stacks");

        unsafe { photostax_stack_array_free(result) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_search_with_text_no_match() {
        let repo = open_testdata_repo();
        let query = CString::new(r#"{"text_query":"zzz_nonexistent_zzz"}"#).unwrap();
        let result = unsafe { photostax_search(repo, query.as_ptr()) };

        assert_eq!(result.len, 0);
        assert!(result.data.is_null());

        unsafe { photostax_stack_array_free(result) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_search_with_has_back_filter() {
        let repo = open_testdata_repo();
        let query = CString::new(r#"{"has_back":true}"#).unwrap();
        let result = unsafe { photostax_search(repo, query.as_ptr()) };

        // Some stacks in testdata have _b files
        if result.len > 0 {
            let first = unsafe { &*result.data };
            assert!(!first.id.is_null());
        }

        unsafe { photostax_stack_array_free(result) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_search_with_has_enhanced_filter() {
        let repo = open_testdata_repo();
        let query = CString::new(r#"{"has_enhanced":true}"#).unwrap();
        let result = unsafe { photostax_search(repo, query.as_ptr()) };

        if result.len > 0 {
            let first = unsafe { &*result.data };
            assert!(!first.id.is_null());
        }

        unsafe { photostax_stack_array_free(result) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_search_with_exif_filter() {
        let repo = open_testdata_repo();
        let query = CString::new(r#"{"exif_filters":[["Make","EPSON"]]}"#).unwrap();
        let result = unsafe { photostax_search(repo, query.as_ptr()) };

        // Testdata has EPSON EXIF tags
        if result.len > 0 {
            let first = unsafe { &*result.data };
            assert!(!first.metadata_json.is_null());
        }

        unsafe { photostax_stack_array_free(result) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_search_with_custom_filter() {
        let repo = open_testdata_repo();
        let query = CString::new(r#"{"custom_filters":[["album","Family"]]}"#).unwrap();
        let result = unsafe { photostax_search(repo, query.as_ptr()) };
        // Likely no match but should not crash
        unsafe { photostax_stack_array_free(result) };
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
        unsafe { photostax_stack_array_free(result) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_search_photo_stack_to_ffi_covers_all_fields() {
        let stack = photostax_core::photo_stack::PhotoStack::new("search_test");

        let ffi = photo_stack_to_ffi(&stack);
        assert!(!ffi.id.is_null());
        // Stack created with new() has absent ImageRefs
        assert!(ffi.original.is_null());
        assert!(ffi.enhanced.is_null());
        assert!(ffi.back.is_null());
        assert!(!ffi.metadata_json.is_null());

        // Clean up
        unsafe {
            drop(CString::from_raw(ffi.id));
            if !ffi.name.is_null() {
                drop(CString::from_raw(ffi.name));
            }
            drop(CString::from_raw(ffi.metadata_json));
        }
    }

    #[test]
    fn test_search_invalid_utf8_query() {
        let repo = open_testdata_repo();
        let invalid: &[u8] = &[0xff, 0x00];
        let result = unsafe { photostax_search(repo, invalid.as_ptr() as *const c_char) };
        assert_eq!(result.len, 0);
        assert!(result.data.is_null());
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
        assert!(result.data.is_null());
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
        assert!(result.data.is_null());
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

        unsafe { photostax_paginated_result_free(page1) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_search_paginated_with_text_filter() {
        let repo = open_testdata_repo();
        let query = CString::new(r#"{"text_query":"FamilyPhotos"}"#).unwrap();

        let result = unsafe { photostax_search_paginated(repo, query.as_ptr(), 0, 100) };
        // total_count should reflect filtered set
        assert!(result.total_count >= result.len);

        unsafe { photostax_paginated_result_free(result) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_search_paginated_offset_beyond_end() {
        let repo = open_testdata_repo();
        let query = CString::new("{}").unwrap();

        let result = unsafe { photostax_search_paginated(repo, query.as_ptr(), 10000, 10) };
        assert_eq!(result.len, 0);
        assert!(result.data.is_null());
        // total_count is still the full count
        assert!(!result.has_more);

        unsafe { photostax_paginated_result_free(result) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_search_with_stack_ids_filter() {
        let repo = open_testdata_repo();
        // Get all stacks first to find valid IDs
        let all_query = CString::new("{}").unwrap();
        let all_result = unsafe { photostax_search(repo, all_query.as_ptr()) };
        assert!(all_result.len > 0, "Expected stacks from testdata");

        let first_id = unsafe { CStr::from_ptr((*all_result.data).id) }
            .to_str()
            .unwrap()
            .to_string();
        unsafe { photostax_stack_array_free(all_result) };

        // Search with stack_ids containing only the first ID
        let query_json = format!(r#"{{"stack_ids":["{}"]}}"#, first_id);
        let query = CString::new(query_json).unwrap();
        let result = unsafe { photostax_search(repo, query.as_ptr()) };

        assert_eq!(result.len, 1);
        let result_id = unsafe { CStr::from_ptr((*result.data).id) }
            .to_str()
            .unwrap();
        assert_eq!(result_id, first_id);

        unsafe { photostax_stack_array_free(result) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_search_with_stack_ids_no_match() {
        let repo = open_testdata_repo();
        let query = CString::new(r#"{"stack_ids":["NONEXISTENT_ID"]}"#).unwrap();
        let result = unsafe { photostax_search(repo, query.as_ptr()) };

        assert_eq!(result.len, 0);
        assert!(result.data.is_null());

        unsafe { photostax_stack_array_free(result) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_search_paginated_with_stack_ids() {
        let repo = open_testdata_repo();
        // Get all stacks first
        let all_query = CString::new("{}").unwrap();
        let all_result = unsafe { photostax_search(repo, all_query.as_ptr()) };
        assert!(all_result.len > 0);

        let first_id = unsafe { CStr::from_ptr((*all_result.data).id) }
            .to_str()
            .unwrap()
            .to_string();
        unsafe { photostax_stack_array_free(all_result) };

        // Paginated search with stack_ids
        let query_json = format!(r#"{{"stack_ids":["{}"]}}"#, first_id);
        let query = CString::new(query_json).unwrap();
        let result = unsafe { photostax_search_paginated(repo, query.as_ptr(), 0, 10) };

        assert_eq!(result.total_count, 1);
        assert_eq!(result.len, 1);
        assert!(!result.has_more);

        unsafe { photostax_paginated_result_free(result) };
        unsafe { photostax_repo_free(repo) };
    }
}

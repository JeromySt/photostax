//! Search FFI functions.
//!
//! Provides C-compatible access to photostax-core search functionality.

use std::ffi::CStr;
use std::os::raw::c_char;
use std::panic;

use photostax_core::repository::Repository;
use photostax_core::search::{filter_stacks, SearchQuery};
use serde::Deserialize;

use crate::types::{FfiPhotoStack, FfiPhotoStackArray, PhotostaxRepo};

/// Helper to convert a PhotoStack to an FfiPhotoStack.
fn photo_stack_to_ffi(stack: &photostax_core::photo_stack::PhotoStack) -> FfiPhotoStack {
    use std::ffi::CString;
    use std::ptr;

    let id = CString::new(stack.id.clone())
        .map(|s| s.into_raw())
        .unwrap_or(ptr::null_mut());

    let path_to_c_string = |path: &Option<std::path::PathBuf>| -> *mut c_char {
        match path {
            Some(p) => {
                let s = p.to_string_lossy().into_owned();
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
        original: path_to_c_string(&stack.original),
        enhanced: path_to_c_string(&stack.enhanced),
        back: path_to_c_string(&stack.back),
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
    let result = panic::catch_unwind(|| {
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

        // Get all stacks first
        let stacks = match repo_ref.inner.scan() {
            Ok(s) => s,
            Err(_) => return FfiPhotoStackArray::empty(),
        };

        // Apply the filter
        let filtered = filter_stacks(&stacks, &query);

        if filtered.is_empty() {
            return FfiPhotoStackArray::empty();
        }

        let ffi_stacks: Vec<FfiPhotoStack> = filtered.iter().map(photo_stack_to_ffi).collect();
        let len = ffi_stacks.len();
        let boxed_slice = ffi_stacks.into_boxed_slice();
        let data = Box::into_raw(boxed_slice) as *mut FfiPhotoStack;

        FfiPhotoStackArray { data, len }
    });

    result.unwrap_or_else(|_| FfiPhotoStackArray::empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::{photostax_repo_free, photostax_repo_open, photostax_stack_array_free};
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
        let mut metadata = photostax_core::photo_stack::Metadata::default();
        metadata
            .exif_tags
            .insert("Make".to_string(), "Canon".to_string());
        metadata
            .xmp_tags
            .insert("Creator".to_string(), "Test".to_string());
        metadata
            .custom_tags
            .insert("rating".to_string(), serde_json::json!(5));

        let stack = photostax_core::photo_stack::PhotoStack {
            id: "search_test".to_string(),
            original: Some(std::path::PathBuf::from("/test/original.jpg")),
            enhanced: None,
            back: Some(std::path::PathBuf::from("/test/back.jpg")),
            metadata,
        };

        let ffi = photo_stack_to_ffi(&stack);
        assert!(!ffi.id.is_null());
        assert!(!ffi.original.is_null());
        assert!(ffi.enhanced.is_null());
        assert!(!ffi.back.is_null());
        assert!(!ffi.metadata_json.is_null());

        let meta_str = unsafe { CStr::from_ptr(ffi.metadata_json) }
            .to_str()
            .unwrap();
        assert!(meta_str.contains("Canon"));
        assert!(meta_str.contains("Creator"));

        // Clean up
        unsafe {
            drop(CString::from_raw(ffi.id));
            drop(CString::from_raw(ffi.original));
            drop(CString::from_raw(ffi.back));
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
}

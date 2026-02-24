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
                CString::new(s).map(|cs| cs.into_raw()).unwrap_or(ptr::null_mut())
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
    use std::ffi::CString;
    use std::ptr;

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
    fn test_search_empty_query() {
        let path = CString::new(".").unwrap();
        let repo = unsafe { photostax_repo_open(path.as_ptr()) };
        assert!(!repo.is_null());

        // Empty query - should return all stacks (but we're in repo root with no photos)
        let query = CString::new("{}").unwrap();
        let result = unsafe { photostax_search(repo, query.as_ptr()) };
        // Will be empty since no photo files in the repo root
        unsafe { photostax_stack_array_free(result) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_search_with_filters() {
        let path = CString::new(".").unwrap();
        let repo = unsafe { photostax_repo_open(path.as_ptr()) };
        assert!(!repo.is_null());

        let query = CString::new(r#"{
            "exif_filters": [["Make", "EPSON"]],
            "has_back": true,
            "text_query": "birthday"
        }"#).unwrap();
        let result = unsafe { photostax_search(repo, query.as_ptr()) };
        // Will be empty since no photo files match
        unsafe { photostax_stack_array_free(result) };
        unsafe { photostax_repo_free(repo) };
    }
}

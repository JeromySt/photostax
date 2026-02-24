//! Metadata FFI functions.
//!
//! Provides C-compatible access to metadata reading and manipulation.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::panic;
use std::ptr;

use photostax_core::repository::Repository;

use crate::types::{FfiResult, PhotostaxRepo};

/// Get metadata for a stack as a JSON string.
///
/// Returns a JSON object with `exif_tags`, `xmp_tags`, and `custom_tags` fields.
///
/// # Safety
///
/// - `repo` must be a valid pointer from [`photostax_repo_open`]
/// - `stack_id` must be a valid null-terminated UTF-8 string
/// - Returns null on error
/// - Caller owns the returned string and must call [`photostax_string_free`]
///
/// [`photostax_repo_open`]: crate::repository::photostax_repo_open
/// [`photostax_string_free`]: crate::repository::photostax_string_free
#[no_mangle]
pub unsafe extern "C" fn photostax_get_metadata(
    repo: *const PhotostaxRepo,
    stack_id: *const c_char,
) -> *mut c_char {
    let result = panic::catch_unwind(|| {
        if repo.is_null() || stack_id.is_null() {
            return ptr::null_mut();
        }

        let repo_ref = unsafe { &*repo };
        let stack_id_str = match unsafe { CStr::from_ptr(stack_id) }.to_str() {
            Ok(s) => s,
            Err(_) => return ptr::null_mut(),
        };

        let stack = match repo_ref.inner.get_stack(stack_id_str) {
            Ok(s) => s,
            Err(_) => return ptr::null_mut(),
        };

        let metadata_json = serde_json::json!({
            "exif_tags": stack.metadata.exif_tags,
            "xmp_tags": stack.metadata.xmp_tags,
            "custom_tags": stack.metadata.custom_tags,
        });

        let json_str = serde_json::to_string_pretty(&metadata_json).unwrap_or_else(|_| "{}".to_string());
        CString::new(json_str).map(|s| s.into_raw()).unwrap_or(ptr::null_mut())
    });

    result.unwrap_or(ptr::null_mut())
}

/// Get a specific EXIF tag value.
///
/// # Safety
///
/// - `repo` must be a valid pointer from [`photostax_repo_open`]
/// - `stack_id` and `tag_name` must be valid null-terminated UTF-8 strings
/// - Returns null if tag not found or on error
/// - Caller owns the returned string and must call [`photostax_string_free`]
///
/// [`photostax_repo_open`]: crate::repository::photostax_repo_open
/// [`photostax_string_free`]: crate::repository::photostax_string_free
#[no_mangle]
pub unsafe extern "C" fn photostax_get_exif_tag(
    repo: *const PhotostaxRepo,
    stack_id: *const c_char,
    tag_name: *const c_char,
) -> *mut c_char {
    let result = panic::catch_unwind(|| {
        if repo.is_null() || stack_id.is_null() || tag_name.is_null() {
            return ptr::null_mut();
        }

        let repo_ref = unsafe { &*repo };
        let stack_id_str = match unsafe { CStr::from_ptr(stack_id) }.to_str() {
            Ok(s) => s,
            Err(_) => return ptr::null_mut(),
        };
        let tag_name_str = match unsafe { CStr::from_ptr(tag_name) }.to_str() {
            Ok(s) => s,
            Err(_) => return ptr::null_mut(),
        };

        let stack = match repo_ref.inner.get_stack(stack_id_str) {
            Ok(s) => s,
            Err(_) => return ptr::null_mut(),
        };

        match stack.metadata.exif_tags.get(tag_name_str) {
            Some(value) => CString::new(value.as_str()).map(|s| s.into_raw()).unwrap_or(ptr::null_mut()),
            None => ptr::null_mut(),
        }
    });

    result.unwrap_or(ptr::null_mut())
}

/// Get a specific custom tag value as JSON.
///
/// # Safety
///
/// - `repo` must be a valid pointer from [`photostax_repo_open`]
/// - `stack_id` and `tag_name` must be valid null-terminated UTF-8 strings
/// - Returns null if tag not found or on error
/// - Caller owns the returned string and must call [`photostax_string_free`]
///
/// [`photostax_repo_open`]: crate::repository::photostax_repo_open
/// [`photostax_string_free`]: crate::repository::photostax_string_free
#[no_mangle]
pub unsafe extern "C" fn photostax_get_custom_tag(
    repo: *const PhotostaxRepo,
    stack_id: *const c_char,
    tag_name: *const c_char,
) -> *mut c_char {
    let result = panic::catch_unwind(|| {
        if repo.is_null() || stack_id.is_null() || tag_name.is_null() {
            return ptr::null_mut();
        }

        let repo_ref = unsafe { &*repo };
        let stack_id_str = match unsafe { CStr::from_ptr(stack_id) }.to_str() {
            Ok(s) => s,
            Err(_) => return ptr::null_mut(),
        };
        let tag_name_str = match unsafe { CStr::from_ptr(tag_name) }.to_str() {
            Ok(s) => s,
            Err(_) => return ptr::null_mut(),
        };

        let stack = match repo_ref.inner.get_stack(stack_id_str) {
            Ok(s) => s,
            Err(_) => return ptr::null_mut(),
        };

        match stack.metadata.custom_tags.get(tag_name_str) {
            Some(value) => {
                let json_str = serde_json::to_string(value).unwrap_or_else(|_| "null".to_string());
                CString::new(json_str).map(|s| s.into_raw()).unwrap_or(ptr::null_mut())
            }
            None => ptr::null_mut(),
        }
    });

    result.unwrap_or(ptr::null_mut())
}

/// Set a custom tag value.
///
/// # Safety
///
/// - `repo` must be a valid pointer from [`photostax_repo_open`]
/// - `stack_id`, `tag_name`, and `value_json` must be valid null-terminated UTF-8 strings
/// - `value_json` must be valid JSON
///
/// [`photostax_repo_open`]: crate::repository::photostax_repo_open
#[no_mangle]
pub unsafe extern "C" fn photostax_set_custom_tag(
    repo: *const PhotostaxRepo,
    stack_id: *const c_char,
    tag_name: *const c_char,
    value_json: *const c_char,
) -> FfiResult {
    let result = panic::catch_unwind(|| {
        if repo.is_null() {
            return FfiResult::error("Repository pointer is null");
        }
        if stack_id.is_null() {
            return FfiResult::error("Stack ID pointer is null");
        }
        if tag_name.is_null() {
            return FfiResult::error("Tag name pointer is null");
        }
        if value_json.is_null() {
            return FfiResult::error("Value JSON pointer is null");
        }

        let repo_ref = unsafe { &*repo };
        let stack_id_str = match unsafe { CStr::from_ptr(stack_id) }.to_str() {
            Ok(s) => s,
            Err(_) => return FfiResult::error("Invalid UTF-8 in stack ID"),
        };
        let tag_name_str = match unsafe { CStr::from_ptr(tag_name) }.to_str() {
            Ok(s) => s,
            Err(_) => return FfiResult::error("Invalid UTF-8 in tag name"),
        };
        let value_str = match unsafe { CStr::from_ptr(value_json) }.to_str() {
            Ok(s) => s,
            Err(_) => return FfiResult::error("Invalid UTF-8 in value JSON"),
        };

        // Parse the value JSON
        let value: serde_json::Value = match serde_json::from_str(value_str) {
            Ok(v) => v,
            Err(e) => return FfiResult::error(&format!("Invalid JSON value: {e}")),
        };

        // Get the stack first
        let stack = match repo_ref.inner.get_stack(stack_id_str) {
            Ok(s) => s,
            Err(e) => return FfiResult::error(&format!("Failed to get stack: {e}")),
        };

        // Create metadata with just the one custom tag
        let mut custom_tags = std::collections::HashMap::new();
        custom_tags.insert(tag_name_str.to_string(), value);

        let metadata = photostax_core::photo_stack::Metadata {
            exif_tags: std::collections::HashMap::new(),
            xmp_tags: std::collections::HashMap::new(),
            custom_tags,
        };

        match repo_ref.inner.write_metadata(&stack, &metadata) {
            Ok(()) => FfiResult::success(),
            Err(e) => FfiResult::error(&e.to_string()),
        }
    });

    result.unwrap_or_else(|_| FfiResult::error("Panic occurred"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;
    use std::ptr;

    #[test]
    fn test_get_metadata_null_pointers() {
        let result = unsafe { photostax_get_metadata(ptr::null(), ptr::null()) };
        assert!(result.is_null());
    }

    #[test]
    fn test_get_metadata_null_stack_id() {
        let path = CString::new(".").unwrap();
        let repo = unsafe { crate::repository::photostax_repo_open(path.as_ptr()) };
        assert!(!repo.is_null());

        let result = unsafe { photostax_get_metadata(repo, ptr::null()) };
        assert!(result.is_null());

        unsafe { crate::repository::photostax_repo_free(repo) };
    }

    #[test]
    fn test_get_exif_tag_null_pointers() {
        let result = unsafe { photostax_get_exif_tag(ptr::null(), ptr::null(), ptr::null()) };
        assert!(result.is_null());
    }

    #[test]
    fn test_get_custom_tag_null_pointers() {
        let result = unsafe { photostax_get_custom_tag(ptr::null(), ptr::null(), ptr::null()) };
        assert!(result.is_null());
    }

    #[test]
    fn test_set_custom_tag_null_pointers() {
        let result = unsafe { photostax_set_custom_tag(ptr::null(), ptr::null(), ptr::null(), ptr::null()) };
        assert!(!result.success);
        assert!(!result.error_message.is_null());
        unsafe { crate::repository::photostax_string_free(result.error_message) };
    }

    #[test]
    fn test_set_custom_tag_invalid_json() {
        let path = CString::new(".").unwrap();
        let repo = unsafe { crate::repository::photostax_repo_open(path.as_ptr()) };
        assert!(!repo.is_null());

        let stack_id = CString::new("nonexistent").unwrap();
        let tag_name = CString::new("test_tag").unwrap();
        let value = CString::new("not valid json").unwrap();

        let result = unsafe { photostax_set_custom_tag(repo, stack_id.as_ptr(), tag_name.as_ptr(), value.as_ptr()) };
        assert!(!result.success);
        assert!(!result.error_message.is_null());
        unsafe { crate::repository::photostax_string_free(result.error_message) };

        unsafe { crate::repository::photostax_repo_free(repo) };
    }
}

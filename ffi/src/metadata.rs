//! Metadata FFI functions.
//!
//! Provides C-compatible access to metadata reading and manipulation.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::panic::{self, AssertUnwindSafe};
use std::ptr;

use crate::types::{FfiResult, PhotostaxStack};

/// Get metadata for a stack as a JSON string.
///
/// Returns a JSON object with `exif_tags`, `xmp_tags`, and `custom_tags` fields.
///
/// # Safety
///
/// - `stack` must be a valid pointer from [`photostax_repo_get_stack`] or a handle array
/// - Returns null on error
/// - Caller owns the returned string and must call [`photostax_string_free`]
///
/// [`photostax_repo_get_stack`]: crate::repository::photostax_repo_get_stack
/// [`photostax_string_free`]: crate::repository::photostax_string_free
#[no_mangle]
pub unsafe extern "C" fn photostax_get_metadata(stack: *const PhotostaxStack) -> *mut c_char {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if stack.is_null() {
            return ptr::null_mut();
        }

        let stack_ref = unsafe { &*stack };

        stack_ref.runtime.block_on(async {
            let metadata = match stack_ref.inner.metadata().read().await {
                Ok(m) => m,
                Err(_) => return ptr::null_mut(),
            };

            let metadata_json = serde_json::json!({
                "exif_tags": metadata.exif_tags,
                "xmp_tags": metadata.xmp_tags,
                "custom_tags": metadata.custom_tags,
            });

            let json_str =
                serde_json::to_string_pretty(&metadata_json).unwrap_or_else(|_| "{}".to_string());
            CString::new(json_str)
                .map(|s| s.into_raw())
                .unwrap_or(ptr::null_mut())
        })
    }));

    result.unwrap_or(ptr::null_mut())
}

/// Get a specific EXIF tag value.
///
/// # Safety
///
/// - `stack` must be a valid pointer from [`photostax_repo_get_stack`] or a handle array
/// - `tag_name` must be a valid null-terminated UTF-8 string
/// - Returns null if tag not found or on error
/// - Caller owns the returned string and must call [`photostax_string_free`]
///
/// [`photostax_repo_get_stack`]: crate::repository::photostax_repo_get_stack
/// [`photostax_string_free`]: crate::repository::photostax_string_free
#[no_mangle]
pub unsafe extern "C" fn photostax_get_exif_tag(
    stack: *const PhotostaxStack,
    tag_name: *const c_char,
) -> *mut c_char {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if stack.is_null() || tag_name.is_null() {
            return ptr::null_mut();
        }

        let tag_name_str = match unsafe { CStr::from_ptr(tag_name) }.to_str() {
            Ok(s) => s,
            Err(_) => return ptr::null_mut(),
        };

        let stack_ref = unsafe { &*stack };

        stack_ref.runtime.block_on(async {
            let metadata = match stack_ref.inner.metadata().read().await {
                Ok(m) => m,
                Err(_) => return ptr::null_mut(),
            };

            match metadata.exif_tags.get(tag_name_str) {
                Some(value) => CString::new(value.as_str())
                    .map(|s| s.into_raw())
                    .unwrap_or(ptr::null_mut()),
                None => ptr::null_mut(),
            }
        })
    }));

    result.unwrap_or(ptr::null_mut())
}

/// Get a specific custom tag value as JSON.
///
/// # Safety
///
/// - `stack` must be a valid pointer from [`photostax_repo_get_stack`] or a handle array
/// - `tag_name` must be a valid null-terminated UTF-8 string
/// - Returns null if tag not found or on error
/// - Caller owns the returned string and must call [`photostax_string_free`]
///
/// [`photostax_repo_get_stack`]: crate::repository::photostax_repo_get_stack
/// [`photostax_string_free`]: crate::repository::photostax_string_free
#[no_mangle]
pub unsafe extern "C" fn photostax_get_custom_tag(
    stack: *const PhotostaxStack,
    tag_name: *const c_char,
) -> *mut c_char {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if stack.is_null() || tag_name.is_null() {
            return ptr::null_mut();
        }

        let tag_name_str = match unsafe { CStr::from_ptr(tag_name) }.to_str() {
            Ok(s) => s,
            Err(_) => return ptr::null_mut(),
        };

        let stack_ref = unsafe { &*stack };

        stack_ref.runtime.block_on(async {
            let metadata = match stack_ref.inner.metadata().read().await {
                Ok(m) => m,
                Err(_) => return ptr::null_mut(),
            };

            match metadata.custom_tags.get(tag_name_str) {
                Some(value) => {
                    let json_str = serde_json::to_string(value).unwrap_or_else(|_| "null".to_string());
                    CString::new(json_str)
                        .map(|s| s.into_raw())
                        .unwrap_or(ptr::null_mut())
                }
                None => ptr::null_mut(),
            }
        })
    }));

    result.unwrap_or(ptr::null_mut())
}

/// Set a custom tag value.
///
/// # Safety
///
/// - `stack` must be a valid pointer from [`photostax_repo_get_stack`] or a handle array
/// - `tag_name` and `value_json` must be valid null-terminated UTF-8 strings
/// - `value_json` must be valid JSON
///
/// [`photostax_repo_get_stack`]: crate::repository::photostax_repo_get_stack
#[no_mangle]
pub unsafe extern "C" fn photostax_set_custom_tag(
    stack: *const PhotostaxStack,
    tag_name: *const c_char,
    value_json: *const c_char,
) -> FfiResult {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if stack.is_null() {
            return FfiResult::error("Stack pointer is null");
        }
        if tag_name.is_null() {
            return FfiResult::error("Tag name pointer is null");
        }
        if value_json.is_null() {
            return FfiResult::error("Value JSON pointer is null");
        }

        let tag_name_str = match unsafe { CStr::from_ptr(tag_name) }.to_str() {
            Ok(s) => s,
            Err(_) => return FfiResult::error("Invalid UTF-8 in tag name"),
        };
        let value_str = match unsafe { CStr::from_ptr(value_json) }.to_str() {
            Ok(s) => s,
            Err(_) => return FfiResult::error("Invalid UTF-8 in value JSON"),
        };

        let value: serde_json::Value = match serde_json::from_str(value_str) {
            Ok(v) => v,
            Err(e) => return FfiResult::error(&format!("Invalid JSON value: {e}")),
        };

        let mut custom_tags = std::collections::HashMap::new();
        custom_tags.insert(tag_name_str.to_string(), value);

        let metadata = photostax_core::photo_stack::Metadata {
            exif_tags: std::collections::HashMap::new(),
            xmp_tags: std::collections::HashMap::new(),
            custom_tags,
        };

        let stack_ref = unsafe { &*stack };

        stack_ref.runtime.block_on(async {
            match stack_ref.inner.metadata().write(&metadata).await {
                Ok(()) => FfiResult::success(),
                Err(e) => FfiResult::error(&e.to_string()),
            }
        })
    }));

    result.unwrap_or_else(|_| FfiResult::error("Panic occurred"))
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let repo = unsafe { crate::repository::photostax_repo_open(path.as_ptr()) };
        assert!(!repo.is_null());
        repo
    }

    fn get_stack_by_name(
        repo: *const crate::types::PhotostaxRepo,
        name: &str,
    ) -> *mut PhotostaxStack {
        let array = unsafe { crate::repository::photostax_repo_scan(repo) };
        assert!(array.len > 0);
        let slice = unsafe { std::slice::from_raw_parts(array.handles, array.len) };
        let mut found: Option<*mut PhotostaxStack> = None;
        for &handle in slice {
            let name_ptr = unsafe { crate::repository::photostax_stack_name(handle) };
            let n = unsafe { CStr::from_ptr(name_ptr) }
                .to_str()
                .unwrap()
                .to_string();
            unsafe { crate::repository::photostax_string_free(name_ptr) };
            if n == name {
                // Get the stack's ID so we can fetch it via photostax_repo_get_stack
                let id_ptr = unsafe { crate::repository::photostax_stack_id(handle) };
                let id_cstr = CString::new(
                    unsafe { CStr::from_ptr(id_ptr) }
                        .to_str()
                        .unwrap()
                        .to_string(),
                )
                .unwrap();
                unsafe { crate::repository::photostax_string_free(id_ptr) };
                let stack =
                    unsafe { crate::repository::photostax_repo_get_stack(repo, id_cstr.as_ptr()) };
                assert!(!stack.is_null(), "stack not found by name");
                found = Some(stack);
                break;
            }
        }
        unsafe { crate::repository::photostax_stack_handle_array_free(array) };
        found.expect("stack not found by name")
    }

    #[test]
    fn test_get_metadata_null_pointer() {
        let result = unsafe { photostax_get_metadata(ptr::null()) };
        assert!(result.is_null());
    }

    #[test]
    fn test_get_metadata_happy_path() {
        let repo = open_testdata_repo();
        let stack = get_stack_by_name(repo, "FamilyPhotos_0001");
        let result = unsafe { photostax_get_metadata(stack) };
        assert!(
            !result.is_null(),
            "Should get metadata for FamilyPhotos_0001"
        );

        let meta_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
        assert!(meta_str.contains("exif_tags"));
        assert!(meta_str.contains("xmp_tags"));
        assert!(meta_str.contains("custom_tags"));

        unsafe { crate::repository::photostax_string_free(result) };
        unsafe { crate::repository::photostax_stack_free(stack) };
        unsafe { crate::repository::photostax_repo_free(repo) };
    }

    #[test]
    fn test_get_exif_tag_null_pointers() {
        let result = unsafe { photostax_get_exif_tag(ptr::null(), ptr::null()) };
        assert!(result.is_null());
    }

    #[test]
    fn test_get_exif_tag_null_tag_name() {
        let repo = open_testdata_repo();
        let stack = get_stack_by_name(repo, "FamilyPhotos_0001");
        let result = unsafe { photostax_get_exif_tag(stack, ptr::null()) };
        assert!(result.is_null());
        unsafe { crate::repository::photostax_stack_free(stack) };
        unsafe { crate::repository::photostax_repo_free(repo) };
    }

    #[test]
    fn test_get_exif_tag_happy_path() {
        let repo = open_testdata_repo();
        let stack = get_stack_by_name(repo, "FamilyPhotos_0001");
        let tag = CString::new("Make").unwrap();
        let result = unsafe { photostax_get_exif_tag(stack, tag.as_ptr()) };

        // Testdata JPEG files have Make=EPSON
        if !result.is_null() {
            let tag_val = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
            assert!(tag_val.contains("EPSON"));
            unsafe { crate::repository::photostax_string_free(result) };
        }

        unsafe { crate::repository::photostax_stack_free(stack) };
        unsafe { crate::repository::photostax_repo_free(repo) };
    }

    #[test]
    fn test_get_exif_tag_nonexistent_tag() {
        let repo = open_testdata_repo();
        let stack = get_stack_by_name(repo, "FamilyPhotos_0001");
        let tag = CString::new("NonexistentTag").unwrap();
        let result = unsafe { photostax_get_exif_tag(stack, tag.as_ptr()) };
        assert!(result.is_null());
        unsafe { crate::repository::photostax_stack_free(stack) };
        unsafe { crate::repository::photostax_repo_free(repo) };
    }

    #[test]
    fn test_get_custom_tag_null_pointers() {
        let result = unsafe { photostax_get_custom_tag(ptr::null(), ptr::null()) };
        assert!(result.is_null());
    }

    #[test]
    fn test_get_custom_tag_nonexistent_tag() {
        let repo = open_testdata_repo();
        let stack = get_stack_by_name(repo, "FamilyPhotos_0001");
        let tag = CString::new("nonexistent_tag").unwrap();
        let result = unsafe { photostax_get_custom_tag(stack, tag.as_ptr()) };
        assert!(result.is_null());
        unsafe { crate::repository::photostax_stack_free(stack) };
        unsafe { crate::repository::photostax_repo_free(repo) };
    }

    #[test]
    fn test_set_custom_tag_null_pointers() {
        let result = unsafe { photostax_set_custom_tag(ptr::null(), ptr::null(), ptr::null()) };
        assert!(!result.success);
        assert!(!result.error_message.is_null());
        unsafe { crate::repository::photostax_string_free(result.error_message) };
    }

    #[test]
    fn test_set_custom_tag_null_stack() {
        let tag = CString::new("album").unwrap();
        let value = CString::new(r#""Family""#).unwrap();
        let result = unsafe { photostax_set_custom_tag(ptr::null(), tag.as_ptr(), value.as_ptr()) };
        assert!(!result.success);
        unsafe { crate::repository::photostax_string_free(result.error_message) };
    }

    #[test]
    fn test_set_custom_tag_null_tag_name() {
        let repo = open_testdata_repo();
        let stack = get_stack_by_name(repo, "FamilyPhotos_0001");
        let value = CString::new(r#""test""#).unwrap();
        let result = unsafe { photostax_set_custom_tag(stack, ptr::null(), value.as_ptr()) };
        assert!(!result.success);
        unsafe { crate::repository::photostax_string_free(result.error_message) };
        unsafe { crate::repository::photostax_stack_free(stack) };
        unsafe { crate::repository::photostax_repo_free(repo) };
    }

    #[test]
    fn test_set_custom_tag_null_value() {
        let repo = open_testdata_repo();
        let stack = get_stack_by_name(repo, "FamilyPhotos_0001");
        let tag = CString::new("album").unwrap();
        let result = unsafe { photostax_set_custom_tag(stack, tag.as_ptr(), ptr::null()) };
        assert!(!result.success);
        unsafe { crate::repository::photostax_string_free(result.error_message) };
        unsafe { crate::repository::photostax_stack_free(stack) };
        unsafe { crate::repository::photostax_repo_free(repo) };
    }

    #[test]
    fn test_set_custom_tag_invalid_json() {
        let repo = open_testdata_repo();
        let stack = get_stack_by_name(repo, "FamilyPhotos_0001");
        let tag_name = CString::new("test_tag").unwrap();
        let value = CString::new("not valid json").unwrap();

        let result = unsafe { photostax_set_custom_tag(stack, tag_name.as_ptr(), value.as_ptr()) };
        assert!(!result.success);
        assert!(!result.error_message.is_null());
        unsafe { crate::repository::photostax_string_free(result.error_message) };

        unsafe { crate::repository::photostax_stack_free(stack) };
        unsafe { crate::repository::photostax_repo_free(repo) };
    }

    #[test]
    fn test_set_custom_tag_happy_path() {
        // Copy testdata to temp dir
        let dir = tempfile::tempdir().unwrap();
        for entry in std::fs::read_dir(testdata_path()).unwrap() {
            let entry = entry.unwrap();
            std::fs::copy(entry.path(), dir.path().join(entry.file_name())).unwrap();
        }

        let path = CString::new(dir.path().to_str().unwrap()).unwrap();
        let repo = unsafe { crate::repository::photostax_repo_open(path.as_ptr()) };
        assert!(!repo.is_null());

        let stack = get_stack_by_name(repo, "FamilyPhotos_0001");
        let tag = CString::new("album").unwrap();
        let value = CString::new(r#""Family Vacation""#).unwrap();
        let result = unsafe { photostax_set_custom_tag(stack, tag.as_ptr(), value.as_ptr()) };
        assert!(result.success, "set_custom_tag should succeed");

        // Verify the tag was set by reading it back
        let read_result = unsafe { photostax_get_custom_tag(stack, tag.as_ptr()) };
        assert!(
            !read_result.is_null(),
            "Should be able to read back the tag"
        );
        let tag_val = unsafe { CStr::from_ptr(read_result) }.to_str().unwrap();
        assert!(tag_val.contains("Family Vacation"));
        unsafe { crate::repository::photostax_string_free(read_result) };

        unsafe { crate::repository::photostax_stack_free(stack) };
        unsafe { crate::repository::photostax_repo_free(repo) };
    }

    // ======================== Invalid UTF-8 tests ========================

    #[test]
    fn test_get_exif_tag_invalid_utf8_tag_name() {
        let repo = open_testdata_repo();
        let stack = get_stack_by_name(repo, "FamilyPhotos_0001");
        let invalid: &[u8] = &[0xff, 0x00];
        let result = unsafe { photostax_get_exif_tag(stack, invalid.as_ptr() as *const c_char) };
        assert!(result.is_null());
        unsafe { crate::repository::photostax_stack_free(stack) };
        unsafe { crate::repository::photostax_repo_free(repo) };
    }

    #[test]
    fn test_get_custom_tag_invalid_utf8_tag_name() {
        let repo = open_testdata_repo();
        let stack = get_stack_by_name(repo, "FamilyPhotos_0001");
        let invalid: &[u8] = &[0xff, 0x00];
        let result = unsafe { photostax_get_custom_tag(stack, invalid.as_ptr() as *const c_char) };
        assert!(result.is_null());
        unsafe { crate::repository::photostax_stack_free(stack) };
        unsafe { crate::repository::photostax_repo_free(repo) };
    }

    #[test]
    fn test_set_custom_tag_invalid_utf8_tag_name() {
        let repo = open_testdata_repo();
        let stack = get_stack_by_name(repo, "FamilyPhotos_0001");
        let invalid: &[u8] = &[0xff, 0x00];
        let val = CString::new(r#""val""#).unwrap();
        let result = unsafe {
            photostax_set_custom_tag(stack, invalid.as_ptr() as *const c_char, val.as_ptr())
        };
        assert!(!result.success);
        unsafe { crate::repository::photostax_string_free(result.error_message) };
        unsafe { crate::repository::photostax_stack_free(stack) };
        unsafe { crate::repository::photostax_repo_free(repo) };
    }

    #[test]
    fn test_set_custom_tag_invalid_utf8_value() {
        let repo = open_testdata_repo();
        let stack = get_stack_by_name(repo, "FamilyPhotos_0001");
        let tag = CString::new("key").unwrap();
        let invalid: &[u8] = &[0xff, 0x00];
        let result = unsafe {
            photostax_set_custom_tag(stack, tag.as_ptr(), invalid.as_ptr() as *const c_char)
        };
        assert!(!result.success);
        unsafe { crate::repository::photostax_string_free(result.error_message) };
        unsafe { crate::repository::photostax_stack_free(stack) };
        unsafe { crate::repository::photostax_repo_free(repo) };
    }
}

//! Repository-related FFI functions.
//!
//! These functions provide C-compatible access to the photostax-core repository
//! functionality. All functions handle panics and null pointer checks.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::panic;
use std::path::PathBuf;
use std::ptr;

use photostax_core::backends::local::LocalRepository;
use photostax_core::photo_stack::PhotoStack;
use photostax_core::repository::Repository;
use serde::Deserialize;

use crate::types::{FfiPhotoStack, FfiPhotoStackArray, FfiResult, PhotostaxRepo};

/// Helper to convert a PathBuf option to a C string (null if None).
fn path_to_c_string(path: &Option<PathBuf>) -> *mut c_char {
    match path {
        Some(p) => {
            let s = p.to_string_lossy().into_owned();
            match CString::new(s) {
                Ok(cs) => cs.into_raw(),
                Err(_) => ptr::null_mut(),
            }
        }
        None => ptr::null_mut(),
    }
}

/// Convert a PhotoStack to an FfiPhotoStack.
fn photo_stack_to_ffi(stack: &PhotoStack) -> FfiPhotoStack {
    let id = CString::new(stack.id.clone())
        .map(|s| s.into_raw())
        .unwrap_or(ptr::null_mut());

    let metadata_json = serde_json::json!({
        "exif_tags": stack.metadata.exif_tags,
        "xmp_tags": stack.metadata.xmp_tags,
        "custom_tags": stack.metadata.custom_tags,
    });
    let metadata_str = serde_json::to_string(&metadata_json).unwrap_or_else(|_| "{}".to_string());
    let metadata_json_ptr = CString::new(metadata_str)
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

/// Create a new repository from a directory path.
///
/// # Safety
///
/// - `path` must be a valid null-terminated UTF-8 string
/// - Returns null if `path` is null or invalid
/// - Caller owns the returned pointer and must call [`photostax_repo_free`]
#[no_mangle]
pub unsafe extern "C" fn photostax_repo_open(path: *const c_char) -> *mut PhotostaxRepo {
    let result = panic::catch_unwind(|| {
        if path.is_null() {
            return ptr::null_mut();
        }

        let c_str = unsafe { CStr::from_ptr(path) };
        let path_str = match c_str.to_str() {
            Ok(s) => s,
            Err(_) => return ptr::null_mut(),
        };

        let repo = LocalRepository::new(PathBuf::from(path_str));
        let boxed = Box::new(PhotostaxRepo { inner: repo });
        Box::into_raw(boxed)
    });

    result.unwrap_or(ptr::null_mut())
}

/// Free a repository handle.
///
/// # Safety
///
/// - `repo` must be a pointer returned by [`photostax_repo_open`], or null
/// - After calling, `repo` is invalid and must not be used
#[no_mangle]
pub unsafe extern "C" fn photostax_repo_free(repo: *mut PhotostaxRepo) {
    let _ = panic::catch_unwind(|| {
        if !repo.is_null() {
            drop(unsafe { Box::from_raw(repo) });
        }
    });
}

/// Scan the repository and return all photo stacks.
///
/// # Safety
///
/// - `repo` must be a valid pointer from [`photostax_repo_open`]
/// - Returns empty array if `repo` is null or scan fails
/// - Caller owns the returned array and must call [`photostax_stack_array_free`]
#[no_mangle]
pub unsafe extern "C" fn photostax_repo_scan(repo: *const PhotostaxRepo) -> FfiPhotoStackArray {
    let result = panic::catch_unwind(|| {
        if repo.is_null() {
            return FfiPhotoStackArray::empty();
        }

        let repo_ref = unsafe { &*repo };
        match repo_ref.inner.scan() {
            Ok(stacks) => {
                if stacks.is_empty() {
                    return FfiPhotoStackArray::empty();
                }

                let ffi_stacks: Vec<FfiPhotoStack> =
                    stacks.iter().map(photo_stack_to_ffi).collect();
                let len = ffi_stacks.len();
                let boxed_slice = ffi_stacks.into_boxed_slice();
                let data = Box::into_raw(boxed_slice) as *mut FfiPhotoStack;

                FfiPhotoStackArray { data, len }
            }
            Err(_) => FfiPhotoStackArray::empty(),
        }
    });

    result.unwrap_or_else(|_| FfiPhotoStackArray::empty())
}

/// Get a single stack by ID.
///
/// # Safety
///
/// - `repo` must be a valid pointer from [`photostax_repo_open`]
/// - `id` must be a valid null-terminated UTF-8 string
/// - Returns null if not found or on error
/// - Caller owns the returned pointer and must call [`photostax_stack_free`]
#[no_mangle]
pub unsafe extern "C" fn photostax_repo_get_stack(
    repo: *const PhotostaxRepo,
    id: *const c_char,
) -> *mut FfiPhotoStack {
    let result = panic::catch_unwind(|| {
        if repo.is_null() || id.is_null() {
            return ptr::null_mut();
        }

        let repo_ref = unsafe { &*repo };
        let c_str = unsafe { CStr::from_ptr(id) };
        let id_str = match c_str.to_str() {
            Ok(s) => s,
            Err(_) => return ptr::null_mut(),
        };

        match repo_ref.inner.get_stack(id_str) {
            Ok(stack) => {
                let ffi_stack = photo_stack_to_ffi(&stack);
                Box::into_raw(Box::new(ffi_stack))
            }
            Err(_) => ptr::null_mut(),
        }
    });

    result.unwrap_or(ptr::null_mut())
}

/// Read image bytes.
///
/// # Safety
///
/// - `repo` must be a valid pointer from [`photostax_repo_open`]
/// - `path` must be a valid null-terminated UTF-8 string (file path)
/// - `out_data` must be a valid pointer to receive the data pointer
/// - `out_len` must be a valid pointer to receive the data length
/// - On success, caller owns `*out_data` and must call [`photostax_bytes_free`]
#[no_mangle]
pub unsafe extern "C" fn photostax_read_image(
    repo: *const PhotostaxRepo,
    path: *const c_char,
    out_data: *mut *mut u8,
    out_len: *mut usize,
) -> FfiResult {
    let result = panic::catch_unwind(|| {
        // Null pointer checks
        if repo.is_null() {
            return FfiResult::error("Repository pointer is null");
        }
        if path.is_null() {
            return FfiResult::error("Path pointer is null");
        }
        if out_data.is_null() {
            return FfiResult::error("Output data pointer is null");
        }
        if out_len.is_null() {
            return FfiResult::error("Output length pointer is null");
        }

        let repo_ref = unsafe { &*repo };
        let c_str = unsafe { CStr::from_ptr(path) };
        let path_str = match c_str.to_str() {
            Ok(s) => s,
            Err(_) => return FfiResult::error("Invalid UTF-8 in path"),
        };

        match repo_ref.inner.read_image(std::path::Path::new(path_str)) {
            Ok(bytes) => {
                let len = bytes.len();
                let boxed = bytes.into_boxed_slice();
                let data = Box::into_raw(boxed) as *mut u8;
                unsafe {
                    *out_data = data;
                    *out_len = len;
                }
                FfiResult::success()
            }
            Err(e) => FfiResult::error(&e.to_string()),
        }
    });

    result.unwrap_or_else(|_| FfiResult::error("Panic occurred"))
}

/// Write metadata to a stack.
///
/// # Safety
///
/// - `repo` must be a valid pointer from [`photostax_repo_open`]
/// - `stack_id` must be a valid null-terminated UTF-8 string
/// - `metadata_json` must be a valid null-terminated JSON string
#[no_mangle]
pub unsafe extern "C" fn photostax_write_metadata(
    repo: *const PhotostaxRepo,
    stack_id: *const c_char,
    metadata_json: *const c_char,
) -> FfiResult {
    let result = panic::catch_unwind(|| {
        if repo.is_null() {
            return FfiResult::error("Repository pointer is null");
        }
        if stack_id.is_null() {
            return FfiResult::error("Stack ID pointer is null");
        }
        if metadata_json.is_null() {
            return FfiResult::error("Metadata JSON pointer is null");
        }

        let repo_ref = unsafe { &*repo };
        let stack_id_str = match unsafe { CStr::from_ptr(stack_id) }.to_str() {
            Ok(s) => s,
            Err(_) => return FfiResult::error("Invalid UTF-8 in stack ID"),
        };
        let metadata_str = match unsafe { CStr::from_ptr(metadata_json) }.to_str() {
            Ok(s) => s,
            Err(_) => return FfiResult::error("Invalid UTF-8 in metadata JSON"),
        };

        // Get the stack first
        let stack = match repo_ref.inner.get_stack(stack_id_str) {
            Ok(s) => s,
            Err(e) => return FfiResult::error(&format!("Failed to get stack: {e}")),
        };

        // Parse the metadata JSON
        #[derive(Deserialize, Default)]
        struct MetadataInput {
            #[serde(default)]
            exif_tags: std::collections::HashMap<String, String>,
            #[serde(default)]
            xmp_tags: std::collections::HashMap<String, String>,
            #[serde(default)]
            custom_tags: std::collections::HashMap<String, serde_json::Value>,
        }

        let input: MetadataInput = match serde_json::from_str(metadata_str) {
            Ok(m) => m,
            Err(e) => return FfiResult::error(&format!("Invalid metadata JSON: {e}")),
        };

        let metadata = photostax_core::photo_stack::Metadata {
            exif_tags: input.exif_tags,
            xmp_tags: input.xmp_tags,
            custom_tags: input.custom_tags,
        };

        match repo_ref.inner.write_metadata(&stack, &metadata) {
            Ok(()) => FfiResult::success(),
            Err(e) => FfiResult::error(&e.to_string()),
        }
    });

    result.unwrap_or_else(|_| FfiResult::error("Panic occurred"))
}

/// Free a photo stack array.
///
/// # Safety
///
/// - `array` must have been returned by an FFI function (e.g., [`photostax_repo_scan`])
/// - After calling, all pointers within `array` are invalid
#[no_mangle]
pub unsafe extern "C" fn photostax_stack_array_free(array: FfiPhotoStackArray) {
    let _ = panic::catch_unwind(|| {
        if !array.data.is_null() && array.len > 0 {
            // First, free each stack's strings
            let slice = unsafe { std::slice::from_raw_parts_mut(array.data, array.len) };
            for stack in slice.iter() {
                free_stack_strings(stack);
            }
            // Then free the array itself
            let _ = unsafe { Box::from_raw(std::slice::from_raw_parts_mut(array.data, array.len)) };
        }
    });
}

/// Free a single photo stack.
///
/// # Safety
///
/// - `stack` must have been returned by [`photostax_repo_get_stack`]
/// - After calling, `stack` and all its strings are invalid
#[no_mangle]
pub unsafe extern "C" fn photostax_stack_free(stack: *mut FfiPhotoStack) {
    let _ = panic::catch_unwind(|| {
        if !stack.is_null() {
            let stack_ref = unsafe { &*stack };
            free_stack_strings(stack_ref);
            drop(unsafe { Box::from_raw(stack) });
        }
    });
}

/// Helper to free strings within an FfiPhotoStack (does not free the stack itself).
fn free_stack_strings(stack: &FfiPhotoStack) {
    unsafe {
        if !stack.id.is_null() {
            drop(CString::from_raw(stack.id));
        }
        if !stack.original.is_null() {
            drop(CString::from_raw(stack.original));
        }
        if !stack.enhanced.is_null() {
            drop(CString::from_raw(stack.enhanced));
        }
        if !stack.back.is_null() {
            drop(CString::from_raw(stack.back));
        }
        if !stack.metadata_json.is_null() {
            drop(CString::from_raw(stack.metadata_json));
        }
    }
}

/// Free a string allocated by photostax.
///
/// # Safety
///
/// - `s` must have been allocated by a photostax FFI function, or be null
/// - After calling, `s` is invalid and must not be used
#[no_mangle]
pub unsafe extern "C" fn photostax_string_free(s: *mut c_char) {
    let _ = panic::catch_unwind(|| {
        if !s.is_null() {
            drop(unsafe { CString::from_raw(s) });
        }
    });
}

/// Free a byte buffer allocated by photostax.
///
/// # Safety
///
/// - `data` and `len` must have been returned by a photostax FFI function
/// - After calling, `data` is invalid and must not be used
#[no_mangle]
pub unsafe extern "C" fn photostax_bytes_free(data: *mut u8, len: usize) {
    let _ = panic::catch_unwind(|| {
        if !data.is_null() && len > 0 {
            let _ = unsafe { Box::from_raw(std::slice::from_raw_parts_mut(data, len)) };
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn test_repo_open_null_path() {
        let repo = unsafe { photostax_repo_open(ptr::null()) };
        assert!(repo.is_null());
    }

    #[test]
    fn test_repo_open_valid_path() {
        let path = CString::new(".").unwrap();
        let repo = unsafe { photostax_repo_open(path.as_ptr()) };
        assert!(!repo.is_null());
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_repo_free_null() {
        // Should not panic
        unsafe { photostax_repo_free(ptr::null_mut()) };
    }

    #[test]
    fn test_repo_scan_null() {
        let array = unsafe { photostax_repo_scan(ptr::null()) };
        assert!(array.data.is_null());
        assert_eq!(array.len, 0);
    }

    #[test]
    fn test_repo_get_stack_null_repo() {
        let id = CString::new("test").unwrap();
        let stack = unsafe { photostax_repo_get_stack(ptr::null(), id.as_ptr()) };
        assert!(stack.is_null());
    }

    #[test]
    fn test_repo_get_stack_null_id() {
        let path = CString::new(".").unwrap();
        let repo = unsafe { photostax_repo_open(path.as_ptr()) };
        assert!(!repo.is_null());

        let stack = unsafe { photostax_repo_get_stack(repo, ptr::null()) };
        assert!(stack.is_null());

        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_read_image_null_pointers() {
        let result = unsafe { photostax_read_image(ptr::null(), ptr::null(), ptr::null_mut(), ptr::null_mut()) };
        assert!(!result.success);
        assert!(!result.error_message.is_null());
        unsafe { photostax_string_free(result.error_message) };
    }

    #[test]
    fn test_write_metadata_null_pointers() {
        let result = unsafe { photostax_write_metadata(ptr::null(), ptr::null(), ptr::null()) };
        assert!(!result.success);
        assert!(!result.error_message.is_null());
        unsafe { photostax_string_free(result.error_message) };
    }

    #[test]
    fn test_stack_array_free_empty() {
        let array = FfiPhotoStackArray::empty();
        unsafe { photostax_stack_array_free(array) };
    }

    #[test]
    fn test_stack_free_null() {
        unsafe { photostax_stack_free(ptr::null_mut()) };
    }

    #[test]
    fn test_string_free_null() {
        unsafe { photostax_string_free(ptr::null_mut()) };
    }

    #[test]
    fn test_bytes_free_null() {
        unsafe { photostax_bytes_free(ptr::null_mut(), 0) };
    }

    #[test]
    fn test_ffi_result_success() {
        let result = FfiResult::success();
        assert!(result.success);
        assert!(result.error_message.is_null());
    }

    #[test]
    fn test_ffi_result_error() {
        let result = FfiResult::error("test error");
        assert!(!result.success);
        assert!(!result.error_message.is_null());
        unsafe {
            let msg = CStr::from_ptr(result.error_message);
            assert_eq!(msg.to_str().unwrap(), "test error");
            photostax_string_free(result.error_message);
        }
    }

    #[test]
    fn test_utf8_edge_cases() {
        // Test with valid UTF-8 containing various characters
        let path = CString::new("/tmp/テスト/photo.jpg").unwrap();
        let repo = unsafe { photostax_repo_open(path.as_ptr()) };
        // Should succeed even if directory doesn't exist (just creates handle)
        assert!(!repo.is_null());
        unsafe { photostax_repo_free(repo) };
    }
}

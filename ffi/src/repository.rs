//! Repository-related FFI functions.
//!
//! These functions provide C-compatible access to the photostax-core repository
//! functionality. All functions handle panics and null pointer checks.

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_void};
use std::panic;
use std::path::PathBuf;
use std::ptr;

use photostax_core::backends::local::LocalRepository;
use photostax_core::photo_stack::{PhotoStack, Rotation, RotationTarget, ScannerProfile};
use photostax_core::repository::Repository;
use photostax_core::scanner::ScannerConfig;
use serde::Deserialize;

/// C-compatible progress callback function pointer.
///
/// Parameters:
/// - `phase`: 0 = Scanning, 1 = Classifying, 2 = Complete
/// - `current`: items processed so far in current phase
/// - `total`: total items in current phase
/// - `user_data`: opaque pointer passed through from the caller
pub type ScanProgressFn =
    Option<unsafe extern "C" fn(phase: i32, current: usize, total: usize, user_data: *mut c_void)>;

use crate::types::{
    FfiPaginatedResult, FfiPhotoStack, FfiPhotoStackArray, FfiResult, PhotostaxRepo,
};

/// Helper to convert an `Option<ImageFile>` path to a C string (null if None).
fn image_path_to_c_string(img: &Option<photostax_core::hashing::ImageFile>) -> *mut c_char {
    match img {
        Some(f) => {
            let s = f.path.clone();
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
        original: image_path_to_c_string(&stack.original),
        enhanced: image_path_to_c_string(&stack.enhanced),
        back: image_path_to_c_string(&stack.back),
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

/// Create a new repository with recursive subdirectory scanning.
///
/// When `recursive` is true, the scanner will descend into all subdirectories.
/// This is required when the photo library uses FastFoto's folder-based
/// organisation (e.g. `1984_Mexico/`, `SteveJones/`).
///
/// # Safety
///
/// - `path` must be a valid null-terminated UTF-8 string
/// - Returns null if `path` is null or invalid
/// - Caller owns the returned pointer and must call [`photostax_repo_free`]
#[no_mangle]
pub unsafe extern "C" fn photostax_repo_open_recursive(
    path: *const c_char,
    recursive: bool,
) -> *mut PhotostaxRepo {
    let result = panic::catch_unwind(|| {
        if path.is_null() {
            return ptr::null_mut();
        }

        let c_str = unsafe { CStr::from_ptr(path) };
        let path_str = match c_str.to_str() {
            Ok(s) => s,
            Err(_) => return ptr::null_mut(),
        };

        let config = ScannerConfig {
            recursive,
            ..ScannerConfig::default()
        };
        let repo = LocalRepository::with_config(PathBuf::from(path_str), config);
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

/// Scan with a [`ScannerProfile`] and optional progress callback.
///
/// # Parameters
///
/// - `repo` — valid pointer from [`photostax_repo_open`]
/// - `profile` — scanner profile (0=Auto, 1=EnhancedAndBack, 2=EnhancedOnly, 3=OriginalOnly)
/// - `callback` — optional progress callback invoked per-step (may be null)
/// - `user_data` — opaque pointer forwarded to the callback (may be null)
///
/// # Safety
///
/// - `repo` must be a valid pointer from [`photostax_repo_open`]
/// - `callback` and `user_data` must be valid for the duration of the call
/// - Caller owns the returned array and must call [`photostax_stack_array_free`]
#[no_mangle]
pub unsafe extern "C" fn photostax_repo_scan_with_progress(
    repo: *const PhotostaxRepo,
    profile: i32,
    callback: ScanProgressFn,
    user_data: *mut c_void,
) -> FfiPhotoStackArray {
    let result = panic::catch_unwind(|| {
        if repo.is_null() {
            return FfiPhotoStackArray::empty();
        }

        let repo_ref = unsafe { &*repo };
        let scanner_profile = ScannerProfile::from_int(profile).unwrap_or_default();

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

        match repo_ref.inner.scan_with_progress(scanner_profile, progress) {
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

/// Rotate images in a photo stack by the given number of degrees.
///
/// Accepted `degrees` values: `90`, `-90`, `180`, `-180`, `270`.
/// The `target` parameter controls which images are rotated:
/// - `0` = all images (original + enhanced + back)
/// - `1` = front only (original + enhanced)
/// - `2` = back only
///
/// Returns the updated stack with refreshed metadata on success.
///
/// # Safety
///
/// - `repo` must be a valid pointer from [`photostax_repo_open`]
/// - `stack_id` must be a valid null-terminated UTF-8 string
/// - On success, caller owns the returned pointer and must call [`photostax_stack_free`]
/// - Returns null on error; inspect the result for the error message
#[no_mangle]
pub unsafe extern "C" fn photostax_rotate_stack(
    repo: *const PhotostaxRepo,
    stack_id: *const c_char,
    degrees: i32,
    target: i32,
) -> *mut FfiPhotoStack {
    let result = panic::catch_unwind(|| {
        if repo.is_null() {
            return ptr::null_mut();
        }
        if stack_id.is_null() {
            return ptr::null_mut();
        }

        let repo_ref = unsafe { &*repo };
        let id_str = match unsafe { CStr::from_ptr(stack_id) }.to_str() {
            Ok(s) => s,
            Err(_) => return ptr::null_mut(),
        };

        let rotation = match Rotation::from_degrees(degrees) {
            Some(r) => r,
            None => return ptr::null_mut(),
        };

        let rotation_target = match RotationTarget::from_int(target) {
            Some(t) => t,
            None => return ptr::null_mut(),
        };

        match repo_ref
            .inner
            .rotate_stack(id_str, rotation, rotation_target)
        {
            Ok(stack) => Box::into_raw(Box::new(photo_stack_to_ffi(&stack))),
            Err(_) => ptr::null_mut(),
        }
    });

    result.unwrap_or(ptr::null_mut())
}

/// Scan the repository and return a paginated result.
///
/// When `load_metadata` is true, EXIF/XMP/sidecar metadata is loaded for each
/// stack in the returned page. When false, stacks contain only paths and
/// folder-derived metadata (faster for large repositories).
///
/// # Safety
///
/// - `repo` must be a valid pointer from [`photostax_repo_open`]
/// - Returns empty result if `repo` is null or scan fails
/// - Caller owns the returned result and must call [`photostax_paginated_result_free`]
#[no_mangle]
pub unsafe extern "C" fn photostax_repo_scan_paginated(
    repo: *const PhotostaxRepo,
    offset: usize,
    limit: usize,
    load_metadata: bool,
) -> FfiPaginatedResult {
    let result = panic::catch_unwind(|| {
        if repo.is_null() {
            return FfiPaginatedResult::empty(offset, limit);
        }

        let repo_ref = unsafe { &*repo };
        match repo_ref.inner.scan() {
            Ok(stacks) => {
                let paginated = photostax_core::search::paginate_stacks(
                    &stacks,
                    &photostax_core::search::PaginationParams { offset, limit },
                );

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

                let items: Vec<PhotoStack> = if load_metadata {
                    paginated
                        .items
                        .into_iter()
                        .map(|mut s| {
                            let _ = repo_ref.inner.load_metadata(&mut s);
                            s
                        })
                        .collect()
                } else {
                    paginated.items
                };

                let ffi_stacks: Vec<FfiPhotoStack> = items.iter().map(photo_stack_to_ffi).collect();
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
            }
            Err(_) => FfiPaginatedResult::empty(offset, limit),
        }
    });

    result.unwrap_or_else(|_| FfiPaginatedResult::empty(offset, limit))
}

/// Load full metadata (EXIF, XMP, sidecar) for a specific stack and return it
/// as a JSON string.
///
/// This is the lazy-loading counterpart: call after [`photostax_repo_scan`] to
/// retrieve a single stack's metadata on demand.
///
/// # Safety
///
/// - `repo` must be a valid pointer from [`photostax_repo_open`]
/// - `stack_id` must be a valid null-terminated UTF-8 string
/// - Returns null on error or if the stack is not found
/// - Caller owns the returned string and must call [`photostax_string_free`]
#[no_mangle]
pub unsafe extern "C" fn photostax_stack_load_metadata(
    repo: *const PhotostaxRepo,
    stack_id: *const c_char,
) -> *mut c_char {
    let result = panic::catch_unwind(|| {
        if repo.is_null() || stack_id.is_null() {
            return ptr::null_mut();
        }

        let repo_ref = unsafe { &*repo };
        let c_str = unsafe { CStr::from_ptr(stack_id) };
        let id_str = match c_str.to_str() {
            Ok(s) => s,
            Err(_) => return ptr::null_mut(),
        };

        let mut stack = match repo_ref.inner.get_stack(id_str) {
            Ok(s) => s,
            Err(_) => return ptr::null_mut(),
        };

        if repo_ref.inner.load_metadata(&mut stack).is_err() {
            return ptr::null_mut();
        }

        let metadata_json = serde_json::json!({
            "exif_tags": stack.metadata.exif_tags,
            "xmp_tags": stack.metadata.xmp_tags,
            "custom_tags": stack.metadata.custom_tags,
        });

        let json_str = serde_json::to_string(&metadata_json).unwrap_or_else(|_| "{}".to_string());
        CString::new(json_str)
            .map(|s| s.into_raw())
            .unwrap_or(ptr::null_mut())
    });

    result.unwrap_or(ptr::null_mut())
}

/// Free a paginated result.
///
/// # Safety
///
/// - `result` must have been returned by a paginated FFI function
/// - After calling, all pointers within `result` are invalid
#[no_mangle]
pub unsafe extern "C" fn photostax_paginated_result_free(result: FfiPaginatedResult) {
    let _ = panic::catch_unwind(|| {
        if !result.data.is_null() && result.len > 0 {
            let slice = unsafe { std::slice::from_raw_parts_mut(result.data, result.len) };
            for stack in slice.iter() {
                free_stack_strings(stack);
            }
            let _ = unsafe {
                Box::from_raw(std::ptr::slice_from_raw_parts_mut(result.data, result.len))
            };
        }
    });
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
            let _ =
                unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(array.data, array.len)) };
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
            let _ = unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(data, len)) };
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    /// Helper: path to core/tests/testdata
    fn testdata_path() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("core")
            .join("tests")
            .join("testdata")
    }

    /// Helper: open repo at testdata
    fn open_testdata_repo() -> *mut PhotostaxRepo {
        let path = CString::new(testdata_path().to_str().unwrap()).unwrap();
        let repo = unsafe { photostax_repo_open(path.as_ptr()) };
        assert!(!repo.is_null());
        repo
    }

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
    fn test_repo_scan_with_testdata() {
        let repo = open_testdata_repo();
        let array = unsafe { photostax_repo_scan(repo) };
        assert!(!array.data.is_null());
        assert!(array.len > 0, "Expected stacks from testdata");

        // Verify first stack has valid id
        let first = unsafe { &*array.data };
        assert!(!first.id.is_null());
        let id_str = unsafe { CStr::from_ptr(first.id) }.to_str().unwrap();
        assert!(!id_str.is_empty());

        // After lazy scan, metadata_json has structure but no file-based EXIF data
        assert!(!first.metadata_json.is_null());
        let meta_str = unsafe { CStr::from_ptr(first.metadata_json) }
            .to_str()
            .unwrap();
        let meta: serde_json::Value = serde_json::from_str(meta_str).unwrap();
        assert!(meta["exif_tags"].is_object(), "exif_tags key should exist");
        // Bare scan does NOT load file-based EXIF — map should be empty
        assert!(
            meta["exif_tags"].as_object().unwrap().is_empty()
                || !meta["exif_tags"].as_object().unwrap().contains_key("Make"),
            "Bare scan should not contain file-based EXIF like Make"
        );

        unsafe { photostax_stack_array_free(array) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_repo_scan_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let path = CString::new(dir.path().to_str().unwrap()).unwrap();
        let repo = unsafe { photostax_repo_open(path.as_ptr()) };
        assert!(!repo.is_null());

        let array = unsafe { photostax_repo_scan(repo) };
        assert!(array.data.is_null());
        assert_eq!(array.len, 0);

        unsafe { photostax_stack_array_free(array) };
        unsafe { photostax_repo_free(repo) };
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
    fn test_repo_get_stack_happy_path() {
        let repo = open_testdata_repo();

        let id = CString::new("FamilyPhotos_0001").unwrap();
        let stack_ptr = unsafe { photostax_repo_get_stack(repo, id.as_ptr()) };
        assert!(!stack_ptr.is_null(), "Stack FamilyPhotos_0001 should exist");

        let stack = unsafe { &*stack_ptr };
        let id_str = unsafe { CStr::from_ptr(stack.id) }.to_str().unwrap();
        assert_eq!(id_str, "FamilyPhotos_0001");

        // Should have original path
        assert!(!stack.original.is_null());
        let orig_str = unsafe { CStr::from_ptr(stack.original) }.to_str().unwrap();
        assert!(orig_str.contains("FamilyPhotos_0001"));

        unsafe { photostax_stack_free(stack_ptr) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_repo_get_stack_not_found() {
        let repo = open_testdata_repo();

        let id = CString::new("nonexistent_stack").unwrap();
        let stack_ptr = unsafe { photostax_repo_get_stack(repo, id.as_ptr()) };
        assert!(stack_ptr.is_null());

        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_read_image_null_pointers() {
        let result = unsafe {
            photostax_read_image(ptr::null(), ptr::null(), ptr::null_mut(), ptr::null_mut())
        };
        assert!(!result.success);
        assert!(!result.error_message.is_null());
        unsafe { photostax_string_free(result.error_message) };
    }

    #[test]
    fn test_read_image_null_path() {
        let repo = open_testdata_repo();
        let mut out_data: *mut u8 = ptr::null_mut();
        let mut out_len: usize = 0;
        let result =
            unsafe { photostax_read_image(repo, ptr::null(), &mut out_data, &mut out_len) };
        assert!(!result.success);
        unsafe { photostax_string_free(result.error_message) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_read_image_null_out_data() {
        let repo = open_testdata_repo();
        let path = CString::new("test.jpg").unwrap();
        let mut out_len: usize = 0;
        let result =
            unsafe { photostax_read_image(repo, path.as_ptr(), ptr::null_mut(), &mut out_len) };
        assert!(!result.success);
        unsafe { photostax_string_free(result.error_message) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_read_image_null_out_len() {
        let repo = open_testdata_repo();
        let path = CString::new("test.jpg").unwrap();
        let mut out_data: *mut u8 = ptr::null_mut();
        let result =
            unsafe { photostax_read_image(repo, path.as_ptr(), &mut out_data, ptr::null_mut()) };
        assert!(!result.success);
        unsafe { photostax_string_free(result.error_message) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_read_image_happy_path() {
        let repo = open_testdata_repo();
        let image_path = testdata_path().join("FamilyPhotos_0001.jpg");
        let path = CString::new(image_path.to_str().unwrap()).unwrap();
        let mut out_data: *mut u8 = ptr::null_mut();
        let mut out_len: usize = 0;

        let result =
            unsafe { photostax_read_image(repo, path.as_ptr(), &mut out_data, &mut out_len) };
        assert!(
            result.success,
            "read_image should succeed for existing file"
        );
        assert!(!out_data.is_null());
        assert!(out_len > 0);

        // JPEG magic bytes
        let first_two = unsafe { std::slice::from_raw_parts(out_data, 2) };
        assert_eq!(first_two, &[0xFF, 0xD8], "Should be JPEG data");

        unsafe { photostax_bytes_free(out_data, out_len) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_read_image_nonexistent_file() {
        let repo = open_testdata_repo();
        let path = CString::new("/nonexistent/file.jpg").unwrap();
        let mut out_data: *mut u8 = ptr::null_mut();
        let mut out_len: usize = 0;

        let result =
            unsafe { photostax_read_image(repo, path.as_ptr(), &mut out_data, &mut out_len) };
        assert!(!result.success);
        assert!(!result.error_message.is_null());
        unsafe { photostax_string_free(result.error_message) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_write_metadata_null_pointers() {
        let result = unsafe { photostax_write_metadata(ptr::null(), ptr::null(), ptr::null()) };
        assert!(!result.success);
        assert!(!result.error_message.is_null());
        unsafe { photostax_string_free(result.error_message) };
    }

    #[test]
    fn test_write_metadata_null_stack_id() {
        let repo = open_testdata_repo();
        let json = CString::new("{}").unwrap();
        let result = unsafe { photostax_write_metadata(repo, ptr::null(), json.as_ptr()) };
        assert!(!result.success);
        unsafe { photostax_string_free(result.error_message) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_write_metadata_null_json() {
        let repo = open_testdata_repo();
        let id = CString::new("test").unwrap();
        let result = unsafe { photostax_write_metadata(repo, id.as_ptr(), ptr::null()) };
        assert!(!result.success);
        unsafe { photostax_string_free(result.error_message) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_write_metadata_invalid_json() {
        let repo = open_testdata_repo();
        let id = CString::new("FamilyPhotos_0001").unwrap();
        let json = CString::new("not valid json").unwrap();
        let result = unsafe { photostax_write_metadata(repo, id.as_ptr(), json.as_ptr()) };
        assert!(!result.success);
        unsafe { photostax_string_free(result.error_message) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_write_metadata_nonexistent_stack() {
        let repo = open_testdata_repo();
        let id = CString::new("nonexistent_stack").unwrap();
        let json = CString::new(r#"{"custom_tags":{"album":"test"}}"#).unwrap();
        let result = unsafe { photostax_write_metadata(repo, id.as_ptr(), json.as_ptr()) };
        assert!(!result.success);
        unsafe { photostax_string_free(result.error_message) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_write_metadata_happy_path() {
        // Copy testdata to temp dir so we don't pollute the real testdata
        let dir = tempfile::tempdir().unwrap();
        for entry in std::fs::read_dir(testdata_path()).unwrap() {
            let entry = entry.unwrap();
            std::fs::copy(entry.path(), dir.path().join(entry.file_name())).unwrap();
        }

        let path = CString::new(dir.path().to_str().unwrap()).unwrap();
        let repo = unsafe { photostax_repo_open(path.as_ptr()) };
        assert!(!repo.is_null());

        let id = CString::new("FamilyPhotos_0001").unwrap();
        let json = CString::new(r#"{"custom_tags":{"album":"Family"}}"#).unwrap();
        let result = unsafe { photostax_write_metadata(repo, id.as_ptr(), json.as_ptr()) };
        assert!(result.success, "write_metadata should succeed");

        unsafe { photostax_repo_free(repo) };
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
    fn test_string_free_valid() {
        let s = CString::new("hello").unwrap();
        let ptr = s.into_raw();
        unsafe { photostax_string_free(ptr) };
    }

    #[test]
    fn test_bytes_free_null() {
        unsafe { photostax_bytes_free(ptr::null_mut(), 0) };
    }

    #[test]
    fn test_bytes_free_valid() {
        let data = vec![1u8, 2, 3, 4];
        let len = data.len();
        let boxed = data.into_boxed_slice();
        let ptr = Box::into_raw(boxed) as *mut u8;
        unsafe { photostax_bytes_free(ptr, len) };
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

    #[test]
    fn test_path_to_c_string_none() {
        let result = image_path_to_c_string(&None);
        assert!(result.is_null());
    }

    #[test]
    fn test_path_to_c_string_some() {
        let img = Some(photostax_core::hashing::ImageFile::new(
            "/test/path.jpg",
            0,
        ));
        let result = image_path_to_c_string(&img);
        assert!(!result.is_null());
        let s = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
        assert!(s.contains("path.jpg"));
        unsafe { photostax_string_free(result) };
    }

    #[test]
    fn test_photo_stack_to_ffi_basic() {
        let mut stack = PhotoStack::new("test_stack");
        stack.original = Some(photostax_core::hashing::ImageFile::new(
            "/test/original.jpg",
            0,
        ));
        stack.enhanced = Some(photostax_core::hashing::ImageFile::new(
            "/test/enhanced.jpg",
            0,
        ));
        let ffi = photo_stack_to_ffi(&stack);
        assert!(!ffi.id.is_null());
        let id_str = unsafe { CStr::from_ptr(ffi.id) }.to_str().unwrap();
        assert_eq!(id_str, "test_stack");
        assert!(!ffi.original.is_null());
        assert!(!ffi.enhanced.is_null());
        assert!(ffi.back.is_null());
        assert!(!ffi.metadata_json.is_null());

        // Clean up
        unsafe {
            drop(CString::from_raw(ffi.id));
            drop(CString::from_raw(ffi.original));
            drop(CString::from_raw(ffi.enhanced));
            drop(CString::from_raw(ffi.metadata_json));
        }
    }

    #[test]
    fn test_photo_stack_to_ffi_with_metadata() {
        let mut metadata = photostax_core::photo_stack::Metadata::default();
        metadata
            .exif_tags
            .insert("Make".to_string(), "EPSON".to_string());
        metadata
            .custom_tags
            .insert("album".to_string(), serde_json::json!("Family"));

        let mut stack = PhotoStack::new("meta_test");
        stack.metadata = metadata;
        let ffi = photo_stack_to_ffi(&stack);
        let meta_str = unsafe { CStr::from_ptr(ffi.metadata_json) }
            .to_str()
            .unwrap();
        assert!(meta_str.contains("EPSON"));
        assert!(meta_str.contains("Family"));

        // Clean up
        unsafe {
            drop(CString::from_raw(ffi.id));
            drop(CString::from_raw(ffi.metadata_json));
        }
    }

    // ======================== Invalid UTF-8 tests ========================

    #[test]
    fn test_repo_open_invalid_utf8() {
        let invalid: &[u8] = &[0xff, 0xfe, 0x00];
        let result = unsafe { photostax_repo_open(invalid.as_ptr() as *const c_char) };
        assert!(result.is_null());
    }

    #[test]
    fn test_get_stack_invalid_utf8_id() {
        let repo = open_testdata_repo();
        let invalid: &[u8] = &[0xff, 0x00];
        let result = unsafe { photostax_repo_get_stack(repo, invalid.as_ptr() as *const c_char) };
        assert!(result.is_null());
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_read_image_invalid_utf8_path() {
        let repo = open_testdata_repo();
        let invalid: &[u8] = &[0xff, 0x00];
        let mut data: *mut u8 = ptr::null_mut();
        let mut len: usize = 0;
        let result = unsafe {
            photostax_read_image(repo, invalid.as_ptr() as *const c_char, &mut data, &mut len)
        };
        assert!(!result.success);
        unsafe { photostax_string_free(result.error_message) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_write_metadata_invalid_utf8_stack_id() {
        let repo = open_testdata_repo();
        let invalid: &[u8] = &[0xff, 0x00];
        let json = CString::new("{}").unwrap();
        let result = unsafe {
            photostax_write_metadata(repo, invalid.as_ptr() as *const c_char, json.as_ptr())
        };
        assert!(!result.success);
        unsafe { photostax_string_free(result.error_message) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_write_metadata_invalid_utf8_json() {
        let repo = open_testdata_repo();
        let id = CString::new("FamilyPhotos_0001").unwrap();
        let invalid: &[u8] = &[0xff, 0x00];
        let result = unsafe {
            photostax_write_metadata(repo, id.as_ptr(), invalid.as_ptr() as *const c_char)
        };
        assert!(!result.success);
        unsafe { photostax_string_free(result.error_message) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_write_metadata_nonexistent_stack_utf8() {
        let repo = open_testdata_repo();
        let id = CString::new("NONEXISTENT_UTF8").unwrap();
        let json = CString::new(r#"{"custom_tags":{"k":"v"}}"#).unwrap();
        let result = unsafe { photostax_write_metadata(repo, id.as_ptr(), json.as_ptr()) };
        assert!(!result.success);
        unsafe { photostax_string_free(result.error_message) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_scan_error_on_invalid_repo() {
        // Repo pointing to nonexistent directory
        let path = CString::new("/nonexistent/ffi/repo/dir").unwrap();
        let repo = unsafe { photostax_repo_open(path.as_ptr()) };
        assert!(!repo.is_null());
        let result = unsafe { photostax_repo_scan(repo) };
        // On some OSes returns empty, on others returns empty (scan may succeed with 0)
        // Either way the function should not panic
        unsafe { crate::repository::photostax_stack_array_free(result) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_path_to_c_string_with_null_byte() {
        // A path containing a null byte should cause CString::new to fail
        let img = Some(photostax_core::hashing::ImageFile::new(
            "path\0with_null.jpg",
            0,
        ));
        let result = image_path_to_c_string(&img);
        assert!(result.is_null());
    }

    #[test]
    fn test_open_recursive_null_returns_null() {
        let result = unsafe { photostax_repo_open_recursive(ptr::null(), true) };
        assert!(result.is_null());
    }

    #[test]
    fn test_open_recursive_false_is_equivalent_to_open() {
        let path = CString::new(testdata_path().to_string_lossy().as_ref()).unwrap();
        let repo = unsafe { photostax_repo_open_recursive(path.as_ptr(), false) };
        assert!(!repo.is_null());

        let result = unsafe { photostax_repo_scan(repo) };
        assert!(result.len > 0, "testdata should have stacks");
        unsafe { crate::repository::photostax_stack_array_free(result) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_open_recursive_scans_subdirectories() {
        // Create a temp dir with a subdirectory and files
        let tmp = tempfile::TempDir::new().unwrap();
        let subdir = tmp.path().join("2024_Summer");
        std::fs::create_dir(&subdir).unwrap();

        // Minimal JPEG (SOI + EOI)
        let jpeg = vec![0xFF, 0xD8, 0xFF, 0xD9];
        std::fs::write(subdir.join("IMG_001.jpg"), &jpeg).unwrap();

        // Non-recursive should find nothing at the top level
        let path = CString::new(tmp.path().to_string_lossy().as_ref()).unwrap();
        let repo_flat = unsafe { photostax_repo_open_recursive(path.as_ptr(), false) };
        assert!(!repo_flat.is_null());
        let result_flat = unsafe { photostax_repo_scan(repo_flat) };
        assert_eq!(result_flat.len, 0, "flat scan should find nothing at root");
        unsafe { crate::repository::photostax_stack_array_free(result_flat) };
        unsafe { photostax_repo_free(repo_flat) };

        // Recursive should find the file in the subdirectory
        let repo_rec = unsafe { photostax_repo_open_recursive(path.as_ptr(), true) };
        assert!(!repo_rec.is_null());
        let result_rec = unsafe { photostax_repo_scan(repo_rec) };
        assert_eq!(result_rec.len, 1, "recursive scan should find 1 stack");
        unsafe { crate::repository::photostax_stack_array_free(result_rec) };
        unsafe { photostax_repo_free(repo_rec) };
    }

    // ======================== photostax_stack_load_metadata tests ========================

    #[test]
    fn test_stack_load_metadata_null_repo() {
        let id = CString::new("test").unwrap();
        let result = unsafe { photostax_stack_load_metadata(ptr::null(), id.as_ptr()) };
        assert!(result.is_null());
    }

    #[test]
    fn test_stack_load_metadata_null_id() {
        let repo = open_testdata_repo();
        let result = unsafe { photostax_stack_load_metadata(repo, ptr::null()) };
        assert!(result.is_null());
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_stack_load_metadata_nonexistent_stack() {
        let repo = open_testdata_repo();
        let id = CString::new("nonexistent_stack").unwrap();
        let result = unsafe { photostax_stack_load_metadata(repo, id.as_ptr()) };
        assert!(result.is_null());
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_stack_load_metadata_invalid_utf8() {
        let repo = open_testdata_repo();
        let invalid: &[u8] = &[0xff, 0x00];
        let result =
            unsafe { photostax_stack_load_metadata(repo, invalid.as_ptr() as *const c_char) };
        assert!(result.is_null());
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_stack_load_metadata_happy_path() {
        let repo = open_testdata_repo();
        let id = CString::new("FamilyPhotos_0001").unwrap();
        let result = unsafe { photostax_stack_load_metadata(repo, id.as_ptr()) };
        assert!(
            !result.is_null(),
            "Should load metadata for FamilyPhotos_0001"
        );

        let meta_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
        let meta: serde_json::Value = serde_json::from_str(meta_str).unwrap();
        assert!(meta["exif_tags"].is_object());
        // After load_metadata, EXIF data should be present
        assert!(
            meta["exif_tags"].as_object().unwrap().contains_key("Make"),
            "load_metadata should populate file-based EXIF tags"
        );

        unsafe { photostax_string_free(result) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_scan_then_load_metadata_roundtrip() {
        let repo = open_testdata_repo();

        // Bare scan returns lightweight stacks
        let array = unsafe { photostax_repo_scan(repo) };
        assert!(array.len > 0);
        let first = unsafe { &*array.data };
        let id_str = unsafe { CStr::from_ptr(first.id) }.to_str().unwrap();

        // Load metadata for that stack
        let id = CString::new(id_str).unwrap();
        let meta_ptr = unsafe { photostax_stack_load_metadata(repo, id.as_ptr()) };
        assert!(!meta_ptr.is_null(), "Should load metadata by scan'd ID");

        let meta_str = unsafe { CStr::from_ptr(meta_ptr) }.to_str().unwrap();
        let meta: serde_json::Value = serde_json::from_str(meta_str).unwrap();
        assert!(meta["exif_tags"].is_object());
        assert!(meta["xmp_tags"].is_object());
        assert!(meta["custom_tags"].is_object());

        unsafe { photostax_string_free(meta_ptr) };
        unsafe { photostax_stack_array_free(array) };
        unsafe { photostax_repo_free(repo) };
    }

    // ======================== scan_paginated with load_metadata tests ========================

    #[test]
    fn test_scan_paginated_with_metadata() {
        let repo = open_testdata_repo();
        let page = unsafe { photostax_repo_scan_paginated(repo, 0, 2, true) };
        assert!(page.total_count > 0, "Expected stacks from testdata");
        assert!(page.len > 0);

        // With load_metadata=true, metadata should be populated
        let first = unsafe { &*page.data };
        assert!(!first.metadata_json.is_null());
        let meta_str = unsafe { CStr::from_ptr(first.metadata_json) }
            .to_str()
            .unwrap();
        let meta: serde_json::Value = serde_json::from_str(meta_str).unwrap();
        assert!(
            meta["exif_tags"].as_object().unwrap().contains_key("Make"),
            "With load_metadata=true, EXIF should be populated"
        );

        unsafe { photostax_paginated_result_free(page) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_scan_paginated_without_metadata() {
        let repo = open_testdata_repo();
        let page = unsafe { photostax_repo_scan_paginated(repo, 0, 2, false) };
        assert!(page.total_count > 0, "Expected stacks from testdata");
        assert!(page.len > 0);

        // With load_metadata=false, metadata should be empty/minimal
        let first = unsafe { &*page.data };
        assert!(!first.metadata_json.is_null());
        let meta_str = unsafe { CStr::from_ptr(first.metadata_json) }
            .to_str()
            .unwrap();
        let meta: serde_json::Value = serde_json::from_str(meta_str).unwrap();
        assert!(
            meta["exif_tags"].as_object().unwrap().is_empty()
                || !meta["exif_tags"].as_object().unwrap().contains_key("Make"),
            "With load_metadata=false, EXIF should not be populated"
        );

        unsafe { photostax_paginated_result_free(page) };
        unsafe { photostax_repo_free(repo) };
    }

    // ── Rotate stack FFI tests ─────────────────────────────────────────────

    /// Create a real JPEG in the given directory with known dimensions.
    fn create_test_image_jpeg(path: &std::path::Path, width: u32, height: u32) {
        let img = image::RgbImage::from_fn(width, height, |x, y| image::Rgb([x as u8, y as u8, 0]));
        img.save(path).unwrap();
    }

    #[test]
    fn test_rotate_stack_null_repo() {
        let id = CString::new("test").unwrap();
        let result = unsafe { photostax_rotate_stack(ptr::null(), id.as_ptr(), 90, 0) };
        assert!(result.is_null());
    }

    #[test]
    fn test_rotate_stack_null_id() {
        let dir = tempfile::tempdir().unwrap();
        let path = CString::new(dir.path().to_str().unwrap()).unwrap();
        let repo = unsafe { photostax_repo_open(path.as_ptr()) };
        let result = unsafe { photostax_rotate_stack(repo, ptr::null(), 90, 0) };
        assert!(result.is_null());
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_rotate_stack_invalid_degrees() {
        let dir = tempfile::tempdir().unwrap();
        create_test_image_jpeg(&dir.path().join("IMG_001.jpg"), 4, 2);

        let path = CString::new(dir.path().to_str().unwrap()).unwrap();
        let repo = unsafe { photostax_repo_open(path.as_ptr()) };
        let id = CString::new("IMG_001").unwrap();
        let result = unsafe { photostax_rotate_stack(repo, id.as_ptr(), 45, 0) };
        assert!(result.is_null());
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_rotate_stack_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let path = CString::new(dir.path().to_str().unwrap()).unwrap();
        let repo = unsafe { photostax_repo_open(path.as_ptr()) };
        let id = CString::new("nonexistent").unwrap();
        let result = unsafe { photostax_rotate_stack(repo, id.as_ptr(), 90, 0) };
        assert!(result.is_null());
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_rotate_stack_happy_path() {
        let dir = tempfile::tempdir().unwrap();
        create_test_image_jpeg(&dir.path().join("IMG_001.jpg"), 4, 2);
        create_test_image_jpeg(&dir.path().join("IMG_001_a.jpg"), 4, 2);

        let path = CString::new(dir.path().to_str().unwrap()).unwrap();
        let repo = unsafe { photostax_repo_open(path.as_ptr()) };
        let id = CString::new("IMG_001").unwrap();
        let result = unsafe { photostax_rotate_stack(repo, id.as_ptr(), 90, 0) };
        assert!(!result.is_null(), "rotate_stack should return a stack");

        let stack = unsafe { &*result };
        let id_str = unsafe { CStr::from_ptr(stack.id) }.to_str().unwrap();
        assert_eq!(id_str, "IMG_001");

        // Verify file dimensions changed (4×2 → 2×4 after 90° CW)
        let img = image::open(dir.path().join("IMG_001.jpg")).unwrap();
        assert_eq!(img.width(), 2);
        assert_eq!(img.height(), 4);

        unsafe { photostax_stack_free(result) };
        unsafe { photostax_repo_free(repo) };
    }
}

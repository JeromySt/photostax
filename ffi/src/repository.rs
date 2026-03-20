//! Repository-related FFI functions.
//!
//! These functions provide C-compatible access to the photostax-core repository
//! functionality. All functions handle panics and null pointer checks.

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_void};
use std::panic::{self, AssertUnwindSafe};
use std::path::PathBuf;
use std::ptr;

use photostax_core::backends::local::LocalRepository;
use photostax_core::photo_stack::{PhotoStack, Rotation, RotationTarget, ScannerProfile};
use photostax_core::scanner::ScannerConfig;
use photostax_core::stack_manager::StackManager;
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
    FfiPaginatedResult, FfiPhotoStack, FfiPhotoStackArray, FfiProviderCallbacks, FfiResult,
    PhotostaxRepo,
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

    let name = CString::new(stack.name.clone())
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
        name,
        folder: stack
            .folder
            .as_deref()
            .and_then(|f| CString::new(f).ok())
            .map(|s| s.into_raw())
            .unwrap_or(ptr::null_mut()),
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
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if path.is_null() {
            return ptr::null_mut();
        }

        let c_str = unsafe { CStr::from_ptr(path) };
        let path_str = match c_str.to_str() {
            Ok(s) => s,
            Err(_) => return ptr::null_mut(),
        };

        let repo = LocalRepository::new(PathBuf::from(path_str));
        let mgr = match StackManager::single(Box::new(repo), ScannerProfile::Auto) {
            Ok(m) => m,
            Err(_) => return ptr::null_mut(),
        };
        let boxed = Box::new(PhotostaxRepo {
            inner: std::cell::RefCell::new(mgr),
        });
        Box::into_raw(boxed)
    }));

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
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
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
        let mgr = match StackManager::single(Box::new(repo), ScannerProfile::Auto) {
            Ok(m) => m,
            Err(_) => return ptr::null_mut(),
        };
        let boxed = Box::new(PhotostaxRepo {
            inner: std::cell::RefCell::new(mgr),
        });
        Box::into_raw(boxed)
    }));

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
    let _ = panic::catch_unwind(AssertUnwindSafe(|| {
        if !repo.is_null() {
            drop(unsafe { Box::from_raw(repo) });
        }
    }));
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
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if repo.is_null() {
            return FfiPhotoStackArray::empty();
        }

        let repo_ref = unsafe { &*repo };
        let mut mgr = repo_ref.inner.borrow_mut();
        if mgr.scan().is_err() {
            return FfiPhotoStackArray::empty();
        }
        let all = mgr.query(&photostax_core::search::SearchQuery::new(), None);
        drop(mgr);

        if all.items.is_empty() {
            return FfiPhotoStackArray::empty();
        }

        let ffi_stacks: Vec<FfiPhotoStack> = all.items.iter().map(photo_stack_to_ffi).collect();
        let len = ffi_stacks.len();
        let boxed_slice = ffi_stacks.into_boxed_slice();
        let data = Box::into_raw(boxed_slice) as *mut FfiPhotoStack;

        FfiPhotoStackArray { data, len }
    }));

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
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if repo.is_null() {
            return FfiPhotoStackArray::empty();
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
            return FfiPhotoStackArray::empty();
        }
        let all = mgr.query(&photostax_core::search::SearchQuery::new(), None);
        drop(mgr);

        if all.items.is_empty() {
            return FfiPhotoStackArray::empty();
        }

        let ffi_stacks: Vec<FfiPhotoStack> = all.items.iter().map(photo_stack_to_ffi).collect();
        let len = ffi_stacks.len();
        let boxed_slice = ffi_stacks.into_boxed_slice();
        let data = Box::into_raw(boxed_slice) as *mut FfiPhotoStack;

        FfiPhotoStackArray { data, len }
    }));

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
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if repo.is_null() || id.is_null() {
            return ptr::null_mut();
        }

        let repo_ref = unsafe { &*repo };
        let c_str = unsafe { CStr::from_ptr(id) };
        let id_str = match c_str.to_str() {
            Ok(s) => s,
            Err(_) => return ptr::null_mut(),
        };

        let mut mgr = repo_ref.inner.borrow_mut();
        if mgr.is_empty() && mgr.scan().is_err() {
            return ptr::null_mut();
        }
        match mgr.get_stack(id_str) {
            Some(stack) => {
                let ffi_stack = photo_stack_to_ffi(stack);
                Box::into_raw(Box::new(ffi_stack))
            }
            None => ptr::null_mut(),
        }
    }));

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
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
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

        let mgr = repo_ref.inner.borrow();
        match mgr.read_image(path_str) {
            Ok(mut reader) => {
                let mut buf = Vec::new();
                if let Err(e) = std::io::Read::read_to_end(&mut reader, &mut buf) {
                    return FfiResult::error(&e.to_string());
                }
                let len = buf.len();
                let boxed = buf.into_boxed_slice();
                let data = Box::into_raw(boxed) as *mut u8;
                unsafe {
                    *out_data = data;
                    *out_len = len;
                }
                FfiResult::success()
            }
            Err(e) => FfiResult::error(&e.to_string()),
        }
    }));

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
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
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

        let mut mgr = repo_ref.inner.borrow_mut();
        if mgr.is_empty() && mgr.scan().is_err() {
            return FfiResult::error("Failed to scan repository");
        }
        drop(mgr);

        let mgr = repo_ref.inner.borrow();
        match mgr.write_metadata(stack_id_str, &metadata) {
            Ok(()) => FfiResult::success(),
            Err(e) => FfiResult::error(&e.to_string()),
        }
    }));

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
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
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

        let mut mgr = repo_ref.inner.borrow_mut();
        if mgr.is_empty() && mgr.scan().is_err() {
            return ptr::null_mut();
        }
        match mgr.rotate_stack(id_str, rotation, rotation_target) {
            Ok(stack) => Box::into_raw(Box::new(photo_stack_to_ffi(stack))),
            Err(_) => ptr::null_mut(),
        }
    }));

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
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if repo.is_null() {
            return FfiPaginatedResult::empty(offset, limit);
        }

        let repo_ref = unsafe { &*repo };
        let mut mgr = repo_ref.inner.borrow_mut();
        if load_metadata {
            if mgr.scan_with_metadata().is_err() {
                return FfiPaginatedResult::empty(offset, limit);
            }
        } else if mgr.scan().is_err() {
            return FfiPaginatedResult::empty(offset, limit);
        }
        let stacks: Vec<PhotoStack> = mgr
            .query(&photostax_core::search::SearchQuery::new(), None)
            .items;
        drop(mgr);

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

/// Unified query: search + paginate the cache in a single call.
///
/// This is the preferred way to retrieve stacks. Combines filtering and
/// pagination into one operation without intermediate allocations.
///
/// # Parameters
///
/// - `repo` — repository handle from [`photostax_repo_open`]
/// - `query_json` — JSON-serialized [`SearchQuery`], or null to match all stacks
/// - `offset` — number of items to skip (0-based)
/// - `limit` — maximum items to return; 0 means return all matching stacks
///
/// # Safety
///
/// - `repo` must be a valid pointer from [`photostax_repo_open`]
/// - `query_json`, if non-null, must be a valid null-terminated UTF-8 string
/// - Caller owns the returned result and must call [`photostax_paginated_result_free`]
#[no_mangle]
pub unsafe extern "C" fn photostax_query(
    repo: *const PhotostaxRepo,
    query_json: *const c_char,
    offset: usize,
    limit: usize,
) -> FfiPaginatedResult {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if repo.is_null() {
            return FfiPaginatedResult::empty(offset, limit);
        }

        let repo_ref = unsafe { &*repo };
        let mgr = repo_ref.inner.borrow();

        let query = if query_json.is_null() {
            photostax_core::search::SearchQuery::new()
        } else {
            let json_str = match unsafe { CStr::from_ptr(query_json) }.to_str() {
                Ok(s) => s,
                Err(_) => return FfiPaginatedResult::empty(offset, limit),
            };
            let parsed: Result<photostax_core::search::SearchQuery, _> =
                serde_json::from_str(json_str);
            match parsed {
                Ok(q) => q,
                Err(_) => return FfiPaginatedResult::empty(offset, limit),
            }
        };

        let pagination = if limit > 0 {
            Some(photostax_core::search::PaginationParams { offset, limit })
        } else {
            None
        };

        let paginated = mgr.query(&query, pagination.as_ref());

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

/// Create an empty [`StackManager`] with no repositories.
///
/// Use [`photostax_manager_add_repo`] to register repositories, then
/// [`photostax_repo_scan`] (or any other repo function) to operate on them.
/// The returned handle is compatible with all existing `photostax_repo_*`
/// functions.
///
/// # Safety
///
/// - Caller owns the returned handle and must free it with [`photostax_repo_free`]
/// - Returns null on internal error
#[no_mangle]
pub unsafe extern "C" fn photostax_manager_new() -> *mut PhotostaxRepo {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        let mgr = StackManager::new();
        let boxed = Box::new(PhotostaxRepo {
            inner: std::cell::RefCell::new(mgr),
        });
        Box::into_raw(boxed)
    }));
    result.unwrap_or(ptr::null_mut())
}

/// Add a repository to an existing [`StackManager`].
///
/// The `path` is a filesystem directory. Set `recursive` to scan subdirectories.
/// `profile` controls scanner classification: 0 = Auto, 1 = EnhancedOnly,
/// 2 = EnhancedAndBack, 3 = Skip.
///
/// All subsequent scan/query/get operations on this handle will include stacks
/// from every registered repository.
///
/// # Safety
///
/// - `mgr` must be a valid pointer from [`photostax_manager_new`] or
///   [`photostax_repo_open`]
/// - `path` must be a valid null-terminated UTF-8 string
/// - Returns an [`FfiResult`] indicating success or failure
#[no_mangle]
pub unsafe extern "C" fn photostax_manager_add_repo(
    mgr: *mut PhotostaxRepo,
    path: *const c_char,
    recursive: bool,
    profile: i32,
) -> FfiResult {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if mgr.is_null() || path.is_null() {
            return FfiResult::error("null pointer");
        }

        let c_str = unsafe { CStr::from_ptr(path) };
        let path_str = match c_str.to_str() {
            Ok(s) => s,
            Err(_) => return FfiResult::error("invalid UTF-8 path"),
        };

        let config = ScannerConfig {
            recursive,
            ..ScannerConfig::default()
        };
        let repo = LocalRepository::with_config(PathBuf::from(path_str), config);

        let scanner_profile = ScannerProfile::from_int(profile).unwrap_or_default();

        let mgr_ref = unsafe { &*mgr };
        let mut mgr_inner = mgr_ref.inner.borrow_mut();
        match mgr_inner.add_repo(Box::new(repo), scanner_profile) {
            Ok(_) => FfiResult::success(),
            Err(e) => FfiResult::error(&e.to_string()),
        }
    }));
    result.unwrap_or_else(|_| FfiResult::error("panic in photostax_manager_add_repo"))
}

/// Return the number of repositories registered with a [`StackManager`].
///
/// # Safety
///
/// - `mgr` must be a valid pointer from [`photostax_manager_new`] or
///   [`photostax_repo_open`]
/// - Returns 0 if `mgr` is null
#[no_mangle]
pub unsafe extern "C" fn photostax_manager_repo_count(mgr: *const PhotostaxRepo) -> usize {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if mgr.is_null() {
            return 0;
        }
        let mgr_ref = unsafe { &*mgr };
        mgr_ref.inner.borrow().repo_count()
    }));
    result.unwrap_or(0)
}

/// Add a foreign (host-language-provided) repository to a [`StackManager`].
///
/// The host language provides I/O callbacks via [`FfiProviderCallbacks`].
/// The Rust core handles scanning, naming conventions, and metadata operations.
///
/// `recursive` and `profile` control scanning behaviour (same as
/// [`photostax_manager_add_repo`]).
///
/// # Safety
///
/// - `mgr` must be a valid pointer from [`photostax_manager_new`] or
///   [`photostax_repo_open`]
/// - `callbacks` must contain valid function pointers and a valid `ctx`
/// - The `ctx` pointer and all callbacks must remain valid for the lifetime
///   of the manager handle
/// - `callbacks.location` must be a valid null-terminated UTF-8 string
#[no_mangle]
pub unsafe extern "C" fn photostax_manager_add_foreign_repo(
    mgr: *mut PhotostaxRepo,
    callbacks: FfiProviderCallbacks,
    recursive: bool,
    profile: i32,
) -> FfiResult {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if mgr.is_null() {
            return FfiResult::error("null manager pointer");
        }

        let provider = match unsafe {
            crate::foreign_provider::FfiRepositoryProvider::new(callbacks)
        } {
            Ok(p) => p,
            Err(e) => return FfiResult::error(&format!("invalid provider: {e}")),
        };

        let config = ScannerConfig {
            recursive,
            ..ScannerConfig::default()
        };
        let repo =
            photostax_core::backends::foreign::ForeignRepository::with_config(
                Box::new(provider),
                config,
            );

        let scanner_profile = ScannerProfile::from_int(profile).unwrap_or_default();

        let mgr_ref = unsafe { &*mgr };
        let mut mgr_inner = mgr_ref.inner.borrow_mut();
        match mgr_inner.add_repo(Box::new(repo), scanner_profile) {
            Ok(_) => FfiResult::success(),
            Err(e) => FfiResult::error(&e.to_string()),
        }
    }));
    result.unwrap_or_else(|_| FfiResult::error("panic in photostax_manager_add_foreign_repo"))
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
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if repo.is_null() || stack_id.is_null() {
            return ptr::null_mut();
        }

        let repo_ref = unsafe { &*repo };
        let c_str = unsafe { CStr::from_ptr(stack_id) };
        let id_str = match c_str.to_str() {
            Ok(s) => s,
            Err(_) => return ptr::null_mut(),
        };

        let mut mgr = repo_ref.inner.borrow_mut();
        if mgr.is_empty() && mgr.scan().is_err() {
            return ptr::null_mut();
        }
        if mgr.load_metadata(id_str).is_err() {
            return ptr::null_mut();
        }

        let stack = match mgr.get_stack(id_str) {
            Some(s) => s,
            None => return ptr::null_mut(),
        };

        let metadata_json = serde_json::json!({
            "exif_tags": stack.metadata.exif_tags,
            "xmp_tags": stack.metadata.xmp_tags,
            "custom_tags": stack.metadata.custom_tags,
        });

        let json_str = serde_json::to_string(&metadata_json).unwrap_or_else(|_| "{}".to_string());
        CString::new(json_str)
            .map(|s| s.into_raw())
            .unwrap_or(ptr::null_mut())
    }));

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
    let _ = panic::catch_unwind(AssertUnwindSafe(|| {
        if !result.data.is_null() && result.len > 0 {
            let slice = unsafe { std::slice::from_raw_parts_mut(result.data, result.len) };
            for stack in slice.iter() {
                free_stack_strings(stack);
            }
            let _ = unsafe {
                Box::from_raw(std::ptr::slice_from_raw_parts_mut(result.data, result.len))
            };
        }
    }));
}

/// Free a photo stack array.
///
/// # Safety
///
/// - `array` must have been returned by an FFI function (e.g., [`photostax_repo_scan`])
/// - After calling, all pointers within `array` are invalid
#[no_mangle]
pub unsafe extern "C" fn photostax_stack_array_free(array: FfiPhotoStackArray) {
    let _ = panic::catch_unwind(AssertUnwindSafe(|| {
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
    }));
}

/// Free a single photo stack.
///
/// # Safety
///
/// - `stack` must have been returned by [`photostax_repo_get_stack`]
/// - After calling, `stack` and all its strings are invalid
#[no_mangle]
pub unsafe extern "C" fn photostax_stack_free(stack: *mut FfiPhotoStack) {
    let _ = panic::catch_unwind(AssertUnwindSafe(|| {
        if !stack.is_null() {
            let stack_ref = unsafe { &*stack };
            free_stack_strings(stack_ref);
            drop(unsafe { Box::from_raw(stack) });
        }
    }));
}

/// Helper to free strings within an FfiPhotoStack (does not free the stack itself).
fn free_stack_strings(stack: &FfiPhotoStack) {
    unsafe {
        if !stack.id.is_null() {
            drop(CString::from_raw(stack.id));
        }
        if !stack.name.is_null() {
            drop(CString::from_raw(stack.name));
        }
        if !stack.folder.is_null() {
            drop(CString::from_raw(stack.folder));
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
    let _ = panic::catch_unwind(AssertUnwindSafe(|| {
        if !s.is_null() {
            drop(unsafe { CString::from_raw(s) });
        }
    }));
}

/// Free a byte buffer allocated by photostax.
///
/// # Safety
///
/// - `data` and `len` must have been returned by a photostax FFI function
/// - After calling, `data` is invalid and must not be used
#[no_mangle]
pub unsafe extern "C" fn photostax_bytes_free(data: *mut u8, len: usize) {
    let _ = panic::catch_unwind(AssertUnwindSafe(|| {
        if !data.is_null() && len > 0 {
            let _ = unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(data, len)) };
        }
    }));
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

    /// Helper: scan repo and return the opaque ID for the stack with the given name.
    fn find_stack_id_by_name(repo: *const PhotostaxRepo, name: &str) -> String {
        let array = unsafe { photostax_repo_scan(repo) };
        assert!(array.len > 0, "scan should return stacks");
        let slice = unsafe { std::slice::from_raw_parts(array.data, array.len) };
        let found = slice.iter().find(|s| {
            let n = unsafe { CStr::from_ptr(s.name) }.to_str().unwrap();
            n == name
        });
        let id = unsafe { CStr::from_ptr(found.expect("stack not found by name").id) }
            .to_str()
            .unwrap()
            .to_string();
        unsafe { photostax_stack_array_free(array) };
        id
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

        let opaque_id = find_stack_id_by_name(repo, "FamilyPhotos_0001");
        let id = CString::new(opaque_id).unwrap();
        let stack_ptr = unsafe { photostax_repo_get_stack(repo, id.as_ptr()) };
        assert!(!stack_ptr.is_null(), "Stack FamilyPhotos_0001 should exist");

        let stack = unsafe { &*stack_ptr };
        let name_str = unsafe { CStr::from_ptr(stack.name) }.to_str().unwrap();
        assert_eq!(name_str, "FamilyPhotos_0001");

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

        let opaque_id = find_stack_id_by_name(repo, "FamilyPhotos_0001");
        let id = CString::new(opaque_id).unwrap();
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
        let img = Some(photostax_core::hashing::ImageFile::new("/test/path.jpg", 0));
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
        let opaque_id = find_stack_id_by_name(repo, "FamilyPhotos_0001");
        let id = CString::new(opaque_id).unwrap();
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
        let opaque_id = find_stack_id_by_name(repo, "IMG_001");
        let id = CString::new(opaque_id).unwrap();
        let result = unsafe { photostax_rotate_stack(repo, id.as_ptr(), 90, 0) };
        assert!(!result.is_null(), "rotate_stack should return a stack");

        let stack = unsafe { &*result };
        let name_str = unsafe { CStr::from_ptr(stack.name) }.to_str().unwrap();
        assert_eq!(name_str, "IMG_001");

        // Verify file dimensions changed (4×2 → 2×4 after 90° CW)
        let img = image::open(dir.path().join("IMG_001.jpg")).unwrap();
        assert_eq!(img.width(), 2);
        assert_eq!(img.height(), 4);

        unsafe { photostax_stack_free(result) };
        unsafe { photostax_repo_free(repo) };
    }

    // ── StackManager multi-repo tests ──────────────────────────────────

    #[test]
    fn test_manager_new_creates_empty() {
        let mgr = unsafe { photostax_manager_new() };
        assert!(!mgr.is_null());
        let count = unsafe { photostax_manager_repo_count(mgr) };
        assert_eq!(count, 0);
        unsafe { photostax_repo_free(mgr) };
    }

    #[test]
    fn test_manager_add_repo_then_scan() {
        let mgr = unsafe { photostax_manager_new() };
        let path = CString::new(testdata_path().to_str().unwrap()).unwrap();
        let result = unsafe { photostax_manager_add_repo(mgr, path.as_ptr(), false, 0) };
        assert!(result.success);

        let count = unsafe { photostax_manager_repo_count(mgr) };
        assert_eq!(count, 1);

        let array = unsafe { photostax_repo_scan(mgr) };
        assert!(array.len > 0, "should find stacks after adding repo");
        unsafe { photostax_stack_array_free(array) };
        unsafe { photostax_repo_free(mgr) };
    }

    #[test]
    fn test_manager_add_multiple_repos() {
        let dir1 = tempfile::tempdir().unwrap();
        let dir2 = tempfile::tempdir().unwrap();
        // Copy testdata into both dirs
        for entry in std::fs::read_dir(testdata_path()).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_file() {
                std::fs::copy(entry.path(), dir1.path().join(entry.file_name())).unwrap();
                std::fs::copy(entry.path(), dir2.path().join(entry.file_name())).unwrap();
            }
        }

        let mgr = unsafe { photostax_manager_new() };
        let path1 = CString::new(dir1.path().to_str().unwrap()).unwrap();
        let path2 = CString::new(dir2.path().to_str().unwrap()).unwrap();

        let r1 = unsafe { photostax_manager_add_repo(mgr, path1.as_ptr(), false, 0) };
        assert!(r1.success);
        let r2 = unsafe { photostax_manager_add_repo(mgr, path2.as_ptr(), false, 0) };
        assert!(r2.success);
        assert_eq!(unsafe { photostax_manager_repo_count(mgr) }, 2);

        let array = unsafe { photostax_repo_scan(mgr) };
        // Both repos scanned — should have stacks from both
        assert!(array.len > 0);
        unsafe { photostax_stack_array_free(array) };
        unsafe { photostax_repo_free(mgr) };
    }

    #[test]
    fn test_manager_query_across_repos() {
        let dir1 = tempfile::tempdir().unwrap();
        let dir2 = tempfile::tempdir().unwrap();
        for entry in std::fs::read_dir(testdata_path()).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_file() {
                std::fs::copy(entry.path(), dir1.path().join(entry.file_name())).unwrap();
                std::fs::copy(entry.path(), dir2.path().join(entry.file_name())).unwrap();
            }
        }

        let mgr = unsafe { photostax_manager_new() };
        let path1 = CString::new(dir1.path().to_str().unwrap()).unwrap();
        let path2 = CString::new(dir2.path().to_str().unwrap()).unwrap();
        unsafe { photostax_manager_add_repo(mgr, path1.as_ptr(), false, 0) };
        unsafe { photostax_manager_add_repo(mgr, path2.as_ptr(), false, 0) };
        unsafe { photostax_repo_scan(mgr) };

        // Query with no filter — should get stacks from both repos
        let result = unsafe { photostax_query(mgr, ptr::null(), 0, 0) };
        assert!(result.total_count > 0);
        unsafe { photostax_paginated_result_free(result) };
        unsafe { photostax_repo_free(mgr) };
    }

    #[test]
    fn test_manager_add_repo_null_ptr() {
        let result = unsafe { photostax_manager_add_repo(ptr::null_mut(), ptr::null(), false, 0) };
        assert!(!result.success);
        unsafe { photostax_string_free(result.error_message) };
    }

    #[test]
    fn test_manager_repo_count_null() {
        let count = unsafe { photostax_manager_repo_count(ptr::null()) };
        assert_eq!(count, 0);
    }
}

//! Repository-related FFI functions.
//!
//! These functions provide C-compatible access to the photostax-core repository
//! functionality through an opaque handle-based API. All functions handle panics
//! and null pointer checks.

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_void};
use std::panic::{self, AssertUnwindSafe};
use std::path::PathBuf;
use std::ptr;

use photostax_core::backends::local::LocalRepository;
use photostax_core::photo_stack::{ImageProxy, PhotoStack, Rotation, ScannerProfile};
use photostax_core::scanner::ScannerConfig;
use photostax_core::search::SearchQuery;
use photostax_core::stack_manager::StackManager;
use serde::Deserialize;

use crate::types::{
    FfiDimensions, FfiPaginatedHandleResult, FfiProviderCallbacks, FfiResult, FfiStackHandleArray,
    PhotostaxRepo, PhotostaxStack,
};

/// C-compatible progress callback function pointer.
///
/// Parameters:
/// - `repo_id`: null-terminated UTF-8 string identifying the repository
/// - `phase`: 0 = Scanning, 1 = Classifying, 2 = Complete
/// - `current`: items processed so far in current phase
/// - `total`: total items in current phase
/// - `user_data`: opaque pointer passed through from the caller
pub type ScanProgressFn = Option<
    unsafe extern "C" fn(
        repo_id: *const c_char,
        phase: i32,
        current: usize,
        total: usize,
        user_data: *mut c_void,
    ),
>;

/// Wrapper that marks a raw pointer as `Send`.
///
/// # Safety
///
/// The caller guarantees that the wrapped pointer is safe to access from any
/// thread, which is documented as a requirement in [`FfiProviderCallbacks`]
/// and the progress-callback contract.
pub(crate) struct SendPtr(pub(crate) *mut c_void);
unsafe impl Send for SendPtr {}

impl SendPtr {
    pub(crate) fn as_ptr(&self) -> *mut c_void {
        self.0
    }
}

// ── Helper functions ─────────────────────────────────────────────────────────

/// Convert a slice of PhotoStacks into an array of opaque handles.
///
/// Each stack is cloned and wrapped in a heap-allocated [`PhotostaxStack`].
/// The caller (FFI consumer) owns the returned array and must free it with
/// [`photostax_stack_handle_array_free`].
pub(crate) fn stacks_to_handle_array(
    stacks: &[PhotoStack],
    runtime: &tokio::runtime::Handle,
) -> FfiStackHandleArray {
    if stacks.is_empty() {
        return FfiStackHandleArray::empty();
    }
    let handles: Vec<*mut PhotostaxStack> = stacks
        .iter()
        .map(|s| {
            Box::into_raw(Box::new(PhotostaxStack {
                inner: s.clone(),
                runtime: runtime.clone(),
            }))
        })
        .collect();
    let len = handles.len();
    let boxed = handles.into_boxed_slice();
    let ptr = Box::into_raw(boxed) as *mut *mut PhotostaxStack;
    FfiStackHandleArray { handles: ptr, len }
}

/// Convert a page of PhotoStacks into a paginated handle result.
pub(crate) fn stacks_to_paginated_handles(
    stacks: &[PhotoStack],
    total_count: usize,
    offset: usize,
    limit: usize,
    has_more: bool,
    runtime: &tokio::runtime::Handle,
) -> FfiPaginatedHandleResult {
    if stacks.is_empty() {
        return FfiPaginatedHandleResult {
            handles: ptr::null_mut(),
            len: 0,
            total_count,
            offset,
            limit,
            has_more,
        };
    }
    let handles: Vec<*mut PhotostaxStack> = stacks
        .iter()
        .map(|s| {
            Box::into_raw(Box::new(PhotostaxStack {
                inner: s.clone(),
                runtime: runtime.clone(),
            }))
        })
        .collect();
    let len = handles.len();
    let boxed = handles.into_boxed_slice();
    let ptr = Box::into_raw(boxed) as *mut *mut PhotostaxStack;
    FfiPaginatedHandleResult {
        handles: ptr,
        len,
        total_count,
        offset,
        limit,
        has_more,
    }
}

/// Get an image variant proxy from a PhotoStack.
///
/// - 0 = original
/// - 1 = enhanced
/// - 2 = back
fn get_image_ref<'a>(stack: &'a PhotoStack, variant: i32) -> Option<ImageProxy<'a>> {
    match variant {
        0 => Some(stack.original()),
        1 => Some(stack.enhanced()),
        2 => Some(stack.back()),
        _ => None,
    }
}

/// Deserialization target for metadata JSON input.
#[derive(Deserialize, Default)]
struct MetadataInput {
    #[serde(default)]
    exif_tags: std::collections::HashMap<String, String>,
    #[serde(default)]
    xmp_tags: std::collections::HashMap<String, String>,
    #[serde(default)]
    custom_tags: std::collections::HashMap<String, serde_json::Value>,
}

// ── Repository lifecycle ─────────────────────────────────────────────────────

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

        let runtime = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(_) => return ptr::null_mut(),
        };
        let repo = LocalRepository::new(PathBuf::from(path_str));
        let mgr = match StackManager::single(Box::new(repo), ScannerProfile::Auto) {
            Ok(m) => m,
            Err(_) => return ptr::null_mut(),
        };
        let boxed = Box::new(PhotostaxRepo {
            inner: tokio::sync::Mutex::new(mgr),
            runtime,
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

        let runtime = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
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
            inner: tokio::sync::Mutex::new(mgr),
            runtime,
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

// ── Repository scan / query ──────────────────────────────────────────────────

/// Scan the repository and return all photo stacks as opaque handles.
///
/// # Safety
///
/// - `repo` must be a valid pointer from [`photostax_repo_open`]
/// - Returns empty array if `repo` is null or scan fails
/// - Caller owns the returned array and must call [`photostax_stack_handle_array_free`]
#[no_mangle]
pub unsafe extern "C" fn photostax_repo_scan(repo: *const PhotostaxRepo) -> FfiStackHandleArray {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if repo.is_null() {
            return FfiStackHandleArray::empty();
        }

        let repo_ref = unsafe { &*repo };
        repo_ref.runtime.block_on(async {
            let mut mgr = repo_ref.inner.lock().await;
            let all = match mgr.query(None, None, None, None) {
                Ok(snap) => snap,
                Err(_) => return FfiStackHandleArray::empty(),
            };

            stacks_to_handle_array(all.all_stacks(), repo_ref.runtime.handle())
        })
    }));

    result.unwrap_or_else(|_| FfiStackHandleArray::empty())
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
/// - Caller owns the returned array and must call [`photostax_stack_handle_array_free`]
#[no_mangle]
pub unsafe extern "C" fn photostax_repo_scan_with_progress(
    repo: *const PhotostaxRepo,
    profile: i32,
    callback: ScanProgressFn,
    user_data: *mut c_void,
) -> FfiStackHandleArray {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if repo.is_null() {
            return FfiStackHandleArray::empty();
        }

        let repo_ref = unsafe { &*repo };
        let scanner_profile = ScannerProfile::from_int(profile).unwrap_or_default();

        let mut cb_wrapper;
        let ud = SendPtr(user_data);
        let progress: Option<&mut dyn FnMut(&photostax_core::photo_stack::ScanProgress)> =
            if let Some(cb_fn) = callback {
                cb_wrapper = move |p: &photostax_core::photo_stack::ScanProgress| unsafe {
                    let c_repo_id = std::ffi::CString::new(p.repo_id.as_str()).unwrap_or_default();
                    cb_fn(
                        c_repo_id.as_ptr(),
                        p.phase as i32,
                        p.current,
                        p.total,
                        ud.as_ptr(),
                    );
                };
                Some(&mut cb_wrapper)
            } else {
                None
            };

        repo_ref.runtime.block_on(async {
            let mut mgr = repo_ref.inner.lock().await;
            mgr.set_profile(scanner_profile);
            let all = match mgr.query(None, None, progress, None) {
                Ok(snap) => snap,
                Err(_) => return FfiStackHandleArray::empty(),
            };

            stacks_to_handle_array(all.all_stacks(), repo_ref.runtime.handle())
        })
    }));

    result.unwrap_or_else(|_| FfiStackHandleArray::empty())
}

/// Look up a single stack by ID and return an opaque handle.
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
) -> *mut PhotostaxStack {
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

        repo_ref.runtime.block_on(async {
            let mut mgr = repo_ref.inner.lock().await;
            let query = SearchQuery::new().with_ids(vec![id_str.to_string()]);
            match mgr.query(Some(&query), None, None, None) {
                Ok(result) => {
                    if let Some(stack) = result.all_stacks().first() {
                        Box::into_raw(Box::new(PhotostaxStack {
                            inner: stack.clone(),
                            runtime: repo_ref.runtime.handle().clone(),
                        }))
                    } else {
                        ptr::null_mut()
                    }
                }
                Err(_) => ptr::null_mut(),
            }
        })
    }));

    result.unwrap_or(ptr::null_mut())
}

/// Scan the repository and return a paginated result of opaque handles.
///
/// When `load_metadata` is true, EXIF/XMP/sidecar metadata is loaded for each
/// stack before returning. When false, stacks contain only paths and
/// folder-derived metadata (faster for large repositories).
///
/// # Safety
///
/// - `repo` must be a valid pointer from [`photostax_repo_open`]
/// - Returns empty result if `repo` is null or scan fails
/// - Caller owns the returned result and must call [`photostax_paginated_handle_result_free`]
#[no_mangle]
pub unsafe extern "C" fn photostax_repo_scan_paginated(
    repo: *const PhotostaxRepo,
    offset: usize,
    limit: usize,
    load_metadata: bool,
) -> FfiPaginatedHandleResult {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if repo.is_null() {
            return FfiPaginatedHandleResult::empty(offset, limit);
        }

        let repo_ref = unsafe { &*repo };
        repo_ref.runtime.block_on(async {
            let mut mgr = repo_ref.inner.lock().await;
            mgr.invalidate_cache();
            let initial = match mgr.query(None, None, None, None) {
                Ok(r) => r,
                Err(_) => return FfiPaginatedHandleResult::empty(offset, limit),
            };
            if load_metadata {
                for stack in initial.all_stacks() {
                    let _ = stack.metadata().read();
                }
            }
            let snapshot = match mgr.query(None, None, None, None) {
                Ok(snap) => snap,
                Err(_) => return FfiPaginatedHandleResult::empty(offset, limit),
            };

            let paginated = snapshot.snapshot().get_page(offset, limit);
            stacks_to_paginated_handles(
                &paginated.items,
                paginated.total_count,
                paginated.offset,
                paginated.limit,
                paginated.has_more,
                repo_ref.runtime.handle(),
            )
        })
    }));

    result.unwrap_or_else(|_| FfiPaginatedHandleResult::empty(offset, limit))
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
/// - `callback` — optional progress callback (may be null)
/// - `user_data` — opaque pointer forwarded to callback (may be null)
///
/// # Safety
///
/// - `repo` must be a valid pointer from [`photostax_repo_open`]
/// - `query_json`, if non-null, must be a valid null-terminated UTF-8 string
/// - `callback` and `user_data` must be valid for the duration of the call
/// - Caller owns the returned result and must call [`photostax_paginated_handle_result_free`]
#[no_mangle]
pub unsafe extern "C" fn photostax_query(
    repo: *const PhotostaxRepo,
    query_json: *const c_char,
    offset: usize,
    limit: usize,
    callback: ScanProgressFn,
    user_data: *mut c_void,
) -> FfiPaginatedHandleResult {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if repo.is_null() {
            return FfiPaginatedHandleResult::empty(offset, limit);
        }

        let repo_ref = unsafe { &*repo };

        let query = if query_json.is_null() {
            photostax_core::search::SearchQuery::new()
        } else {
            let json_str = match unsafe { CStr::from_ptr(query_json) }.to_str() {
                Ok(s) => s,
                Err(_) => return FfiPaginatedHandleResult::empty(offset, limit),
            };
            let parsed: Result<photostax_core::search::SearchQuery, _> =
                serde_json::from_str(json_str);
            match parsed {
                Ok(q) => q,
                Err(_) => return FfiPaginatedHandleResult::empty(offset, limit),
            }
        };

        let mut cb_wrapper;
        let ud = SendPtr(user_data);
        let progress: Option<&mut dyn FnMut(&photostax_core::photo_stack::ScanProgress)> =
            if let Some(cb_fn) = callback {
                cb_wrapper = move |p: &photostax_core::photo_stack::ScanProgress| unsafe {
                    let c_repo_id = std::ffi::CString::new(p.repo_id.as_str()).unwrap_or_default();
                    cb_fn(
                        c_repo_id.as_ptr(),
                        p.phase as i32,
                        p.current,
                        p.total,
                        ud.as_ptr(),
                    );
                };
                Some(&mut cb_wrapper)
            } else {
                None
            };

        repo_ref.runtime.block_on(async {
            let mut mgr = repo_ref.inner.lock().await;

            let snapshot = match mgr.query(Some(&query), None, progress, None) {
                Ok(snap) => snap,
                Err(_) => return FfiPaginatedHandleResult::empty(offset, limit),
            };

            let paginated = if limit > 0 {
                snapshot.snapshot().get_page(offset, limit)
            } else {
                snapshot
                    .snapshot()
                    .get_page(0, snapshot.total_count().max(1))
            };

            stacks_to_paginated_handles(
                &paginated.items,
                paginated.total_count,
                paginated.offset,
                paginated.limit,
                paginated.has_more,
                repo_ref.runtime.handle(),
            )
        })
    }));

    result.unwrap_or_else(|_| FfiPaginatedHandleResult::empty(offset, limit))
}

// ── Stack accessors ──────────────────────────────────────────────────────────

/// Return the stack's opaque ID as a C string.
///
/// # Safety
///
/// - `stack` must be a valid pointer from [`photostax_repo_get_stack`] or a handle array
/// - Returns null if `stack` is null
/// - Caller owns the returned string and must call [`photostax_string_free`]
#[no_mangle]
pub unsafe extern "C" fn photostax_stack_id(stack: *const PhotostaxStack) -> *mut c_char {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if stack.is_null() {
            return ptr::null_mut();
        }
        let inner = unsafe { &*stack };
        CString::new(inner.inner.id())
            .map(|s| s.into_raw())
            .unwrap_or(ptr::null_mut())
    }));
    result.unwrap_or(ptr::null_mut())
}

/// Return the stack's human-readable name as a C string.
///
/// # Safety
///
/// - `stack` must be a valid pointer
/// - Caller owns the returned string and must call [`photostax_string_free`]
#[no_mangle]
pub unsafe extern "C" fn photostax_stack_name(stack: *const PhotostaxStack) -> *mut c_char {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if stack.is_null() {
            return ptr::null_mut();
        }
        let inner = unsafe { &*stack };
        CString::new(inner.inner.name())
            .map(|s| s.into_raw())
            .unwrap_or(ptr::null_mut())
    }));
    result.unwrap_or(ptr::null_mut())
}

/// Return the stack's folder as a C string (null if no folder).
///
/// # Safety
///
/// - `stack` must be a valid pointer
/// - Caller owns the returned string and must call [`photostax_string_free`]
#[no_mangle]
pub unsafe extern "C" fn photostax_stack_folder(stack: *const PhotostaxStack) -> *mut c_char {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if stack.is_null() {
            return ptr::null_mut();
        }
        let inner = unsafe { &*stack };
        match inner.inner.folder() {
            Some(f) => CString::new(f)
                .map(|s| s.into_raw())
                .unwrap_or(ptr::null_mut()),
            None => ptr::null_mut(),
        }
    }));
    result.unwrap_or(ptr::null_mut())
}

// ── ImageRef FFI ─────────────────────────────────────────────────────────────

/// Check whether an image variant is present in the stack.
///
/// - `variant`: 0 = original, 1 = enhanced, 2 = back
///
/// Returns false if the stack pointer is null or the variant is invalid.
#[no_mangle]
pub unsafe extern "C" fn photostax_stack_image_is_present(
    stack: *const PhotostaxStack,
    variant: i32,
) -> bool {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if stack.is_null() {
            return false;
        }
        let inner = unsafe { &*stack };
        match get_image_ref(&inner.inner, variant) {
            Some(img) => img.is_present(),
            None => false,
        }
    }));
    result.unwrap_or(false)
}

/// Check whether an image variant's file handle is still valid.
///
/// Returns false if the stack pointer is null or the variant is invalid.
#[no_mangle]
pub unsafe extern "C" fn photostax_stack_image_is_valid(
    stack: *const PhotostaxStack,
    variant: i32,
) -> bool {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if stack.is_null() {
            return false;
        }
        let inner = unsafe { &*stack };
        match get_image_ref(&inner.inner, variant) {
            Some(img) => img.is_valid(),
            None => false,
        }
    }));
    result.unwrap_or(false)
}

/// Return the file size of an image variant in bytes, or -1 on error.
#[no_mangle]
pub unsafe extern "C" fn photostax_stack_image_size(
    stack: *const PhotostaxStack,
    variant: i32,
) -> i64 {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if stack.is_null() {
            return -1i64;
        }
        let inner = unsafe { &*stack };
        match get_image_ref(&inner.inner, variant) {
            Some(img) => img.size().map(|s| s as i64).unwrap_or(-1),
            None => -1,
        }
    }));
    result.unwrap_or(-1)
}

/// Read image bytes from a stack's image variant.
///
/// On success, `*out_data` and `*out_len` are populated with the image data.
/// Caller owns the buffer and must free it with [`photostax_bytes_free`].
///
/// - `variant`: 0 = original, 1 = enhanced, 2 = back
#[no_mangle]
pub unsafe extern "C" fn photostax_stack_image_read(
    stack: *const PhotostaxStack,
    variant: i32,
    out_data: *mut *mut u8,
    out_len: *mut usize,
) -> FfiResult {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if stack.is_null() {
            return FfiResult::error("Stack pointer is null");
        }
        if out_data.is_null() {
            return FfiResult::error("Output data pointer is null");
        }
        if out_len.is_null() {
            return FfiResult::error("Output length pointer is null");
        }

        let inner = unsafe { &*stack };
        let image_ref = match get_image_ref(&inner.inner, variant) {
            Some(img) => img,
            None => {
                return FfiResult::error(&format!(
                    "Invalid variant: {variant}. Use 0=original, 1=enhanced, 2=back"
                ))
            }
        };

        if !image_ref.is_present() {
            let variant_name = match variant {
                0 => "original",
                1 => "enhanced",
                2 => "back",
                _ => "unknown",
            };
            return FfiResult::error(&format!("No {variant_name} image present in stack"));
        }

        inner.runtime.block_on(async {
            match image_ref.read() {
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
        })
    }));

    result.unwrap_or_else(|_| FfiResult::error("Panic occurred"))
}

/// Compute and return the content hash of an image variant.
///
/// The hash is cached after the first computation. Returns null on error.
/// Caller owns the returned string and must call [`photostax_string_free`].
#[no_mangle]
pub unsafe extern "C" fn photostax_stack_image_hash(
    stack: *const PhotostaxStack,
    variant: i32,
) -> *mut c_char {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if stack.is_null() {
            return ptr::null_mut();
        }
        let inner = unsafe { &*stack };
        let image_ref = match get_image_ref(&inner.inner, variant) {
            Some(img) => img,
            None => return ptr::null_mut(),
        };
        inner.runtime.block_on(async {
            match image_ref.hash() {
                Ok(h) => CString::new(h)
                    .map(|s| s.into_raw())
                    .unwrap_or(ptr::null_mut()),
                Err(_) => ptr::null_mut(),
            }
        })
    }));
    result.unwrap_or(ptr::null_mut())
}

/// Return the dimensions (width, height) of an image variant.
///
/// The dimensions are cached after the first computation.
/// On error, returns `FfiDimensions { width: 0, height: 0, success: false }`.
#[no_mangle]
pub unsafe extern "C" fn photostax_stack_image_dimensions(
    stack: *const PhotostaxStack,
    variant: i32,
) -> FfiDimensions {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if stack.is_null() {
            return FfiDimensions {
                width: 0,
                height: 0,
                success: false,
            };
        }
        let inner = unsafe { &*stack };
        let image_ref = match get_image_ref(&inner.inner, variant) {
            Some(img) => img,
            None => {
                return FfiDimensions {
                    width: 0,
                    height: 0,
                    success: false,
                }
            }
        };
        inner.runtime.block_on(async {
            match image_ref.dimensions() {
                Ok((w, h)) => FfiDimensions {
                    width: w,
                    height: h,
                    success: true,
                },
                Err(_) => FfiDimensions {
                    width: 0,
                    height: 0,
                    success: false,
                },
            }
        })
    }));
    result.unwrap_or(FfiDimensions {
        width: 0,
        height: 0,
        success: false,
    })
}

/// Rotate an image variant by the given number of degrees.
///
/// Accepted `degrees` values: `90`, `-90`, `180`, `-180`, `270`.
#[no_mangle]
pub unsafe extern "C" fn photostax_stack_image_rotate(
    stack: *const PhotostaxStack,
    variant: i32,
    degrees: i32,
) -> FfiResult {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if stack.is_null() {
            return FfiResult::error("Stack pointer is null");
        }

        let rotation = match Rotation::from_degrees(degrees) {
            Some(r) => r,
            None => {
                return FfiResult::error(&format!(
                    "Invalid degrees: {degrees}. Use 90, -90, 180, -180, or 270"
                ))
            }
        };

        let inner = unsafe { &*stack };
        let image_ref = match get_image_ref(&inner.inner, variant) {
            Some(img) => img,
            None => {
                return FfiResult::error(&format!(
                    "Invalid variant: {variant}. Use 0=original, 1=enhanced, 2=back"
                ))
            }
        };

        if !image_ref.is_present() {
            return FfiResult::error("Image variant is not present");
        }

        inner.runtime.block_on(async {
            match image_ref.rotate(rotation) {
                Ok(()) => FfiResult::success(),
                Err(e) => FfiResult::error(&e.to_string()),
            }
        })
    }));

    result.unwrap_or_else(|_| FfiResult::error("Panic occurred"))
}

/// Invalidate cached hash and dimensions for an image variant.
#[no_mangle]
pub unsafe extern "C" fn photostax_stack_image_invalidate(
    stack: *const PhotostaxStack,
    variant: i32,
) {
    let _ = panic::catch_unwind(AssertUnwindSafe(|| {
        if stack.is_null() {
            return;
        }
        let inner = unsafe { &*stack };
        if let Some(image_ref) = get_image_ref(&inner.inner, variant) {
            inner.runtime.block_on(async {
                image_ref.invalidate_caches();
            });
        }
    }));
}

/// Swap front and back images when a photo was scanned backwards.
///
/// Renames files on disk so filenames match their new roles, deletes
/// the enhanced variant (it was of the wrong side), and clears caches.
#[no_mangle]
pub unsafe extern "C" fn photostax_stack_swap_front_back(
    stack: *const PhotostaxStack,
) -> FfiResult {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if stack.is_null() {
            return FfiResult::error("Stack pointer is null");
        }

        let inner = unsafe { &*stack };

        inner.runtime.block_on(async {
            match inner.inner.swap_front_back() {
                Ok(()) => FfiResult::success(),
                Err(e) => FfiResult::error(&e.to_string()),
            }
        })
    }));

    result.unwrap_or_else(|_| FfiResult::error("Panic occurred"))
}

/// Return a bitmask of which image variants are present.
///
/// - bit 0 (1) = original
/// - bit 1 (2) = enhanced
/// - bit 2 (4) = back
///
/// Returns 0 if the stack pointer is null.
#[no_mangle]
pub unsafe extern "C" fn photostax_stack_images_present(stack: *const PhotostaxStack) -> u8 {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if stack.is_null() {
            return 0;
        }
        let inner = unsafe { &*stack };
        inner.inner.images_present().bits()
    }));
    result.unwrap_or(0)
}

// ── MetadataRef FFI ──────────────────────────────────────────────────────────

/// Check whether metadata has been loaded (cached) for this stack.
#[no_mangle]
pub unsafe extern "C" fn photostax_stack_metadata_is_loaded(stack: *const PhotostaxStack) -> bool {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if stack.is_null() {
            return false;
        }
        let inner = unsafe { &*stack };
        inner
            .runtime
            .block_on(async { inner.inner.metadata().is_loaded() })
    }));
    result.unwrap_or(false)
}

/// Load metadata from the backing store and return it as a JSON string.
///
/// This triggers a read from disk if the metadata has not been loaded yet.
/// Returns null on error. Caller owns the returned string and must call
/// [`photostax_string_free`].
#[no_mangle]
pub unsafe extern "C" fn photostax_stack_metadata_read(
    stack: *const PhotostaxStack,
) -> *mut c_char {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if stack.is_null() {
            return ptr::null_mut();
        }
        let inner = unsafe { &*stack };
        inner.runtime.block_on(async {
            let metadata = match inner.inner.metadata().read() {
                Ok(m) => m,
                Err(_) => return ptr::null_mut(),
            };

            let metadata_json = serde_json::json!({
                "exif_tags": metadata.exif_tags,
                "xmp_tags": metadata.xmp_tags,
                "custom_tags": metadata.custom_tags,
            });

            let json_str =
                serde_json::to_string(&metadata_json).unwrap_or_else(|_| "{}".to_string());
            CString::new(json_str)
                .map(|s| s.into_raw())
                .unwrap_or(ptr::null_mut())
        })
    }));

    result.unwrap_or(ptr::null_mut())
}

/// Return cached metadata as a JSON string without triggering a load.
///
/// Returns null if metadata has not been loaded yet or the stack is null.
/// Caller owns the returned string and must call [`photostax_string_free`].
#[no_mangle]
pub unsafe extern "C" fn photostax_stack_metadata_cached(
    stack: *const PhotostaxStack,
) -> *mut c_char {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if stack.is_null() {
            return ptr::null_mut();
        }
        let inner = unsafe { &*stack };
        inner.runtime.block_on(async {
            match inner.inner.metadata().cached() {
                Some(metadata) => {
                    let metadata_json = serde_json::json!({
                        "exif_tags": metadata.exif_tags,
                        "xmp_tags": metadata.xmp_tags,
                        "custom_tags": metadata.custom_tags,
                    });
                    let json_str =
                        serde_json::to_string(&metadata_json).unwrap_or_else(|_| "{}".to_string());
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

/// Write metadata to the stack's backing store.
///
/// `json` must be a JSON object with optional keys: `exif_tags`, `xmp_tags`,
/// `custom_tags`.
#[no_mangle]
pub unsafe extern "C" fn photostax_stack_metadata_write(
    stack: *const PhotostaxStack,
    json: *const c_char,
) -> FfiResult {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if stack.is_null() {
            return FfiResult::error("Stack pointer is null");
        }
        if json.is_null() {
            return FfiResult::error("Metadata JSON pointer is null");
        }

        let metadata_str = match unsafe { CStr::from_ptr(json) }.to_str() {
            Ok(s) => s,
            Err(_) => return FfiResult::error("Invalid UTF-8 in metadata JSON"),
        };

        let input: MetadataInput = match serde_json::from_str(metadata_str) {
            Ok(m) => m,
            Err(e) => return FfiResult::error(&format!("Invalid metadata JSON: {e}")),
        };

        let metadata = photostax_core::photo_stack::Metadata {
            exif_tags: input.exif_tags,
            xmp_tags: input.xmp_tags,
            custom_tags: input.custom_tags,
        };

        let inner = unsafe { &*stack };
        inner.runtime.block_on(async {
            match inner.inner.metadata().write(&metadata) {
                Ok(()) => FfiResult::success(),
                Err(e) => FfiResult::error(&e.to_string()),
            }
        })
    }));

    result.unwrap_or_else(|_| FfiResult::error("Panic occurred"))
}

/// Invalidate cached metadata, forcing a re-read on next access.
#[no_mangle]
pub unsafe extern "C" fn photostax_stack_metadata_invalidate(stack: *const PhotostaxStack) {
    let _ = panic::catch_unwind(AssertUnwindSafe(|| {
        if stack.is_null() {
            return;
        }
        let inner = unsafe { &*stack };
        inner.runtime.block_on(async {
            inner.inner.metadata().invalidate();
        });
    }));
}

// ── StackManager multi-repo ──────────────────────────────────────────────────

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
        let runtime = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(_) => return ptr::null_mut(),
        };
        let mgr = StackManager::new();
        let boxed = Box::new(PhotostaxRepo {
            inner: tokio::sync::Mutex::new(mgr),
            runtime,
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
        let mut mgr_inner = mgr_ref.runtime.block_on(mgr_ref.inner.lock());
        match mgr_inner.add_repo(Box::new(repo), scanner_profile) {
            Ok(_) => FfiResult::success(),
            Err(e) => FfiResult::error(&e.to_string()),
        }
    }));
    result.unwrap_or_else(|_| FfiResult::error("panic in photostax_manager_add_repo"))
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

        let provider =
            match unsafe { crate::foreign_provider::FfiRepositoryProvider::new(callbacks) } {
                Ok(p) => p,
                Err(e) => return FfiResult::error(&format!("invalid provider: {e}")),
            };

        let config = ScannerConfig {
            recursive,
            ..ScannerConfig::default()
        };
        let repo = photostax_core::backends::foreign::ForeignRepository::with_config(
            Box::new(provider),
            config,
        );

        let scanner_profile = ScannerProfile::from_int(profile).unwrap_or_default();

        let mgr_ref = unsafe { &*mgr };
        let mut mgr_inner = mgr_ref.runtime.block_on(mgr_ref.inner.lock());
        match mgr_inner.add_repo(Box::new(repo), scanner_profile) {
            Ok(_) => FfiResult::success(),
            Err(e) => FfiResult::error(&e.to_string()),
        }
    }));
    result.unwrap_or_else(|_| FfiResult::error("panic in photostax_manager_add_foreign_repo"))
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
        let mgr_inner = mgr_ref.runtime.block_on(mgr_ref.inner.lock());
        mgr_inner.repo_count()
    }));
    result.unwrap_or(0)
}

// ── Free functions ───────────────────────────────────────────────────────────

/// Free a single opaque stack handle.
///
/// # Safety
///
/// - `stack` must have been returned by [`photostax_repo_get_stack`] or from a
///   handle array, or be null
/// - After calling, `stack` is invalid and must not be used
#[no_mangle]
pub unsafe extern "C" fn photostax_stack_free(stack: *mut PhotostaxStack) {
    let _ = panic::catch_unwind(AssertUnwindSafe(|| {
        if !stack.is_null() {
            drop(Box::from_raw(stack));
        }
    }));
}

/// Free an array of opaque stack handles.
///
/// This frees every handle in the array and the array itself.
///
/// # Safety
///
/// - `array` must have been returned by [`photostax_repo_scan`] or similar
/// - After calling, all handles within `array` are invalid
#[no_mangle]
pub unsafe extern "C" fn photostax_stack_handle_array_free(array: FfiStackHandleArray) {
    let _ = panic::catch_unwind(AssertUnwindSafe(|| {
        if !array.handles.is_null() && array.len > 0 {
            let slice = std::slice::from_raw_parts_mut(array.handles, array.len);
            for &mut handle in slice.iter_mut() {
                if !handle.is_null() {
                    drop(Box::from_raw(handle));
                }
            }
            drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(
                array.handles,
                array.len,
            )));
        }
    }));
}

/// Free a paginated handle result.
///
/// This frees every handle in the result and the array itself.
///
/// # Safety
///
/// - `result` must have been returned by a paginated FFI function
/// - After calling, all handles within `result` are invalid
#[no_mangle]
pub unsafe extern "C" fn photostax_paginated_handle_result_free(result: FfiPaginatedHandleResult) {
    let _ = panic::catch_unwind(AssertUnwindSafe(|| {
        if !result.handles.is_null() && result.len > 0 {
            let slice = std::slice::from_raw_parts_mut(result.handles, result.len);
            for &mut handle in slice.iter_mut() {
                if !handle.is_null() {
                    drop(Box::from_raw(handle));
                }
            }
            drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(
                result.handles,
                result.len,
            )));
        }
    }));
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

// ── Tests ────────────────────────────────────────────────────────────────────

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
        let slice = unsafe { std::slice::from_raw_parts(array.handles, array.len) };
        let found = slice.iter().find(|&&handle| {
            let name_ptr = unsafe { photostax_stack_name(handle) };
            let n = unsafe { CStr::from_ptr(name_ptr) }
                .to_str()
                .unwrap()
                .to_string();
            unsafe { photostax_string_free(name_ptr) };
            n == name
        });
        let handle = *found.expect("stack not found by name");
        let id_ptr = unsafe { photostax_stack_id(handle) };
        let id = unsafe { CStr::from_ptr(id_ptr) }
            .to_str()
            .unwrap()
            .to_string();
        unsafe { photostax_string_free(id_ptr) };
        unsafe { photostax_stack_handle_array_free(array) };
        id
    }

    /// Create a real JPEG in the given directory with known dimensions.
    fn create_test_image_jpeg(path: &std::path::Path, width: u32, height: u32) {
        let img = image::RgbImage::from_fn(width, height, |x, y| image::Rgb([x as u8, y as u8, 0]));
        img.save(path).unwrap();
    }

    // ── Repository lifecycle tests ───────────────────────────────────────

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
    fn test_repo_open_invalid_utf8() {
        let invalid: &[u8] = &[0xff, 0xfe, 0x00];
        let result = unsafe { photostax_repo_open(invalid.as_ptr() as *const c_char) };
        assert!(result.is_null());
    }

    #[test]
    fn test_repo_free_null() {
        unsafe { photostax_repo_free(ptr::null_mut()) };
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
        unsafe { photostax_stack_handle_array_free(result) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_open_recursive_scans_subdirectories() {
        let tmp = tempfile::TempDir::new().unwrap();
        let subdir = tmp.path().join("2024_Summer");
        std::fs::create_dir(&subdir).unwrap();
        let jpeg = vec![0xFF, 0xD8, 0xFF, 0xD9];
        std::fs::write(subdir.join("IMG_001.jpg"), &jpeg).unwrap();

        let path = CString::new(tmp.path().to_string_lossy().as_ref()).unwrap();
        let repo_flat = unsafe { photostax_repo_open_recursive(path.as_ptr(), false) };
        assert!(!repo_flat.is_null());
        let result_flat = unsafe { photostax_repo_scan(repo_flat) };
        assert_eq!(result_flat.len, 0, "flat scan should find nothing at root");
        unsafe { photostax_stack_handle_array_free(result_flat) };
        unsafe { photostax_repo_free(repo_flat) };

        let repo_rec = unsafe { photostax_repo_open_recursive(path.as_ptr(), true) };
        assert!(!repo_rec.is_null());
        let result_rec = unsafe { photostax_repo_scan(repo_rec) };
        assert_eq!(result_rec.len, 1, "recursive scan should find 1 stack");
        unsafe { photostax_stack_handle_array_free(result_rec) };
        unsafe { photostax_repo_free(repo_rec) };
    }

    #[test]
    fn test_utf8_edge_cases() {
        let path = CString::new("/tmp/test_photostax_unicode/photo.jpg").unwrap();
        let repo = unsafe { photostax_repo_open(path.as_ptr()) };
        assert!(!repo.is_null());
        unsafe { photostax_repo_free(repo) };
    }

    // ── Scan tests ───────────────────────────────────────────────────────

    #[test]
    fn test_repo_scan_null() {
        let array = unsafe { photostax_repo_scan(ptr::null()) };
        assert!(array.handles.is_null());
        assert_eq!(array.len, 0);
    }

    #[test]
    fn test_repo_scan_with_testdata() {
        let repo = open_testdata_repo();
        let array = unsafe { photostax_repo_scan(repo) };
        assert!(array.len > 0);

        let first = unsafe {
            *std::slice::from_raw_parts(array.handles, array.len)
                .first()
                .unwrap()
        };
        let id_ptr = unsafe { photostax_stack_id(first) };
        assert!(!id_ptr.is_null());
        let id_str = unsafe { CStr::from_ptr(id_ptr) }.to_str().unwrap();
        assert!(!id_str.is_empty());
        unsafe { photostax_string_free(id_ptr) };

        unsafe { photostax_stack_handle_array_free(array) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_repo_scan_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let path = CString::new(dir.path().to_str().unwrap()).unwrap();
        let repo = unsafe { photostax_repo_open(path.as_ptr()) };
        assert!(!repo.is_null());

        let array = unsafe { photostax_repo_scan(repo) };
        assert!(array.handles.is_null());
        assert_eq!(array.len, 0);

        unsafe { photostax_stack_handle_array_free(array) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_scan_error_on_invalid_repo() {
        let path = CString::new("/nonexistent/ffi/repo/dir").unwrap();
        let repo = unsafe { photostax_repo_open(path.as_ptr()) };
        assert!(!repo.is_null());
        let result = unsafe { photostax_repo_scan(repo) };
        unsafe { photostax_stack_handle_array_free(result) };
        unsafe { photostax_repo_free(repo) };
    }

    // ── get_stack tests ──────────────────────────────────────────────────

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
    fn test_repo_get_stack_invalid_utf8_id() {
        let repo = open_testdata_repo();
        let invalid: &[u8] = &[0xff, 0x00];
        let result = unsafe { photostax_repo_get_stack(repo, invalid.as_ptr() as *const c_char) };
        assert!(result.is_null());
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_repo_get_stack_happy_path() {
        let repo = open_testdata_repo();

        let opaque_id = find_stack_id_by_name(repo, "FamilyPhotos_0001");
        let id = CString::new(opaque_id).unwrap();
        let stack_ptr = unsafe { photostax_repo_get_stack(repo, id.as_ptr()) };
        assert!(!stack_ptr.is_null(), "Stack FamilyPhotos_0001 should exist");

        let name_ptr = unsafe { photostax_stack_name(stack_ptr) };
        let name_str = unsafe { CStr::from_ptr(name_ptr) }.to_str().unwrap();
        assert_eq!(name_str, "FamilyPhotos_0001");
        unsafe { photostax_string_free(name_ptr) };

        assert!(unsafe { photostax_stack_image_is_present(stack_ptr, 0) });

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

    // ── Stack accessor tests ─────────────────────────────────────────────

    #[test]
    fn test_stack_id_null() {
        let result = unsafe { photostax_stack_id(ptr::null()) };
        assert!(result.is_null());
    }

    #[test]
    fn test_stack_name_null() {
        let result = unsafe { photostax_stack_name(ptr::null()) };
        assert!(result.is_null());
    }

    #[test]
    fn test_stack_folder_null() {
        let result = unsafe { photostax_stack_folder(ptr::null()) };
        assert!(result.is_null());
    }

    #[test]
    fn test_stack_accessors_with_testdata() {
        let repo = open_testdata_repo();
        let array = unsafe { photostax_repo_scan(repo) };
        assert!(array.len > 0);

        let first = unsafe {
            *std::slice::from_raw_parts(array.handles, array.len)
                .first()
                .unwrap()
        };

        let id_ptr = unsafe { photostax_stack_id(first) };
        assert!(!id_ptr.is_null());
        let id_str = unsafe { CStr::from_ptr(id_ptr) }.to_str().unwrap();
        assert!(!id_str.is_empty());
        unsafe { photostax_string_free(id_ptr) };

        let name_ptr = unsafe { photostax_stack_name(first) };
        assert!(!name_ptr.is_null());
        let name_str = unsafe { CStr::from_ptr(name_ptr) }.to_str().unwrap();
        assert!(!name_str.is_empty());
        unsafe { photostax_string_free(name_ptr) };

        let folder_ptr = unsafe { photostax_stack_folder(first) };
        if !folder_ptr.is_null() {
            unsafe { photostax_string_free(folder_ptr) };
        }

        unsafe { photostax_stack_handle_array_free(array) };
        unsafe { photostax_repo_free(repo) };
    }

    // ── ImageRef FFI tests ───────────────────────────────────────────────

    #[test]
    fn test_image_is_present_null() {
        assert!(!unsafe { photostax_stack_image_is_present(ptr::null(), 0) });
    }

    #[test]
    fn test_image_is_present_invalid_variant() {
        let repo = open_testdata_repo();
        let array = unsafe { photostax_repo_scan(repo) };
        assert!(array.len > 0);
        let first = unsafe {
            *std::slice::from_raw_parts(array.handles, array.len)
                .first()
                .unwrap()
        };

        assert!(!unsafe { photostax_stack_image_is_present(first, 99) });

        unsafe { photostax_stack_handle_array_free(array) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_image_is_valid_null() {
        assert!(!unsafe { photostax_stack_image_is_valid(ptr::null(), 0) });
    }

    #[test]
    fn test_image_size_null() {
        assert_eq!(unsafe { photostax_stack_image_size(ptr::null(), 0) }, -1);
    }

    #[test]
    fn test_image_size_with_testdata() {
        let repo = open_testdata_repo();
        let opaque_id = find_stack_id_by_name(repo, "FamilyPhotos_0001");
        let id = CString::new(opaque_id).unwrap();
        let stack_ptr = unsafe { photostax_repo_get_stack(repo, id.as_ptr()) };
        assert!(!stack_ptr.is_null());

        let size = unsafe { photostax_stack_image_size(stack_ptr, 0) };
        assert!(size > 0, "Original image should have positive size");

        unsafe { photostax_stack_free(stack_ptr) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_image_read_null_stack() {
        let mut data: *mut u8 = ptr::null_mut();
        let mut len: usize = 0;
        let result = unsafe { photostax_stack_image_read(ptr::null(), 0, &mut data, &mut len) };
        assert!(!result.success);
        unsafe { photostax_string_free(result.error_message) };
    }

    #[test]
    fn test_image_read_null_out_pointers() {
        let repo = open_testdata_repo();
        let array = unsafe { photostax_repo_scan(repo) };
        let first = unsafe {
            *std::slice::from_raw_parts(array.handles, array.len)
                .first()
                .unwrap()
        };

        let result =
            unsafe { photostax_stack_image_read(first, 0, ptr::null_mut(), ptr::null_mut()) };
        assert!(!result.success);
        unsafe { photostax_string_free(result.error_message) };

        unsafe { photostax_stack_handle_array_free(array) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_image_read_happy_path() {
        let repo = open_testdata_repo();
        let opaque_id = find_stack_id_by_name(repo, "FamilyPhotos_0001");
        let id = CString::new(opaque_id).unwrap();
        let stack_ptr = unsafe { photostax_repo_get_stack(repo, id.as_ptr()) };
        assert!(!stack_ptr.is_null());

        let mut out_data: *mut u8 = ptr::null_mut();
        let mut out_len: usize = 0;
        let result =
            unsafe { photostax_stack_image_read(stack_ptr, 0, &mut out_data, &mut out_len) };
        assert!(result.success, "read should succeed for existing stack");
        assert!(!out_data.is_null());
        assert!(out_len > 0);

        let first_two = unsafe { std::slice::from_raw_parts(out_data, 2) };
        assert_eq!(first_two, &[0xFF, 0xD8], "Should be JPEG data");

        unsafe { photostax_bytes_free(out_data, out_len) };
        unsafe { photostax_stack_free(stack_ptr) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_image_read_invalid_variant() {
        let repo = open_testdata_repo();
        let opaque_id = find_stack_id_by_name(repo, "FamilyPhotos_0001");
        let id = CString::new(opaque_id).unwrap();
        let stack_ptr = unsafe { photostax_repo_get_stack(repo, id.as_ptr()) };
        assert!(!stack_ptr.is_null());

        let mut out_data: *mut u8 = ptr::null_mut();
        let mut out_len: usize = 0;
        let result =
            unsafe { photostax_stack_image_read(stack_ptr, 99, &mut out_data, &mut out_len) };
        assert!(!result.success);
        unsafe { photostax_string_free(result.error_message) };

        unsafe { photostax_stack_free(stack_ptr) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_image_hash_null() {
        let result = unsafe { photostax_stack_image_hash(ptr::null(), 0) };
        assert!(result.is_null());
    }

    #[test]
    fn test_image_hash_with_testdata() {
        let repo = open_testdata_repo();
        let opaque_id = find_stack_id_by_name(repo, "FamilyPhotos_0001");
        let id = CString::new(opaque_id).unwrap();
        let stack_ptr = unsafe { photostax_repo_get_stack(repo, id.as_ptr()) };
        assert!(!stack_ptr.is_null());

        let hash_ptr = unsafe { photostax_stack_image_hash(stack_ptr, 0) };
        assert!(!hash_ptr.is_null(), "Should compute hash for original");
        let hash_str = unsafe { CStr::from_ptr(hash_ptr) }.to_str().unwrap();
        assert!(!hash_str.is_empty());
        unsafe { photostax_string_free(hash_ptr) };

        unsafe { photostax_stack_free(stack_ptr) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_image_dimensions_null() {
        let dims = unsafe { photostax_stack_image_dimensions(ptr::null(), 0) };
        assert!(!dims.success);
    }

    #[test]
    fn test_image_dimensions_with_testdata() {
        let repo = open_testdata_repo();
        let opaque_id = find_stack_id_by_name(repo, "FamilyPhotos_0001");
        let id = CString::new(opaque_id).unwrap();
        let stack_ptr = unsafe { photostax_repo_get_stack(repo, id.as_ptr()) };
        assert!(!stack_ptr.is_null());

        let dims = unsafe { photostax_stack_image_dimensions(stack_ptr, 0) };
        assert!(dims.success, "Should get dimensions for original");
        assert!(dims.width > 0);
        assert!(dims.height > 0);

        unsafe { photostax_stack_free(stack_ptr) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_image_rotate_null() {
        let result = unsafe { photostax_stack_image_rotate(ptr::null(), 0, 90) };
        assert!(!result.success);
        unsafe { photostax_string_free(result.error_message) };
    }

    #[test]
    fn test_image_rotate_invalid_degrees() {
        let dir = tempfile::tempdir().unwrap();
        create_test_image_jpeg(&dir.path().join("IMG_001.jpg"), 4, 2);

        let path = CString::new(dir.path().to_str().unwrap()).unwrap();
        let repo = unsafe { photostax_repo_open(path.as_ptr()) };
        let opaque_id = find_stack_id_by_name(repo, "IMG_001");
        let id = CString::new(opaque_id).unwrap();
        let stack_ptr = unsafe { photostax_repo_get_stack(repo, id.as_ptr()) };
        assert!(!stack_ptr.is_null());

        let result = unsafe { photostax_stack_image_rotate(stack_ptr, 0, 45) };
        assert!(!result.success);
        unsafe { photostax_string_free(result.error_message) };

        unsafe { photostax_stack_free(stack_ptr) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_image_rotate_happy_path() {
        let dir = tempfile::tempdir().unwrap();
        create_test_image_jpeg(&dir.path().join("IMG_001.jpg"), 4, 2);
        create_test_image_jpeg(&dir.path().join("IMG_001_a.jpg"), 4, 2);

        let path = CString::new(dir.path().to_str().unwrap()).unwrap();
        let repo = unsafe { photostax_repo_open(path.as_ptr()) };
        let opaque_id = find_stack_id_by_name(repo, "IMG_001");
        let id = CString::new(opaque_id).unwrap();
        let stack_ptr = unsafe { photostax_repo_get_stack(repo, id.as_ptr()) };
        assert!(!stack_ptr.is_null());

        let result = unsafe { photostax_stack_image_rotate(stack_ptr, 0, 90) };
        assert!(result.success, "rotate should succeed");

        // Verify file dimensions changed (4x2 -> 2x4 after 90 CW)
        let img = image::open(dir.path().join("IMG_001.jpg")).unwrap();
        assert_eq!(img.width(), 2);
        assert_eq!(img.height(), 4);

        unsafe { photostax_stack_free(stack_ptr) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_image_invalidate_null() {
        unsafe { photostax_stack_image_invalidate(ptr::null(), 0) };
    }

    #[test]
    fn test_image_invalidate_invalid_variant() {
        let repo = open_testdata_repo();
        let array = unsafe { photostax_repo_scan(repo) };
        let first = unsafe {
            *std::slice::from_raw_parts(array.handles, array.len)
                .first()
                .unwrap()
        };

        unsafe { photostax_stack_image_invalidate(first, 99) };

        unsafe { photostax_stack_handle_array_free(array) };
        unsafe { photostax_repo_free(repo) };
    }

    // ── MetadataRef FFI tests ────────────────────────────────────────────

    #[test]
    fn test_metadata_is_loaded_null() {
        assert!(!unsafe { photostax_stack_metadata_is_loaded(ptr::null()) });
    }

    #[test]
    fn test_metadata_is_loaded_before_read() {
        let repo = open_testdata_repo();
        let opaque_id = find_stack_id_by_name(repo, "FamilyPhotos_0001");
        let id = CString::new(opaque_id).unwrap();
        let stack_ptr = unsafe { photostax_repo_get_stack(repo, id.as_ptr()) };
        assert!(!stack_ptr.is_null());

        assert!(!unsafe { photostax_stack_metadata_is_loaded(stack_ptr) });

        unsafe { photostax_stack_free(stack_ptr) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_metadata_read_null() {
        let result = unsafe { photostax_stack_metadata_read(ptr::null()) };
        assert!(result.is_null());
    }

    #[test]
    fn test_metadata_read_happy_path() {
        let repo = open_testdata_repo();
        let opaque_id = find_stack_id_by_name(repo, "FamilyPhotos_0001");
        let id = CString::new(opaque_id).unwrap();
        let stack_ptr = unsafe { photostax_repo_get_stack(repo, id.as_ptr()) };
        assert!(!stack_ptr.is_null());

        let meta_ptr = unsafe { photostax_stack_metadata_read(stack_ptr) };
        assert!(!meta_ptr.is_null(), "Should load metadata");

        let meta_str = unsafe { CStr::from_ptr(meta_ptr) }.to_str().unwrap();
        let meta: serde_json::Value = serde_json::from_str(meta_str).unwrap();
        assert!(meta["exif_tags"].is_object());
        assert!(meta["xmp_tags"].is_object());
        assert!(meta["custom_tags"].is_object());
        assert!(
            meta["exif_tags"].as_object().unwrap().contains_key("Make"),
            "metadata_read should populate file-based EXIF tags"
        );

        assert!(unsafe { photostax_stack_metadata_is_loaded(stack_ptr) });

        unsafe { photostax_string_free(meta_ptr) };
        unsafe { photostax_stack_free(stack_ptr) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_metadata_cached_null() {
        let result = unsafe { photostax_stack_metadata_cached(ptr::null()) };
        assert!(result.is_null());
    }

    #[test]
    fn test_metadata_cached_before_read() {
        let repo = open_testdata_repo();
        let opaque_id = find_stack_id_by_name(repo, "FamilyPhotos_0001");
        let id = CString::new(opaque_id).unwrap();
        let stack_ptr = unsafe { photostax_repo_get_stack(repo, id.as_ptr()) };
        assert!(!stack_ptr.is_null());

        let cached_ptr = unsafe { photostax_stack_metadata_cached(stack_ptr) };
        assert!(cached_ptr.is_null(), "Should be null before metadata read");

        let _ = unsafe { photostax_stack_metadata_read(stack_ptr) };
        let cached_ptr2 = unsafe { photostax_stack_metadata_cached(stack_ptr) };
        assert!(!cached_ptr2.is_null(), "Should be populated after read");
        unsafe { photostax_string_free(cached_ptr2) };

        unsafe { photostax_stack_free(stack_ptr) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_metadata_write_null_stack() {
        let json = CString::new("{}").unwrap();
        let result = unsafe { photostax_stack_metadata_write(ptr::null(), json.as_ptr()) };
        assert!(!result.success);
        unsafe { photostax_string_free(result.error_message) };
    }

    #[test]
    fn test_metadata_write_null_json() {
        let repo = open_testdata_repo();
        let opaque_id = find_stack_id_by_name(repo, "FamilyPhotos_0001");
        let id = CString::new(opaque_id).unwrap();
        let stack_ptr = unsafe { photostax_repo_get_stack(repo, id.as_ptr()) };
        assert!(!stack_ptr.is_null());

        let result = unsafe { photostax_stack_metadata_write(stack_ptr, ptr::null()) };
        assert!(!result.success);
        unsafe { photostax_string_free(result.error_message) };

        unsafe { photostax_stack_free(stack_ptr) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_metadata_write_invalid_json() {
        let repo = open_testdata_repo();
        let opaque_id = find_stack_id_by_name(repo, "FamilyPhotos_0001");
        let id = CString::new(opaque_id).unwrap();
        let stack_ptr = unsafe { photostax_repo_get_stack(repo, id.as_ptr()) };
        assert!(!stack_ptr.is_null());

        let json = CString::new("not valid json").unwrap();
        let result = unsafe { photostax_stack_metadata_write(stack_ptr, json.as_ptr()) };
        assert!(!result.success);
        unsafe { photostax_string_free(result.error_message) };

        unsafe { photostax_stack_free(stack_ptr) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_metadata_write_happy_path() {
        let dir = tempfile::tempdir().unwrap();
        for entry in std::fs::read_dir(testdata_path()).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_file() {
                std::fs::copy(entry.path(), dir.path().join(entry.file_name())).unwrap();
            }
        }

        let path = CString::new(dir.path().to_str().unwrap()).unwrap();
        let repo = unsafe { photostax_repo_open(path.as_ptr()) };
        assert!(!repo.is_null());

        let opaque_id = find_stack_id_by_name(repo, "FamilyPhotos_0001");
        let id = CString::new(opaque_id).unwrap();
        let stack_ptr = unsafe { photostax_repo_get_stack(repo, id.as_ptr()) };
        assert!(!stack_ptr.is_null());

        let json = CString::new(r#"{"custom_tags":{"album":"Family"}}"#).unwrap();
        let result = unsafe { photostax_stack_metadata_write(stack_ptr, json.as_ptr()) };
        assert!(result.success, "write_metadata should succeed");

        unsafe { photostax_stack_free(stack_ptr) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_metadata_invalidate_null() {
        unsafe { photostax_stack_metadata_invalidate(ptr::null()) };
    }

    #[test]
    fn test_metadata_invalidate_clears_loaded() {
        let repo = open_testdata_repo();
        let opaque_id = find_stack_id_by_name(repo, "FamilyPhotos_0001");
        let id = CString::new(opaque_id).unwrap();
        let stack_ptr = unsafe { photostax_repo_get_stack(repo, id.as_ptr()) };
        assert!(!stack_ptr.is_null());

        let meta_ptr = unsafe { photostax_stack_metadata_read(stack_ptr) };
        assert!(!meta_ptr.is_null());
        unsafe { photostax_string_free(meta_ptr) };
        assert!(unsafe { photostax_stack_metadata_is_loaded(stack_ptr) });

        unsafe { photostax_stack_metadata_invalidate(stack_ptr) };
        assert!(
            !unsafe { photostax_stack_metadata_is_loaded(stack_ptr) },
            "After invalidate, is_loaded should be false"
        );

        unsafe { photostax_stack_free(stack_ptr) };
        unsafe { photostax_repo_free(repo) };
    }

    // ── Free function tests ──────────────────────────────────────────────

    #[test]
    fn test_stack_free_null() {
        unsafe { photostax_stack_free(ptr::null_mut()) };
    }

    #[test]
    fn test_stack_handle_array_free_empty() {
        let array = FfiStackHandleArray::empty();
        unsafe { photostax_stack_handle_array_free(array) };
    }

    #[test]
    fn test_paginated_handle_result_free_empty() {
        let result = FfiPaginatedHandleResult::empty(0, 10);
        unsafe { photostax_paginated_handle_result_free(result) };
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

    // ── scan_paginated tests ─────────────────────────────────────────────

    #[test]
    fn test_scan_paginated_null() {
        let result = unsafe { photostax_repo_scan_paginated(ptr::null(), 0, 10, false) };
        assert!(result.handles.is_null());
        assert_eq!(result.len, 0);
    }

    #[test]
    fn test_scan_paginated_with_metadata() {
        let repo = open_testdata_repo();
        let page = unsafe { photostax_repo_scan_paginated(repo, 0, 2, true) };
        assert!(page.total_count > 0, "Expected stacks from testdata");
        assert!(page.len > 0);

        let first = unsafe {
            *std::slice::from_raw_parts(page.handles, page.len)
                .first()
                .unwrap()
        };

        assert!(unsafe { photostax_stack_metadata_is_loaded(first) });
        let cached_ptr = unsafe { photostax_stack_metadata_cached(first) };
        assert!(!cached_ptr.is_null());
        let meta_str = unsafe { CStr::from_ptr(cached_ptr) }.to_str().unwrap();
        let meta: serde_json::Value = serde_json::from_str(meta_str).unwrap();
        assert!(
            meta["exif_tags"].as_object().unwrap().contains_key("Make"),
            "With load_metadata=true, EXIF should be populated"
        );
        unsafe { photostax_string_free(cached_ptr) };

        unsafe { photostax_paginated_handle_result_free(page) };
        unsafe { photostax_repo_free(repo) };
    }

    #[test]
    fn test_scan_paginated_without_metadata() {
        let repo = open_testdata_repo();
        let page = unsafe { photostax_repo_scan_paginated(repo, 0, 2, false) };
        assert!(page.total_count > 0, "Expected stacks from testdata");
        assert!(page.len > 0);

        let first = unsafe {
            *std::slice::from_raw_parts(page.handles, page.len)
                .first()
                .unwrap()
        };

        let cached_ptr = unsafe { photostax_stack_metadata_cached(first) };
        if !cached_ptr.is_null() {
            let meta_str = unsafe { CStr::from_ptr(cached_ptr) }.to_str().unwrap();
            let meta: serde_json::Value = serde_json::from_str(meta_str).unwrap();
            assert!(
                meta["exif_tags"].as_object().unwrap().is_empty()
                    || !meta["exif_tags"].as_object().unwrap().contains_key("Make"),
                "With load_metadata=false, EXIF should not be populated"
            );
            unsafe { photostax_string_free(cached_ptr) };
        }

        unsafe { photostax_paginated_handle_result_free(page) };
        unsafe { photostax_repo_free(repo) };
    }

    // ── query tests ──────────────────────────────────────────────────────

    #[test]
    fn test_query_null() {
        let result =
            unsafe { photostax_query(ptr::null(), ptr::null(), 0, 0, None, ptr::null_mut()) };
        assert!(result.handles.is_null());
        assert_eq!(result.len, 0);
    }

    #[test]
    fn test_query_all() {
        let repo = open_testdata_repo();
        let result = unsafe { photostax_query(repo, ptr::null(), 0, 0, None, ptr::null_mut()) };
        assert!(result.total_count > 0);
        assert!(result.len > 0);

        unsafe { photostax_paginated_handle_result_free(result) };
        unsafe { photostax_repo_free(repo) };
    }

    // ── StackManager multi-repo tests ────────────────────────────────────

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
        unsafe { photostax_stack_handle_array_free(array) };
        unsafe { photostax_repo_free(mgr) };
    }

    #[test]
    fn test_manager_add_multiple_repos() {
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

        let r1 = unsafe { photostax_manager_add_repo(mgr, path1.as_ptr(), false, 0) };
        assert!(r1.success);
        let r2 = unsafe { photostax_manager_add_repo(mgr, path2.as_ptr(), false, 0) };
        assert!(r2.success);
        assert_eq!(unsafe { photostax_manager_repo_count(mgr) }, 2);

        let array = unsafe { photostax_repo_scan(mgr) };
        assert!(array.len > 0);
        unsafe { photostax_stack_handle_array_free(array) };
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

        let result = unsafe { photostax_query(mgr, ptr::null(), 0, 0, None, ptr::null_mut()) };
        assert!(result.total_count > 0);
        unsafe { photostax_paginated_handle_result_free(result) };
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

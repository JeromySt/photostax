//! C-compatible type definitions for FFI.
//!
//! All types in this module are `#[repr(C)]` to ensure consistent memory layout
//! across the FFI boundary. Pointers returned from FFI functions must be freed
//! using the corresponding `*_free` functions.

use std::cell::RefCell;
use std::os::raw::c_char;

/// Opaque handle to a [`StackManager`].
///
/// This type is opaque to C code and should only be manipulated through
/// the FFI functions. Create with [`photostax_repo_open`] and free with
/// [`photostax_repo_free`].
///
/// Internally uses [`RefCell`] because `StackManager` mutation methods
/// (`scan`, `load_metadata`, `rotate_stack`, etc.) require `&mut self`,
/// while the FFI functions receive `*const PhotostaxRepo`.
///
/// [`StackManager`]: photostax_core::stack_manager::StackManager
/// [`photostax_repo_open`]: crate::repository::photostax_repo_open
/// [`photostax_repo_free`]: crate::repository::photostax_repo_free
pub struct PhotostaxRepo {
    pub(crate) inner: RefCell<photostax_core::stack_manager::StackManager>,
}

/// A photo stack returned across FFI.
///
/// All string pointers are owned by this struct and must be freed by calling
/// [`photostax_stack_free`]. Null pointers indicate absent values.
///
/// # Memory Ownership
///
/// - Caller receives ownership of the entire struct
/// - Call [`photostax_stack_free`] to release all memory
/// - Do not free individual string fields separately
///
/// [`photostax_stack_free`]: crate::repository::photostax_stack_free
#[repr(C)]
pub struct FfiPhotoStack {
    /// Stack identifier (never null). This is an opaque hash.
    pub id: *mut c_char,
    /// Human-readable stack name, typically the file stem (never null).
    pub name: *mut c_char,
    /// Subfolder name within the repository (null if root level).
    pub folder: *mut c_char,
    /// Path to original image (null if absent).
    pub original: *mut c_char,
    /// Path to enhanced image (null if absent).
    pub enhanced: *mut c_char,
    /// Path to back image (null if absent).
    pub back: *mut c_char,
    /// JSON-serialized metadata (never null, may be "{}").
    pub metadata_json: *mut c_char,
}

/// Array of photo stacks.
///
/// # Memory Ownership
///
/// - Caller receives ownership of the entire array
/// - Call [`photostax_stack_array_free`] to release all memory
/// - Do not free individual stacks separately after freeing the array
///
/// [`photostax_stack_array_free`]: crate::repository::photostax_stack_array_free
#[repr(C)]
pub struct FfiPhotoStackArray {
    /// Pointer to array of stacks (null if len == 0).
    pub data: *mut FfiPhotoStack,
    /// Number of stacks in the array.
    pub len: usize,
}

/// Paginated result of photo stacks returned across FFI.
///
/// Contains a page of stacks along with pagination metadata needed
/// for rendering pagination controls in a web UI.
///
/// # Memory Ownership
///
/// - Caller receives ownership of the entire result
/// - Call [`photostax_paginated_result_free`] to release all memory
///
/// [`photostax_paginated_result_free`]: crate::repository::photostax_paginated_result_free
#[repr(C)]
pub struct FfiPaginatedResult {
    /// Pointer to array of stacks in this page (null if len == 0).
    pub data: *mut FfiPhotoStack,
    /// Number of stacks in this page.
    pub len: usize,
    /// Total number of stacks across all pages (before pagination).
    pub total_count: usize,
    /// The offset used for this page.
    pub offset: usize,
    /// The page size limit used for this page.
    pub limit: usize,
    /// Whether there are more items beyond this page.
    pub has_more: bool,
}

/// Result type for FFI calls.
///
/// On success, `success` is true and `error_message` is null.
/// On failure, `success` is false and `error_message` contains the error.
///
/// # Memory Ownership
///
/// - If `error_message` is non-null, caller must free it with [`photostax_string_free`]
///
/// [`photostax_string_free`]: crate::repository::photostax_string_free
#[repr(C)]
pub struct FfiResult {
    /// True if the operation succeeded.
    pub success: bool,
    /// Error message (null on success, must be freed on failure).
    pub error_message: *mut c_char,
}

impl FfiPhotoStackArray {
    /// Create an empty array.
    pub(crate) fn empty() -> Self {
        Self {
            data: std::ptr::null_mut(),
            len: 0,
        }
    }
}

impl FfiPaginatedResult {
    /// Create an empty paginated result.
    pub(crate) fn empty(offset: usize, limit: usize) -> Self {
        Self {
            data: std::ptr::null_mut(),
            len: 0,
            total_count: 0,
            offset,
            limit,
            has_more: false,
        }
    }
}

impl FfiResult {
    /// Create a success result.
    pub(crate) fn success() -> Self {
        Self {
            success: true,
            error_message: std::ptr::null_mut(),
        }
    }

    /// Create an error result with the given message.
    pub(crate) fn error(msg: &str) -> Self {
        use std::ffi::CString;
        let c_msg = CString::new(msg).unwrap_or_else(|_| CString::new("Unknown error").unwrap());
        Self {
            success: false,
            error_message: c_msg.into_raw(),
        }
    }
}

// ── Foreign repository callback types ────────────────────────────────────────

/// A file entry returned by the foreign list_entries callback.
///
/// All string pointers must remain valid until the `free_entries` callback
/// is called. The Rust side copies these strings immediately.
#[repr(C)]
pub struct FfiFileEntry {
    /// File name including extension (e.g., "IMG_001_a.jpg"). Never null.
    pub name: *const c_char,
    /// Relative folder path using forward slashes (empty string for root). Never null.
    pub folder: *const c_char,
    /// Full path or URI to the file. Never null.
    pub path: *const c_char,
    /// File size in bytes.
    pub size: u64,
}

/// Result of a list_entries callback.
#[repr(C)]
pub struct FfiFileEntryArray {
    /// Pointer to array of entries (null if len == 0).
    pub data: *const FfiFileEntry,
    /// Number of entries.
    pub len: usize,
    /// Non-zero indicates an error (entries are invalid).
    pub error: i32,
}

/// Result of an open_read or open_write callback.
#[repr(C)]
pub struct FfiStreamHandle {
    /// Opaque stream handle. Zero indicates failure.
    pub handle: u64,
    /// Non-zero indicates an error.
    pub error: i32,
}

/// Result of a read callback.
#[repr(C)]
pub struct FfiReadResult {
    /// Number of bytes actually read.
    pub bytes_read: usize,
    /// Non-zero indicates an error.
    pub error: i32,
}

/// Result of a seek callback.
#[repr(C)]
pub struct FfiSeekResult {
    /// New position after seeking.
    pub position: u64,
    /// Non-zero indicates an error.
    pub error: i32,
}

/// Result of a write callback.
#[repr(C)]
pub struct FfiWriteResult {
    /// Number of bytes actually written.
    pub bytes_written: usize,
    /// Non-zero indicates an error.
    pub error: i32,
}

/// Callback function pointers for a foreign repository provider.
///
/// The host language fills this struct with function pointers that implement
/// file I/O operations. The `ctx` pointer is passed through to every callback
/// and can be used to maintain state in the host language (e.g., a managed
/// object reference, a COM pointer, or a JavaScript reference).
///
/// # Lifetime
///
/// The `ctx` pointer and all callback functions must remain valid for the
/// lifetime of the repository (until the `StackManager` handle is freed).
///
/// # Thread Safety
///
/// Callbacks may be invoked from any Rust thread. Host implementations must
/// be thread-safe or serialize access internally.
#[repr(C)]
pub struct FfiProviderCallbacks {
    /// Opaque context pointer passed to every callback.
    pub ctx: *mut std::os::raw::c_void,

    /// Location URI for this repository (e.g., "onedrive://user/Photos").
    /// Must be a valid null-terminated UTF-8 string. Remains valid for
    /// the lifetime of the provider.
    pub location: *const c_char,

    /// List file entries under a prefix.
    ///
    /// - `ctx`: the context pointer
    /// - `prefix`: null-terminated UTF-8 folder prefix (empty string for root)
    /// - `recursive`: whether to recurse into subdirectories
    ///
    /// Returns an `FfiFileEntryArray`. The caller (Rust) copies entries
    /// immediately, then calls `free_entries` so the host can release memory.
    pub list_entries: unsafe extern "C" fn(
        ctx: *mut std::os::raw::c_void,
        prefix: *const c_char,
        recursive: bool,
    ) -> FfiFileEntryArray,

    /// Free an entry array previously returned by `list_entries`.
    pub free_entries:
        unsafe extern "C" fn(ctx: *mut std::os::raw::c_void, entries: FfiFileEntryArray),

    /// Open a file for reading.
    ///
    /// Returns an `FfiStreamHandle` with a non-zero handle on success.
    pub open_read: unsafe extern "C" fn(
        ctx: *mut std::os::raw::c_void,
        path: *const c_char,
    ) -> FfiStreamHandle,

    /// Read bytes from a stream.
    ///
    /// - `handle`: stream handle from `open_read`
    /// - `buf`: buffer to read into
    /// - `len`: maximum number of bytes to read
    pub read: unsafe extern "C" fn(
        ctx: *mut std::os::raw::c_void,
        handle: u64,
        buf: *mut u8,
        len: usize,
    ) -> FfiReadResult,

    /// Seek within a stream.
    ///
    /// - `handle`: stream handle from `open_read`
    /// - `offset`: byte offset
    /// - `whence`: 0 = from start, 1 = from current, 2 = from end
    pub seek: unsafe extern "C" fn(
        ctx: *mut std::os::raw::c_void,
        handle: u64,
        offset: i64,
        whence: i32,
    ) -> FfiSeekResult,

    /// Close a read stream.
    pub close_read: unsafe extern "C" fn(ctx: *mut std::os::raw::c_void, handle: u64),

    /// Open a file for writing.
    ///
    /// Returns an `FfiStreamHandle` with a non-zero handle on success.
    pub open_write: unsafe extern "C" fn(
        ctx: *mut std::os::raw::c_void,
        path: *const c_char,
    ) -> FfiStreamHandle,

    /// Write bytes to a stream.
    ///
    /// - `handle`: stream handle from `open_write`
    /// - `buf`: bytes to write
    /// - `len`: number of bytes to write
    pub write: unsafe extern "C" fn(
        ctx: *mut std::os::raw::c_void,
        handle: u64,
        buf: *const u8,
        len: usize,
    ) -> FfiWriteResult,

    /// Close a write stream.
    pub close_write: unsafe extern "C" fn(ctx: *mut std::os::raw::c_void, handle: u64),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::{CStr, CString};

    #[test]
    fn test_ffi_photo_stack_array_empty() {
        let array = FfiPhotoStackArray::empty();
        assert!(array.data.is_null());
        assert_eq!(array.len, 0);
    }

    #[test]
    fn test_ffi_paginated_result_empty() {
        let result = FfiPaginatedResult::empty(10, 20);
        assert!(result.data.is_null());
        assert_eq!(result.len, 0);
        assert_eq!(result.total_count, 0);
        assert_eq!(result.offset, 10);
        assert_eq!(result.limit, 20);
        assert!(!result.has_more);
    }

    #[test]
    fn test_ffi_result_success() {
        let result = FfiResult::success();
        assert!(result.success);
        assert!(result.error_message.is_null());
    }

    #[test]
    fn test_ffi_result_error_message() {
        let result = FfiResult::error("something went wrong");
        assert!(!result.success);
        assert!(!result.error_message.is_null());
        let msg = unsafe { CStr::from_ptr(result.error_message) }
            .to_str()
            .unwrap();
        assert_eq!(msg, "something went wrong");
        unsafe { drop(CString::from_raw(result.error_message)) };
    }

    #[test]
    fn test_ffi_result_error_empty_message() {
        let result = FfiResult::error("");
        assert!(!result.success);
        assert!(!result.error_message.is_null());
        let msg = unsafe { CStr::from_ptr(result.error_message) }
            .to_str()
            .unwrap();
        assert_eq!(msg, "");
        unsafe { drop(CString::from_raw(result.error_message)) };
    }

    #[test]
    fn test_photostax_repo_struct_size() {
        // PhotostaxRepo wraps a LocalRepository
        assert!(std::mem::size_of::<PhotostaxRepo>() > 0);
    }

    #[test]
    fn test_ffi_photo_stack_repr_c() {
        // Verify the struct has the expected fields and layout
        let stack = FfiPhotoStack {
            id: std::ptr::null_mut(),
            name: std::ptr::null_mut(),
            folder: std::ptr::null_mut(),
            original: std::ptr::null_mut(),
            enhanced: std::ptr::null_mut(),
            back: std::ptr::null_mut(),
            metadata_json: std::ptr::null_mut(),
        };
        assert!(stack.id.is_null());
        assert!(stack.name.is_null());
        assert!(stack.folder.is_null());
        assert!(stack.original.is_null());
        assert!(stack.enhanced.is_null());
        assert!(stack.back.is_null());
        assert!(stack.metadata_json.is_null());
    }

    #[test]
    fn test_ffi_photo_stack_array_repr_c() {
        let array = FfiPhotoStackArray {
            data: std::ptr::null_mut(),
            len: 42,
        };
        assert!(array.data.is_null());
        assert_eq!(array.len, 42);
    }

    #[test]
    fn test_ffi_result_repr_c() {
        let result = FfiResult {
            success: true,
            error_message: std::ptr::null_mut(),
        };
        assert!(result.success);
        assert!(result.error_message.is_null());
    }
}

//! C-compatible type definitions for FFI.
//!
//! All types in this module are `#[repr(C)]` to ensure consistent memory layout
//! across the FFI boundary. Pointers returned from FFI functions must be freed
//! using the corresponding `*_free` functions.

use std::os::raw::c_char;

/// Opaque handle to a LocalRepository.
///
/// This type is opaque to C code and should only be manipulated through
/// the FFI functions. Create with [`photostax_repo_open`] and free with
/// [`photostax_repo_free`].
///
/// [`photostax_repo_open`]: crate::repository::photostax_repo_open
/// [`photostax_repo_free`]: crate::repository::photostax_repo_free
pub struct PhotostaxRepo {
    pub(crate) inner: photostax_core::backends::local::LocalRepository,
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
    /// Stack identifier (never null).
    pub id: *mut c_char,
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

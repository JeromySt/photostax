//! C FFI bindings for the photostax-core library.
//!
//! This crate provides C-compatible functions for using photostax-core
//! from languages like C, C++, C#, and others via P/Invoke or similar mechanisms.

#![allow(clippy::missing_safety_doc)]

use std::ffi::CStr;
use std::os::raw::c_char;
use std::path::Path;
use std::ptr;

use photostax_core::backends::local::LocalRepository;
use photostax_core::repository::Repository;

/// Create a new local repository handle.
///
/// # Safety
/// The `path` must be a valid null-terminated UTF-8 string.
/// The returned pointer must be freed with `photostax_repository_free`.
#[no_mangle]
pub unsafe extern "C" fn photostax_repository_new(path: *const c_char) -> *mut LocalRepository {
    if path.is_null() {
        return ptr::null_mut();
    }

    let c_str = unsafe { CStr::from_ptr(path) };
    let path_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => return ptr::null_mut(),
    };

    Box::into_raw(Box::new(LocalRepository::new(Path::new(path_str))))
}

/// Free a repository handle.
///
/// # Safety
/// The `repo` must be a valid pointer returned by `photostax_repository_new`,
/// or null (in which case this is a no-op).
#[no_mangle]
pub unsafe extern "C" fn photostax_repository_free(repo: *mut LocalRepository) {
    if !repo.is_null() {
        drop(unsafe { Box::from_raw(repo) });
    }
}

/// Scan the repository and return the count of photo stacks found.
/// Returns -1 on error.
///
/// # Safety
/// The `repo` must be a valid pointer returned by `photostax_repository_new`.
#[no_mangle]
pub unsafe extern "C" fn photostax_repository_scan_count(repo: *const LocalRepository) -> i32 {
    if repo.is_null() {
        return -1;
    }

    let repo = unsafe { &*repo };
    match repo.scan() {
        Ok(stacks) => stacks.len() as i32,
        Err(_) => -1,
    }
}

/// Get the version string of the library.
///
/// # Safety
/// The returned string is statically allocated and must not be freed.
#[no_mangle]
pub extern "C" fn photostax_version() -> *const c_char {
    static VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "\0");
    VERSION.as_ptr() as *const c_char
}

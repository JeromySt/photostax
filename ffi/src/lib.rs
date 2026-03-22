//! C FFI bindings for the photostax-core library.
//!
//! This crate provides C-compatible functions for using photostax-core
//! from languages like C, C++, C#, and others via P/Invoke or similar mechanisms.

#![allow(clippy::missing_safety_doc)]

pub mod foreign_provider;
pub mod metadata;
pub mod repository;
pub mod search;
pub mod snapshot;
pub mod types;

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::{CStr, CString};

    #[test]
    fn test_repository_new_null_path() {
        let result = unsafe { photostax_repository_new(ptr::null()) };
        assert!(result.is_null());
    }

    #[test]
    fn test_repository_new_valid_path() {
        let path = CString::new(".").unwrap();
        let repo = unsafe { photostax_repository_new(path.as_ptr()) };
        assert!(!repo.is_null());
        unsafe { photostax_repository_free(repo) };
    }

    #[test]
    fn test_repository_new_with_testdata() {
        let testdata = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("core")
            .join("tests")
            .join("testdata");
        let path = CString::new(testdata.to_str().unwrap()).unwrap();
        let repo = unsafe { photostax_repository_new(path.as_ptr()) };
        assert!(!repo.is_null());
        unsafe { photostax_repository_free(repo) };
    }

    #[test]
    fn test_repository_free_null() {
        // Should not panic
        unsafe { photostax_repository_free(ptr::null_mut()) };
    }

    #[test]
    fn test_repository_scan_count_null() {
        let result = unsafe { photostax_repository_scan_count(ptr::null()) };
        assert_eq!(result, -1);
    }

    #[test]
    fn test_repository_scan_count_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let path = CString::new(dir.path().to_str().unwrap()).unwrap();
        let repo = unsafe { photostax_repository_new(path.as_ptr()) };
        assert!(!repo.is_null());

        let count = unsafe { photostax_repository_scan_count(repo) };
        assert_eq!(count, 0);

        unsafe { photostax_repository_free(repo) };
    }

    #[test]
    fn test_repository_scan_count_with_testdata() {
        let testdata = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("core")
            .join("tests")
            .join("testdata");
        let path = CString::new(testdata.to_str().unwrap()).unwrap();
        let repo = unsafe { photostax_repository_new(path.as_ptr()) };
        assert!(!repo.is_null());

        let count = unsafe { photostax_repository_scan_count(repo) };
        assert!(count > 0, "Expected at least one photo stack in testdata");

        unsafe { photostax_repository_free(repo) };
    }

    #[test]
    fn test_version_returns_valid_string() {
        let version_ptr = photostax_version();
        assert!(!version_ptr.is_null());
        let version_str = unsafe { CStr::from_ptr(version_ptr) };
        let version = version_str.to_str().unwrap();
        assert!(!version.is_empty());
        assert!(version.contains('.'), "Version should be semver: {version}");
    }

    #[test]
    fn test_version_matches_cargo_pkg() {
        let version_ptr = photostax_version();
        let version_str = unsafe { CStr::from_ptr(version_ptr) };
        let version = version_str.to_str().unwrap();
        assert_eq!(version, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn test_repository_new_invalid_utf8() {
        // Create a pointer to invalid UTF-8 bytes (null-terminated)
        let invalid: &[u8] = &[0xff, 0xfe, 0x00]; // invalid UTF-8 + null terminator
        let result = unsafe { photostax_repository_new(invalid.as_ptr() as *const c_char) };
        assert!(result.is_null());
    }

    #[test]
    fn test_repository_scan_count_invalid_dir() {
        // Create a repo pointing to a non-existent directory to trigger scan error
        let path = CString::new("/nonexistent/scan_count_test_dir").unwrap();
        let repo = unsafe { photostax_repository_new(path.as_ptr()) };
        assert!(!repo.is_null());
        let count = unsafe { photostax_repository_scan_count(repo) };
        // On some OSes this returns 0 (empty), on others -1 (error)
        assert!(count == 0 || count == -1);
        unsafe { photostax_repository_free(repo) };
    }
}

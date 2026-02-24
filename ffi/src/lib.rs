//! # photostax-ffi
//!
//! C FFI bindings for the photostax-core library.
//!
//! This crate provides a C-compatible API for accessing photostax-core functionality
//! from other languages via FFI (Foreign Function Interface). It supports:
//!
//! - .NET via P/Invoke
//! - TypeScript/Node.js via napi-rs or FFI
//! - Any language with C FFI support
//!
//! ## Building
//!
//! ```bash
//! cargo build --package photostax-ffi
//! ```
//!
//! This produces both a dynamic library (`cdylib`) and a static library (`staticlib`),
//! along with a C header file (`photostax.h`) via cbindgen.
//!
//! ## Memory Management
//!
//! All pointers returned by FFI functions are owned by the caller and must be
//! freed using the corresponding `*_free` functions:
//!
//! | Allocation Function | Free Function |
//! |---------------------|---------------|
//! | `photostax_repo_open` | `photostax_repo_free` |
//! | `photostax_repo_scan` | `photostax_stack_array_free` |
//! | `photostax_repo_get_stack` | `photostax_stack_free` |
//! | `photostax_read_image` | `photostax_bytes_free` |
//! | Any function returning `*mut c_char` | `photostax_string_free` |
//!
//! ## Error Handling
//!
//! Functions that can fail return an `FfiResult` struct with:
//! - `success`: boolean indicating success/failure
//! - `error_message`: null on success, error string on failure (must be freed)
//!
//! All functions catch panics to prevent unwinding across the FFI boundary.
//!
//! ## Example (C)
//!
//! ```c
//! #include "photostax.h"
//!
//! int main() {
//!     // Open a repository
//!     PhotostaxRepo* repo = photostax_repo_open("/path/to/photos");
//!     if (!repo) {
//!         return 1;
//!     }
//!
//!     // Scan for photo stacks
//!     FfiPhotoStackArray stacks = photostax_repo_scan(repo);
//!     for (size_t i = 0; i < stacks.len; i++) {
//!         printf("Stack: %s\n", stacks.data[i].id);
//!     }
//!
//!     // Clean up
//!     photostax_stack_array_free(stacks);
//!     photostax_repo_free(repo);
//!     return 0;
//! }
//! ```

#![warn(missing_docs)]
#![allow(clippy::missing_safety_doc)]

pub mod types;
pub mod repository;
pub mod search;
pub mod metadata;

// Re-export all public FFI functions and types at crate root
pub use types::*;
pub use repository::*;
pub use search::*;
pub use metadata::*;

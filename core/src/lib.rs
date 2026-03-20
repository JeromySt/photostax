//! # photostax-core
//!
//! Unified photo stack library for Epson FastFoto repositories.
//!
//! [![GitHub](https://img.shields.io/badge/GitHub-JeromySt%2Fphotostax-blue)](https://github.com/JeromySt/photostax)
//!
//! ## Overview
//!
//! Epson FastFoto scanners produce multiple files per scanned photo:
//!
//! | File Pattern | Description |
//! |--------------|-------------|
//! | `<name>.jpg` or `<name>.tif` | Original front scan |
//! | `<name>_a.jpg` or `<name>_a.tif` | Enhanced version (color-corrected) **or** back of photo |
//! | `<name>_b.jpg` or `<name>_b.tif` | Back of the photo (always) |
//!
//! This library groups these files into [`PhotoStack`] objects and provides a
//! [`Repository`] trait for accessing them from various storage backends.
//! **Both JPEG and TIFF formats are fully supported.**
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use photostax_core::backends::local::LocalRepository;
//! use photostax_core::stack_manager::StackManager;
//! use photostax_core::photo_stack::ScannerProfile;
//! use photostax_core::search::SearchQuery;
//!
//! // Create a StackManager with a local repository
//! let repo = LocalRepository::new("/path/to/photos");
//! let mut mgr = StackManager::single(Box::new(repo), ScannerProfile::Auto).unwrap();
//! mgr.scan().unwrap();
//!
//! // Query all stacks → returns a ScanSnapshot
//! let snap = mgr.query(&SearchQuery::new());
//! for stack in snap.stacks() {
//!     println!("Photo: {} ({})", stack.name, stack.id);
//! }
//!
//! // Search with pagination via snapshot
//! let query = SearchQuery::new().with_has_back(true);
//! let snap = mgr.query(&query);
//! let page = snap.get_page(0, 20);
//! println!("{} of {} stacks", page.items.len(), page.total_count);
//!
//! // Next page from the same snapshot
//! if page.has_more {
//!     let page2 = snap.get_page(20, 20);
//! }
//! ```
//!
//! ## Module Organization
//!
//! - [`photo_stack`] — Core [`PhotoStack`] and [`Metadata`] types representing grouped photos
//! - [`classify`] — Image analysis for classifying ambiguous `_a` scans as front or back
//! - [`repository`] — [`Repository`] trait for storage backend abstraction
//! - [`scanner`] — Directory scanning and file grouping logic
//! - [`search`] — Query builder for filtering photo stacks by metadata
//! - [`snapshot`] — Point-in-time snapshot for consistent pagination
//! - [`metadata`] — EXIF, XMP, and sidecar database support
//! - [`backends`] — Storage backend implementations (local filesystem, cloud planned)
//!
//! ## Features
//!
//! - **Multi-format support**: JPEG (`.jpg`, `.jpeg`) and TIFF (`.tif`, `.tiff`)
//! - **Metadata merging**: Combines EXIF, XMP, and custom sidecar database tags
//! - **Search & filter**: Query stacks by metadata with a fluent builder API
//! - **Extensible backends**: Pluggable [`Repository`] trait for different storage systems
//!
//! ## License
//!
//! Licensed under either of [Apache License, Version 2.0](https://github.com/JeromySt/photostax/blob/main/LICENSE-APACHE)
//! or [MIT license](https://github.com/JeromySt/photostax/blob/main/LICENSE-MIT) at your option.
//!
//! [`PhotoStack`]: photo_stack::PhotoStack
//! [`Metadata`]: photo_stack::Metadata
//! [`Repository`]: repository::Repository

#![warn(missing_docs)]

pub mod backends;
pub mod classifier;
pub mod classify;
pub mod events;
pub mod file_access;
pub mod hashing;
pub mod image_handle;
pub mod metadata;
pub mod metadata_handle;
pub mod photo_stack;
pub mod repository;
pub mod scanner;
pub mod search;
pub mod snapshot;
pub mod stack_manager;

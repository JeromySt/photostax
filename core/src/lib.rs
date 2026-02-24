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
//! | `<name>_a.jpg` or `<name>_a.tif` | Enhanced version (color-corrected) |
//! | `<name>_b.jpg` or `<name>_b.tif` | Back of the photo |
//!
//! This library groups these files into [`PhotoStack`] objects and provides a
//! [`Repository`] trait for accessing them from various storage backends.
//! **Both JPEG and TIFF formats are fully supported.**
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use photostax_core::backends::local::LocalRepository;
//! use photostax_core::repository::Repository;
//!
//! // Open a local directory containing FastFoto scans
//! let repo = LocalRepository::new("/path/to/photos");
//! let stacks = repo.scan().unwrap();
//!
//! for stack in &stacks {
//!     println!("Photo: {}", stack.id);
//!     if let Some(ref back) = stack.back {
//!         println!("  Has back scan: {}", back.display());
//!     }
//! }
//! ```
//!
//! ## Module Organization
//!
//! - [`photo_stack`] — Core [`PhotoStack`] and [`Metadata`] types representing grouped photos
//! - [`repository`] — [`Repository`] trait for storage backend abstraction
//! - [`scanner`] — Directory scanning and file grouping logic
//! - [`search`] — Query builder for filtering photo stacks by metadata
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

pub mod photo_stack;
pub mod repository;
pub mod scanner;
pub mod backends;
pub mod metadata;
pub mod search;

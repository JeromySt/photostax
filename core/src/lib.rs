//! # photostax-core
//!
//! Unified photo stack library for Epson FastFoto repositories.
//!
//! Epson FastFoto scanners produce multiple files per scanned photo:
//! - `<name>.jpg` — original scan (front)
//! - `<name>_a.jpg` — enhanced version (front, color-corrected)
//! - `<name>_b.jpg` — back of the photo
//!
//! This library groups these files into [`PhotoStack`] objects and provides
//! a [`Repository`] trait for accessing them from various storage backends.
//!
//! ## License
//!
//! Licensed under either of [Apache License, Version 2.0](../LICENSE-APACHE)
//! or [MIT license](../LICENSE-MIT) at your option.

pub mod photo_stack;
pub mod repository;
pub mod scanner;
pub mod backends;
pub mod metadata;
pub mod search;

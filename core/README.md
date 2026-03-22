# photostax-core

**Unified photo stack library for Epson FastFoto repositories — scanning, metadata, and search.**

[![Crates.io](https://img.shields.io/crates/v/photostax-core.svg)](https://crates.io/crates/photostax-core)
[![Documentation](https://docs.rs/photostax-core/badge.svg)](https://docs.rs/photostax-core)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](https://github.com/JeromySt/photostax#license)

## Overview

Epson FastFoto scanners produce multiple files per scanned photo:

| File Pattern | Description |
|--------------|-------------|
| `<name>.jpg` or `<name>.tif` | Original front scan |
| `<name>_a.jpg` or `<name>_a.tif` | Enhanced version (color-corrected) |
| `<name>_b.jpg` or `<name>_b.tif` | Back of the photo |

This library groups them into a single `PhotoStack` abstraction, enabling applications to operate on complete photos rather than individual files.

## Installation

```sh
cargo add photostax-core
```

## Features

- **PhotoStack-centric API** — all I/O through `stack.original.read()`, `stack.metadata.write()`, etc.
- **Multi-format support** — JPEG (`.jpg`, `.jpeg`) and TIFF (`.tif`, `.tiff`)
- **ImageRef / MetadataRef** — lazy, cached accessors for image data and metadata
- **Repository trait** — pluggable storage backends (local filesystem included)
- **SessionManager** — multi-repo cache with unified query and pagination
- **Search & filter** — query stacks by metadata with a fluent builder API
- **QueryResult** — page-based results with `next_page()`, `prev_page()`, and sub-queries

## Quick Start

```rust
use photostax_core::backends::local::LocalRepository;
use photostax_core::stack_manager::StackManager;
use photostax_core::photo_stack::ScannerProfile;
use photostax_core::search::SearchQuery;

let repo = LocalRepository::new("/path/to/photos");
let mut mgr = StackManager::single(Box::new(repo), ScannerProfile::Auto).unwrap();

// Query all stacks — query() auto-scans on first call
let mut result = mgr.query(None, Some(20), None).unwrap();
for stack in result.current_page() {
    println!("Photo: {} ({})", stack.name(), stack.id());
    if stack.has_original() {
        println!("  Has original image");
    }
}

// Search with pagination
let query = SearchQuery::new().with_has_back(true);
let mut result = mgr.query(Some(&query), Some(20), None).unwrap();
println!("Page 1: {} stacks of {} total", result.current_page().len(), result.total_count());

// Navigate pages
while let Some(page) = result.next_page() {
    println!("Next page: {} stacks", page.len());
}
```

## API Overview

### Core Types

| Type | Description |
|------|-------------|
| `PhotoStack` | Grouped photo with `original`, `enhanced`, `back` (`ImageRef`) and `metadata` (`MetadataRef`) |
| `ImageRef` | Lazy, cached accessor for a single image variant — `read()`, `hash()`, `dimensions()`, `rotate()` |
| `MetadataRef` | Lazy accessor for stack metadata — `read()`, `write()` |
| `Metadata` | EXIF, XMP, and custom tags for a photo stack |
| `Repository` | Trait for storage backend abstraction |
| `LocalRepository` | Local filesystem implementation |
| `StackManager` | Multi-repo cache manager (aliased as `SessionManager`) |
| `SearchQuery` | Builder for filtering stacks by metadata |
| `QueryResult` | Page-based query result with `next_page()`, `prev_page()`, `current_page()`, `query()` sub-queries |

### Key Operations

```rust
// Create a manager — query() auto-scans on first call
let mut mgr = StackManager::single(Box::new(repo), ScannerProfile::Auto)?;

// Query with pagination and progress
let mut result = mgr.query(
    Some(&SearchQuery::new().with_text("vacation").with_has_back(true)),
    Some(20),
    Some(&mut |p| println!("{:?}: {}/{}", p.phase, p.current, p.total)),
)?;

// Per-stack image I/O via accessor methods
let stack = result.current_page().first().unwrap();
let mut reader = stack.original_read()?;         // Read image bytes
let hash = stack.enhanced_hash()?;               // SHA-256 (cached)
stack.back_rotate(Rotation::Cw90)?;              // Rotate in place

// Per-stack metadata
let meta = stack.metadata_read()?;               // Lazy load EXIF/XMP/custom
stack.metadata_write(&updated_meta)?;            // Write back

// Page navigation
println!("{} of {} stacks", result.current_page().len(), result.total_count());
while let Some(page) = result.next_page() {
    println!("Page: {} stacks", page.len());
}

// Sub-query on existing results
let sub = result.query(&SearchQuery::new().with_text("beach"), Some(10));
```

## Building from Source

```sh
git clone https://github.com/JeromySt/photostax
cd photostax
cargo build --release --package photostax-core
cargo test --package photostax-core
```

## License

Licensed under either of [Apache License, Version 2.0](../LICENSE-APACHE) or [MIT License](../LICENSE-MIT) at your option.

---

[← Back to main README](../README.md) | [API Documentation](https://docs.rs/photostax-core)

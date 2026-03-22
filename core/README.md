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
- **ScanSnapshot** — point-in-time snapshots with O(1) staleness detection

## Quick Start

```rust
use photostax_core::backends::local::LocalRepository;
use photostax_core::stack_manager::StackManager;
use photostax_core::photo_stack::ScannerProfile;
use photostax_core::search::SearchQuery;

// Create a manager with a local repository
let repo = LocalRepository::new("/path/to/photos");
let mut mgr = StackManager::single(Box::new(repo), ScannerProfile::Auto).unwrap();
// Query all stacks → returns a ScanSnapshot (auto-scans on first call)
let snap = mgr.query(None, None)?;

for stack in snap.stacks() {
    println!("Photo: {} ({})", stack.name, stack.id);

    // Read original image via ImageRef
    if stack.original.is_present() {
        let mut reader = stack.original.read().unwrap();
        // ... process image bytes ...
    }

    // Read metadata via MetadataRef (lazy-loaded)
    let meta = stack.metadata.read().unwrap();
    if let Some(make) = meta.exif_tags.get("Make") {
        println!("  Camera: {make}");
    }
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
| `ScanSnapshot` | Point-in-time snapshot for consistent pagination |
| `PaginatedResult<T>` | A page of results with total count and navigation metadata |

### Key Operations

```rust
// Create a manager — query() auto-scans on first call
let mut mgr = StackManager::single(Box::new(repo), ScannerProfile::Auto)?;

// Per-stack image I/O via ImageRef
let stack = snap.stacks().first().unwrap();
let mut reader = stack.original.read()?;      // Read image bytes
let hash = stack.enhanced.hash()?;            // SHA-256 (cached)
let (w, h) = stack.back.dimensions()?;        // Image dimensions (cached)
stack.back.rotate(Rotation::Cw90)?;           // Rotate in place

// Per-stack metadata via MetadataRef
let meta = stack.metadata.read()?;            // Lazy load EXIF/XMP/custom
stack.metadata.write(&updated_meta)?;         // Write back

// Search + pagination via ScanSnapshot
let query = SearchQuery::new()
    .with_text("vacation")
    .with_has_back(true);
let snap = mgr.query(&query);
let page = snap.get_page(0, 20);
println!("{} of {} stacks", page.items.len(), page.total_count);

// Next page from the same snapshot
if page.has_more {
    let page2 = snap.get_page(20, 20);
}
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

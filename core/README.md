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

- **Multi-format support** — JPEG (`.jpg`, `.jpeg`) and TIFF (`.tif`, `.tiff`)
- **PhotoStack abstraction** — Groups front, enhanced, and back scans into a single unit
- **Repository trait** — Pluggable storage backends (local filesystem included)
- **Metadata support** — Read EXIF, read/write XMP, and custom sidecar database
- **Search & filter** — Query stacks by metadata with a fluent builder API

## Quick Start

```rust
use photostax_core::backends::local::LocalRepository;
use photostax_core::repository::Repository;

let repo = LocalRepository::new("/path/to/photos");
let stacks = repo.scan().unwrap();

for stack in &stacks {
    println!("Photo: {}", stack.id);
    if let Some(ref back) = stack.back {
        println!("  Has back scan: {}", back.display());
    }
}
```

## API Overview

### Core Types

| Type | Description |
|------|-------------|
| `PhotoStack` | Represents a grouped photo with original, enhanced, and back scans |
| `Metadata` | EXIF, XMP, and custom metadata for a photo stack |
| `Repository` | Trait for storage backend abstraction |
| `LocalRepository` | Local filesystem implementation |
| `SearchQuery` | Builder for filtering stacks by metadata |

### Key Methods

```rust
// Scanning
let stacks = repo.scan()?;              // Discover all photo stacks
let stack = repo.get_stack("IMG_001")?; // Get specific stack by ID

// Metadata
let metadata = repo.read_metadata("IMG_001")?;
repo.write_metadata("IMG_001", &metadata)?;

// Search
let query = SearchQuery::new()
    .with_text("vacation")
    .with_has_back(true);
let results = repo.search(&query)?;

// Read image bytes
let bytes = repo.read_image(&stack.original.unwrap())?;
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

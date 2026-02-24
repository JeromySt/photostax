# photostax-core

**Unified photo stack library for Epson FastFoto repositories — scanning, metadata, and search.**

[![Crates.io](https://img.shields.io/crates/v/photostax-core.svg)](https://crates.io/crates/photostax-core)
[![Documentation](https://docs.rs/photostax-core/badge.svg)](https://docs.rs/photostax-core)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](https://github.com/JeromySt/photostax#license)

## Overview

Epson FastFoto scanners produce multiple files per scanned photo (`<name>.jpg`, `<name>_a.jpg`, `<name>_b.jpg`). This library groups them into a single `PhotoStack` abstraction, enabling applications to operate on complete photos rather than individual files.

## Features

- **PhotoStack abstraction** — Groups front, enhanced, and back scans into a single unit
- **Repository trait** — Pluggable storage backends (local filesystem included)
- **EXIF metadata** — Read and write photo metadata
- **SQLite caching** — Fast indexed lookups for large collections

## Usage

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

## License

Licensed under either of [Apache License, Version 2.0](../LICENSE-APACHE) or [MIT License](../LICENSE-MIT) at your option.

See the [main repository](https://github.com/JeromySt/photostax) for more details.

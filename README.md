# photostax

**Unified photo stack library for Epson FastFoto repositories.**

[![CI](https://github.com/JeromySt/photostax/actions/workflows/ci.yml/badge.svg)](https://github.com/JeromySt/photostax/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](#license)

Epson FastFoto scanners produce multiple files per scanned photo:

| File | Description |
|------|-------------|
| `<name>.jpg` | Original front scan |
| `<name>_a.jpg` | Enhanced version (color-corrected) |
| `<name>_b.jpg` | Back of the photo |

**photostax** groups these into a single `PhotoStack` abstraction, enabling applications to operate on complete photos rather than individual files. This is essential for workflows like OCR (reading the back), auto-tagging, metadata management, and browsing.

## Architecture

- **Rust core** (`photostax-core`) — single source of truth with high-performance scanning, metadata, and storage backend support
- **CLI tool** (`photostax-cli`) — command-line interface for scanning, searching, and managing photo stacks
- **Client bindings** — idiomatic libraries for C# (.NET), TypeScript (Node.js), and more
- **Storage backends** — pluggable `Repository` trait (local filesystem now; OneDrive, Google Drive planned)

## Quick Start

### Library Usage

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

### CLI Usage

```sh
# Scan a directory and list all photo stacks
photostax-cli scan /photos

# Search for stacks containing "birthday"
photostax-cli search /photos birthday

# Show detailed info about a specific stack
photostax-cli info /photos IMG_0001

# Export all stacks as JSON
photostax-cli export /photos --output stacks.json

# Manage metadata
photostax-cli metadata write /photos IMG_0001 --tag album="Family Photos"
photostax-cli metadata read /photos IMG_0001 --format json
```

See [cli/README.md](cli/README.md) for complete CLI documentation.

## Building

```sh
cargo build --workspace
cargo test --workspace
```

## Project Structure

```
photostax/
├── core/               # Rust core library (photostax-core)
├── cli/                # CLI tool for inspection/scripting
├── bindings/
│   ├── dotnet/         # C# client (P/Invoke)
│   └── typescript/     # TypeScript client (napi-rs)
├── LICENSE-MIT
├── LICENSE-APACHE
└── README.md
```

## License

Licensed under either of

- [Apache License, Version 2.0](LICENSE-APACHE)
- [MIT License](LICENSE-MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.

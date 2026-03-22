# photostax

**Unified photo stack library for Epson FastFoto repositories.**

[![CI](https://github.com/JeromySt/photostax/actions/workflows/ci.yml/badge.svg)](https://github.com/JeromySt/photostax/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/JeromySt/photostax/branch/main/graph/badge.svg)](https://codecov.io/gh/JeromySt/photostax)
[![Crates.io](https://img.shields.io/crates/v/photostax-core.svg)](https://crates.io/crates/photostax-core)
[![NuGet](https://img.shields.io/nuget/v/Photostax.svg)](https://www.nuget.org/packages/Photostax)
[![npm](https://img.shields.io/npm/v/@photostax/core.svg)](https://www.npmjs.com/package/@photostax/core)
[![docs.rs](https://docs.rs/photostax-core/badge.svg)](https://docs.rs/photostax-core)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](#license)

## What is this?

Epson FastFoto scanners produce multiple files per scanned photo:

| File Pattern | Description |
|--------------|-------------|
| `<Prefix>_NNNN.jpg/.tif` | Original front scan |
| `<Prefix>_NNNN_a.jpg/.tif` | Enhanced (color-corrected) version |
| `<Prefix>_NNNN_b.jpg/.tif` | Back of photo scan |

**photostax** groups these into a single `PhotoStack`, managing metadata that is embedded directly in the image files (EXIF/XMP) so any photo application can access it.

## Supported Formats

- **JPEG** (`.jpg`, `.jpeg`)
- **TIFF** (`.tif`, `.tiff`)

## Available Libraries

| Language | Package | Install | Docs |
|----------|---------|---------|------|
| Rust | `photostax-core` | `cargo add photostax-core` | [core/README.md](core/README.md) |
| .NET/C# | `Photostax` | `dotnet add package Photostax` | [bindings/dotnet/README.md](bindings/dotnet/README.md) |
| TypeScript/Node.js | `@photostax/core` | `npm install @photostax/core` | [bindings/typescript/README.md](bindings/typescript/README.md) |
| CLI | `photostax-cli` | `cargo install photostax-cli` | [cli/README.md](cli/README.md) |

## Quick Start (Rust)

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

## Quick Start (.NET)

```csharp
using Photostax;

using var repo = new PhotostaxRepository("/path/to/photos");

// Query all stacks — Query() auto-scans on first call
var result = repo.Query(pageSize: 20);
foreach (var stack in result.CurrentPage)
{
    Console.WriteLine($"Photo: {stack.Name} ({stack.Id})");
    if (stack.HasOriginal)
        Console.WriteLine($"  Has original image");
}

// Search with page navigation
var filtered = repo.Query(new SearchQuery().WithHasBack(true), pageSize: 20);
Console.WriteLine($"Page 1: {filtered.CurrentPage.Count} of {filtered.TotalCount} total");

// Navigate pages
while (filtered.NextPage() is { } page)
{
    Console.WriteLine($"Next page: {page.Count} stacks");
}
```

## Quick Start (TypeScript)

```typescript
import { PhotostaxRepository } from '@photostax/core';

const repo = new PhotostaxRepository('/path/to/photos');

// Query all stacks — query() auto-scans on first call
const result = repo.query(undefined, 20);
for (const stack of result.currentPage()) {
  console.log(`Photo: ${stack.name} (${stack.id})`);
  if (stack.hasOriginal) {
    console.log(`  Has original image`);
  }
}

// Search with page navigation
const filtered = repo.query({ text: 'birthday', hasBack: true }, 20);
console.log(`Page 1: ${filtered.currentPage().length} of ${filtered.totalCount} total`);

// Navigate pages
let page;
while ((page = filtered.nextPage()) !== null) {
  console.log(`Next page: ${page.length} stacks`);
}
```

## Quick Start (CLI)

```sh
# Scan a directory and list all photo stacks
photostax-cli scan /photos

# Search for stacks containing "birthday"
photostax-cli search /photos birthday

# Paginate results (20 items starting at offset 40)
photostax-cli scan /photos --limit 20 --offset 40
photostax-cli search /photos birthday --limit 10 --offset 0

# Show detailed info about a specific stack
photostax-cli info /photos IMG_0001

# Export all stacks as JSON
photostax-cli export /photos --output stacks.json

# Manage metadata
photostax-cli metadata write /photos IMG_0001 --tag album="Family Photos"
photostax-cli metadata read /photos IMG_0001 --format json
```

See [cli/README.md](cli/README.md) for complete CLI documentation.

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        Applications                              │
├──────────────┬─────────────────┬─────────────────┬──────────────┤
│    .NET      │   TypeScript    │      CLI        │   (Future)   │
│    (C#)      │   (Node.js)     │  (photostax-cli)│              │
├──────────────┴─────────────────┴─────────────────┴──────────────┤
│                         photostax-ffi                            │
│                     (C-compatible FFI layer)                     │
├─────────────────────────────────────────────────────────────────┤
│                        photostax-core                            │
│  ┌─────────────┐  ┌────────────┐  ┌──────────────────────────┐ │
│  │StackManager │  │ FileAccess │  │ Content Hashing          │ │
│  │(multi-repo  │  │ (I/O trait)│  │ (SHA-256, Merkle, lazy)  │ │
│  │ cache,      │  │            │  │                          │ │
│  │ query())    │  │            │  │                          │ │
│  └─────────────┘  └────────────┘  └──────────────────────────┘ │
│  ┌─────────────┐  ┌────────────┐  ┌──────────────────────────┐ │
│  │ FS Watching │  │ Opaque IDs │  │ Scanner & Metadata       │ │
│  │ (notify rx) │  │ (hash-based│  │ (EXIF, XMP, sidecars)    │ │
│  │             │  │  globally  │  │                          │ │
│  │             │  │  unique)   │  │                          │ │
│  └─────────────┘  └────────────┘  └──────────────────────────┘ │
├─────────────────────────────────────────────────────────────────┤
│                       Storage Backends                           │
│        ┌──────────────┬──────────────┬──────────────┐           │
│        │ Local FS     │ OneDrive     │ Google Drive │           │
│        │ (implemented)│ (planned)    │ (planned)    │           │
│        └──────────────┴──────────────┴──────────────┘           │
└─────────────────────────────────────────────────────────────────┘
```

- **StackManager** — primary API; unified multi-repository cache with `query()` as the sole entry point, overlap detection, and filesystem watch integration
- **FileAccess trait** — backend-polymorphic file I/O with `open_read()`/`open_write()` and `HashingReader` for zero-overhead content hashing
- **Opaque stack IDs** — deterministic SHA-256 hashes (16 hex chars) that are globally unique across subfolders; `stack.name` and `stack.folder` for display
- **Content hashing** — lazy per-file SHA-256 via `ImageFile::content_hash()` and Merkle-style `PhotoStack::content_hash()` for duplicate detection
- **Filesystem watching** — `Repository::watch()` returns a `Receiver<StackEvent>` for reactive cache updates via `CacheEvent` (`StackAdded`/`StackUpdated`/`StackRemoved`)
- **Rust core** (`photostax-core`) — scanning, metadata, storage backend support
- **FFI layer** (`photostax-ffi`) — C-compatible interface for language bindings
- **Client bindings** — idiomatic libraries for C# (.NET), TypeScript (Node.js), and more (constructors wrap `StackManager` automatically)
- **Storage backends** — pluggable `Repository: FileAccess` trait (local filesystem now; OneDrive, Google Drive planned)

**Metadata strategy**: EXIF tags are read from images, XMP tags are read/written for interoperability with other photo apps, and XMP sidecar files (`.xmp`) store custom tags and EXIF overrides alongside your images. See [docs/metadata-strategy.md](docs/metadata-strategy.md) for details.

## Building from Source

### Prerequisites

- [Rust toolchain](https://rustup.rs/) (1.70+)
- For .NET binding: [.NET SDK 8.0+](https://dotnet.microsoft.com/download)
- For TypeScript binding: [Node.js 18+](https://nodejs.org/) with npm

### Rust Workspace

```sh
# Build all crates
cargo build --workspace

# Run all tests
cargo test --workspace

# Build release binaries
cargo build --release --workspace
```

### .NET Binding

```sh
# Build native library first
cargo build --release -p photostax-ffi

# Build .NET library
cd bindings/dotnet
dotnet build

# Run tests
dotnet test
```

### TypeScript Binding

```sh
# Install dependencies and build
cd bindings/typescript
npm install
npm run build

# Run tests
npm test
```

## Testing

### Running All Tests

```sh
# Rust tests
cargo test --workspace

# .NET tests
cd bindings/dotnet && dotnet test

# TypeScript tests
cd bindings/typescript && npm test
```

### Coverage

```sh
# Install cargo-llvm-cov
cargo install cargo-llvm-cov

# Generate coverage report
cargo llvm-cov --workspace --html
```

## Project Structure

```
photostax/
├── core/                   # Rust core library (photostax-core)
│   ├── src/                # Library source code
│   └── tests/              # Integration tests
├── ffi/                    # C FFI bindings (photostax-ffi)
│   ├── src/                # FFI implementation
│   └── photostax.h         # C header file
├── cli/                    # CLI tool (photostax-cli)
│   ├── src/                # CLI implementation
│   └── tests/              # CLI tests
├── bindings/
│   ├── dotnet/             # C# client (P/Invoke)
│   │   ├── src/            # .NET source
│   │   └── tests/          # .NET tests
│   └── typescript/         # TypeScript client (napi-rs)
│       ├── src/            # TypeScript source
│       └── __tests__/      # TypeScript tests
├── docs/                   # Documentation
│   ├── architecture.md     # System architecture
│   ├── fastfoto-convention.md  # File naming patterns
│   ├── metadata-strategy.md    # EXIF/XMP/sidecar handling
│   └── bindings-guide.md   # How to create new bindings
├── LICENSE-MIT
├── LICENSE-APACHE
├── CHANGELOG.md
└── README.md
```

## Documentation

- [Migration Guide (v0.4.x → v0.5.0)](docs/MIGRATION.md) — Breaking changes and upgrade instructions
- [Architecture Overview](docs/architecture.md) — System design and component responsibilities
- [FastFoto Naming Convention](docs/fastfoto-convention.md) — How scanner files are named and grouped
- [Metadata Strategy](docs/metadata-strategy.md) — EXIF, XMP, and sidecar database handling
- [Bindings Guide](docs/bindings-guide.md) — How to create new language bindings

## Contributing

Contributions are welcome! Please:

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## License

Licensed under either of

- [Apache License, Version 2.0](LICENSE-APACHE)
- [MIT License](LICENSE-MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.

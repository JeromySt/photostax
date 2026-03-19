# @photostax/core

**Node.js binding for the photostax library — access Epson FastFoto repositories from TypeScript/JavaScript.**

[![npm](https://img.shields.io/npm/v/@photostax/core.svg)](https://www.npmjs.com/package/@photostax/core)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](../../LICENSE-MIT)

## Overview

Epson FastFoto scanners produce multiple files per scanned photo:

| File Pattern | Description |
|--------------|-------------|
| `<name>.jpg` or `<name>.tif` | Original front scan |
| `<name>_a.jpg` or `<name>_a.tif` | Enhanced version (color-corrected) |
| `<name>_b.jpg` or `<name>_b.tif` | Back of the photo |

This library groups these files into `PhotoStack` objects and provides access to their metadata from EXIF, XMP, and a sidecar database.

## Installation

```bash
npm install @photostax/core
```

## Quick Start

```typescript
import { PhotostaxRepository } from '@photostax/core';

// Open a directory containing FastFoto scans
const repo = new PhotostaxRepository('/path/to/photos');

// Scan for all photo stacks
const stacks = repo.scan();

for (const stack of stacks) {
  // Use stack.name for display; stack.id is an opaque 16-char hex hash
  console.log(`Photo: ${stack.name} (id=${stack.id})`);
  console.log(`  Folder: ${stack.folder ?? '(root)'}`);
  console.log(`  Original: ${stack.original}`);
  console.log(`  Enhanced: ${stack.enhanced}`);
  console.log(`  Back: ${stack.back}`);
  console.log(`  EXIF Make: ${stack.metadata.exifTags['Make']}`);
}

// Use query() for search + pagination in one call (preferred)
const page1 = repo.query(
  { text: "birthday", hasBack: true },
  0,   // offset
  20   // limit
);
console.log(`Showing ${page1.items.length} of ${page1.totalCount} total`);

if (page1.hasMore) {
  const page2 = repo.query({ text: "birthday", hasBack: true }, 20, 20);
}

// query() with no arguments returns all stacks, unpaginated
const all = repo.query();
```

## API Overview

### PhotostaxRepository

The main class for accessing photo stacks.

| Method | Description |
|--------|-------------|
| `scan()` | Discover all photo stacks in the repository |
| `getStack(id)` | Get a specific stack by its opaque hash ID |
| `readImage(path)` | Read raw image bytes as Buffer |
| `writeMetadata(id, metadata)` | Write metadata to a stack |
| `query(filter?, offset?, limit?)` | **Preferred.** Search and paginate in one call (see below) |
| `search(query)` | Find stacks matching a query *(convenience wrapper around `query()`)* |
| `scanPaginated(offset, limit)` | Scan with pagination *(convenience wrapper around `query()`)* |
| `searchPaginated(query, offset, limit)` | Search with pagination *(convenience wrapper around `query()`)* |

### PhotoStack

```typescript
interface PhotoStack {
  id: string;                  // Opaque 16-char hex hash (SHA-256); use for lookups
  name: string;                // Human-readable display name (e.g. "IMG_0042")
  folder: string | null;       // Subfolder within the repository, or null for root
  original: string | null;     // Path to original scan
  enhanced: string | null;     // Path to enhanced scan
  back: string | null;         // Path to back scan
  metadata: Metadata;
}
```

> **v0.2.x note:** Stack IDs are now opaque hashes, not human-readable stems.
> Use `stack.name` for display purposes and `stack.id` for programmatic lookups
> (e.g. `repo.getStack(stack.id)`).

### Metadata

```typescript
interface Metadata {
  exifTags: Record<string, string>;    // EXIF tags from image
  xmpTags: Record<string, string>;     // XMP metadata
  customTags: Record<string, unknown>; // Custom JSON metadata
}
```

### SearchQuery

```typescript
interface SearchQuery {
  text?: string;                       // Free-text search
  exifFilters?: KeyValueFilter[];      // EXIF tag filters
  customFilters?: KeyValueFilter[];    // Custom tag filters
  hasBack?: boolean;                   // Filter by back scan presence
  hasEnhanced?: boolean;               // Filter by enhanced scan presence
}
```

### PaginatedResult

```typescript
interface PaginatedResult {
  items: PhotoStack[];          // Items in this page
  totalCount: number;           // Total items across all pages
  offset: number;               // Offset used for this page
  limit: number;                // Limit used for this page
  hasMore: boolean;             // Whether more items exist beyond this page
}
```

### query()

The preferred way to search and paginate in a single call. All parameters are optional:

```typescript
// All stacks (no filter, no pagination)
const all = repo.query();

// Filter only
const filtered = repo.query({ text: "vacation" });

// Filter + pagination
const page = repo.query(
  { text: "birthday", hasBack: true },
  0,   // offset
  20   // limit
);

// Pagination without filter
const page = repo.query(undefined, 0, 50);
```

`query()` returns a `PaginatedResult`. When called without `offset`/`limit`, all
matching stacks are returned in a single result with `hasMore: false`.

> `search()`, `scanPaginated()`, and `searchPaginated()` remain available for
> backward compatibility but are now convenience wrappers around `query()`.

### Utility Functions

| Function | Description |
|----------|-------------|
| `isNativeAvailable()` | Check if native addon loaded successfully |
| `getNativeLoadError()` | Get error if native addon failed to load |

## Building from Source

### Prerequisites

- [Rust toolchain](https://rustup.rs/) (1.70+)
- [Node.js 18+](https://nodejs.org/) with npm
- Python 3.x (for node-gyp)
- C++ compiler (Visual Studio Build Tools on Windows, GCC/Clang on Unix)

### Build Steps

```bash
# Clone the repository
git clone https://github.com/JeromySt/photostax.git
cd photostax/bindings/typescript

# Install dependencies
npm install

# Build the native addon
npm run build

# Run tests
npm test
```

### Development Build

For faster iteration during development:

```bash
npm run build:debug
```

## Supported Platforms

Pre-built binaries are published for:

| Platform | Architectures |
|----------|---------------|
| Windows | x64, arm64 |
| macOS | x64, arm64 |
| Linux | x64, arm64, musl |

## File Format Support

| Format | Extensions | EXIF | XMP |
|--------|------------|------|-----|
| JPEG | `.jpg`, `.jpeg` | ✓ | Embedded |
| TIFF | `.tif`, `.tiff` | ✓ | Embedded or sidecar |

## License

Licensed under either of [Apache License, Version 2.0](../../LICENSE-APACHE) or [MIT license](../../LICENSE-MIT) at your option.

---

[← Back to main README](../../README.md) | [FFI Documentation](../../ffi/README.md)

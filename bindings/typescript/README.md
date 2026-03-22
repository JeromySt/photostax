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

This library groups these files into `PhotoStack` objects with lazy, cached accessors for image data and metadata.

## Architecture

`PhotostaxRepository` is a native Node.js addon (via napi-rs) that wraps the Rust `SessionManager` (formerly `StackManager`) — the unified cache and query engine powering photostax. Each instance creates a `SessionManager` internally with a single repository, giving you:

- **O(1) stack lookups** by opaque ID
- **PhotoStack-centric I/O** — `stack.original.read()`, `stack.metadata.read()`, etc.
- **Lazy, cached accessors** — image data and metadata loaded on demand
- **ScanSnapshot queries** — `query()` returns a snapshot for consistent pagination

> **Multi-repo support:** Use the `StackManager` class to manage multiple repositories through a single cache.

### Multi-repo with StackManager

```typescript
import { StackManager } from '@photostax/core';

const mgr = new StackManager();
mgr.addRepo('/photos/2024', { recursive: true });
mgr.addRepo('/photos/2023', { recursive: true });
console.log(`Managing ${mgr.repoCount} repos`);

// Query across all repos — returns a ScanSnapshot
const snap = mgr.query({ text: 'birthday' });
const page = snap.getPage(0, 20);
for (const stack of page.items) {
  console.log(`${stack.name} (${stack.id})`);

  // Per-stack image I/O (no manager methods needed)
  if (stack.original.isPresent) {
    const buf = stack.original.read();
    // ... process image ...
  }
}
```

### Custom Repository Providers

Register a custom backend (e.g., cloud storage) by implementing the `RepositoryProvider` interface:

```typescript
import { StackManager, type RepositoryProvider, type FileEntry } from '@photostax/core';

const oneDriveProvider: RepositoryProvider = {
  location: 'onedrive://user/photos',
  listEntries: (prefix: string, recursive: boolean): FileEntry[] => {
    return [
      { name: 'IMG_001.jpg', folder: 'vacation', path: 'onedrive://user/photos/vacation/IMG_001.jpg', size: 2048576 },
      { name: 'IMG_001_a.jpg', folder: 'vacation', path: 'onedrive://user/photos/vacation/IMG_001_a.jpg', size: 2148576 },
    ];
  },
  readFile: (path: string): Buffer => {
    return downloadFromOneDrive(path);
  },
  writeFile: (path: string, data: Buffer): void => {
    uploadToOneDrive(path, data);
  },
};

const mgr = new StackManager();
mgr.addForeignRepo(oneDriveProvider, { recursive: true, profile: 'auto' });
// query() auto-scans on first call
```

The host providesI/O primitives while Rust handles all scanning, file grouping, naming convention parsing, and metadata operations.

## Installation

```bash
npm install @photostax/core
```

## Quick Start

```typescript
import { PhotostaxRepository } from '@photostax/core';

// Open a directory containing FastFoto scans
const repo = new PhotostaxRepository('/path/to/photos');

// Query all stacks — query() auto-scans on first call
const snap = repo.query();

for (const stack of snap.stacks) {
  console.log(`Photo: ${stack.name} (id=${stack.id})`);
  console.log(`  Folder: ${stack.folder ?? '(root)'}`);

  // Read images via ImageRef (lazy, cached)
  if (stack.original.isPresent) {
    const buf = stack.original.read();
    console.log(`  Original: ${stack.original.size} bytes`);
    console.log(`  Hash: ${stack.original.hash()}`);
  }

  // Read metadata via MetadataRef (lazy-loaded)
  const meta = stack.metadata.read();
  console.log(`  Camera: ${meta.exifTags['Make'] ?? 'unknown'}`);
}

// Search with pagination via ScanSnapshot
const snap2 = repo.query({ text: 'birthday', hasBack: true });
const page = snap2.getPage(0, 20);
console.log(`Showing ${page.items.length} of ${page.totalCount} total`);

if (page.hasMore) {
  const page2 = snap2.getPage(20, 20);
}

// Write metadata directly on the stack
snap.stacks[0].metadata.write({ customTags: { album: 'Family Photos' } });
```

## API Overview

### PhotostaxRepository

The main class for accessing photo stacks.

| Method | Description |
|--------|-------------|
| `query(filter?)` | **(v0.4.0)** Returns a `ScanSnapshot` for search + pagination. Auto-scans on first call. |
| `getStack(id)` | Get a specific stack by its opaque hash ID |

### StackManager

Multi-repository manager for unified access across directories and custom backends.

| Method | Description |
|--------|-------------|
| `new StackManager()` | Create an empty manager |
| `addRepo(path, options?)` | Register a local directory |
| `addForeignRepo(provider, options?)` | Register a custom repository provider |
| `repoCount` | Number of registered repositories |
| `getStack(id)` | Retrieve a single stack by opaque ID |
| `query(filter?)` | Search across all repos, returns `ScanSnapshot` |
| `createSnapshot()` | Create a point-in-time snapshot |

### RepositoryProvider

Interface for custom repository backends:

```typescript
interface RepositoryProvider {
  readonly location: string;
  listEntries(prefix: string, recursive: boolean): FileEntry[];
  readFile(path: string): Buffer;
  writeFile(path: string, data: Buffer): void;
}

interface FileEntry {
  name: string;    // File name with extension
  folder: string;  // Relative folder path (empty for root)
  path: string;    // Full path or URI
  size: number;    // File size in bytes
}
```

### PhotoStack (v0.4.0)

All I/O operations are accessed directly on the stack object:

```typescript
interface PhotoStack {
  id: string;                  // Opaque 16-char hex hash; use for lookups
  name: string;                // Human-readable display name
  folder: string | null;       // Subfolder within the repository
  original: ImageRef;          // Original front scan accessor
  enhanced: ImageRef;          // Enhanced scan accessor
  back: ImageRef;              // Back scan accessor
  metadata: MetadataRef;       // Metadata accessor
}
```

### ImageRef

Lazy, cached accessor for a single image variant:

```typescript
interface ImageRef {
  isPresent: boolean;          // Whether this image variant exists
  read(): Buffer;              // Read image bytes
  hash(): string;              // SHA-256 content hash (cached)
  dimensions(): { width: number; height: number }; // Image dimensions (cached)
  size: number;                // File size in bytes
  rotate(degrees: number): void; // Rotate image in place
}
```

### MetadataRef

Lazy accessor for stack metadata:

```typescript
interface MetadataRef {
  read(): Metadata;            // Load EXIF, XMP, and custom tags (lazy-loaded)
  write(metadata: Metadata): void; // Write metadata back
}
```

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
  repoId?: string;                     // Filter by repository ID (v0.4.0)
}
```

### ScanSnapshot

Point-in-time snapshot for consistent pagination:

```typescript
interface ScanSnapshot {
  stacks: PhotoStack[];        // All stacks in the snapshot
  totalCount: number;          // Total number of stacks
  getPage(offset: number, limit: number): PaginatedResult;
  isStale(): boolean;          // O(1) staleness check (v0.4.0)
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

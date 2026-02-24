# @photostax/core

Node.js binding for the photostax library - a unified photo stack library for Epson FastFoto repositories.

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

### Prerequisites

Building from source requires:
- [Rust toolchain](https://rustup.rs/) (rustc 1.70+)
- Node.js 18+ with npm
- Python 3.x (for node-gyp)
- C++ compiler (Visual Studio Build Tools on Windows, GCC/Clang on Unix)

## Quick Start

```typescript
import { PhotostaxRepository } from '@photostax/core';

// Open a directory containing FastFoto scans
const repo = new PhotostaxRepository('/path/to/photos');

// Scan for all photo stacks
const stacks = repo.scan();

for (const stack of stacks) {
  console.log(`Photo: ${stack.id}`);
  console.log(`  Original: ${stack.original}`);
  console.log(`  Enhanced: ${stack.enhanced}`);
  console.log(`  Back: ${stack.back}`);
  console.log(`  EXIF Make: ${stack.metadata.exifTags['Make']}`);
}
```

## API Reference

### PhotostaxRepository

The main class for accessing photo stacks.

#### Constructor

```typescript
new PhotostaxRepository(directoryPath: string)
```

Creates a repository rooted at the given directory.

#### Methods

##### `scan(): PhotoStack[]`

Scans the directory and returns all discovered photo stacks with metadata.

```typescript
const stacks = repo.scan();
```

##### `getStack(id: string): PhotoStack`

Retrieves a single stack by its ID (base filename without suffix).

```typescript
const stack = repo.getStack('IMG_001');
```

##### `readImage(path: string): Buffer`

Reads the raw bytes of an image file.

```typescript
const buffer = repo.readImage(stack.original!);
```

##### `writeMetadata(stackId: string, metadata: Partial<Metadata>): void`

Writes metadata to a stack. XMP tags are written to image files, custom tags go to the sidecar database.

```typescript
repo.writeMetadata('IMG_001', {
  customTags: { album: 'Family Reunion', people: ['John', 'Jane'] }
});
```

##### `search(query: SearchQuery): PhotoStack[]`

Searches for stacks matching the given criteria.

```typescript
const results = repo.search({
  text: 'birthday',
  hasBack: true,
  exifFilters: [{ key: 'Make', value: 'EPSON' }]
});
```

### Types

#### PhotoStack

```typescript
interface PhotoStack {
  id: string;                  // Base filename identifier
  original: string | null;     // Path to original scan
  enhanced: string | null;     // Path to enhanced scan
  back: string | null;         // Path to back scan
  metadata: Metadata;
}
```

#### Metadata

```typescript
interface Metadata {
  exifTags: Record<string, string>;    // EXIF tags from image
  xmpTags: Record<string, string>;     // XMP metadata
  customTags: Record<string, unknown>; // Custom JSON metadata
}
```

#### SearchQuery

```typescript
interface SearchQuery {
  text?: string;                       // Free-text search
  exifFilters?: KeyValueFilter[];      // EXIF tag filters
  customFilters?: KeyValueFilter[];    // Custom tag filters
  hasBack?: boolean;                   // Filter by back scan presence
  hasEnhanced?: boolean;               // Filter by enhanced scan presence
}

interface KeyValueFilter {
  key: string;
  value: string;
}
```

### Utility Functions

#### `isNativeAvailable(): boolean`

Returns `true` if the native addon was loaded successfully.

#### `getNativeLoadError(): Error | null`

Returns the error if the native addon failed to load, or `null` on success.

## Building from Source

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

The native addon builds for:
- Windows (x64, arm64)
- macOS (x64, arm64)
- Linux (x64, arm64, musl)

Pre-built binaries are published to npm for common platforms.

## File Format Support

- **JPEG**: `.jpg`, `.jpeg`
- **TIFF**: `.tif`, `.tiff`

Both formats support EXIF metadata extraction. XMP metadata is read from embedded data (JPEG) or sidecar `.xmp` files (TIFF).

## License

Licensed under either of [Apache License, Version 2.0](../../LICENSE-APACHE) or [MIT license](../../LICENSE-MIT) at your option.

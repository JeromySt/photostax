# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.7] - 2026-03-14

### Added

- Stack-level image rotation across all layers
  - `Rotation` enum (`Cw90`, `Ccw90`, `Cw180`) with `from_degrees()` / `as_degrees()` helpers
  - `Repository::rotate_stack()` trait method — rotates all images (original, enhanced, back) at the pixel level
  - `LocalRepository` implementation using the `image` crate for JPEG and TIFF decode/rotate/re-encode
  - `photostax_rotate_stack` FFI function accepting stack ID and degree value (90, -90, 180, -180)
  - `photostax-cli rotate <dir> <stack_id> --degrees <angle>` CLI command
  - `rotateStack(stackId, degrees)` method in TypeScript binding
  - `RotateStack(stackId, degrees)` method in .NET binding

## [0.1.5] - 2026-03-08

### Added

- Lazy-loading architecture for efficient scanning across all layers
  - `Repository::scan()` now returns lightweight stacks with only file paths and folder-derived metadata — no file content I/O
  - `Repository::load_metadata()` loads EXIF, XMP, and sidecar data on demand for individual stacks
  - `LocalRepository::scan_with_metadata()` convenience method for eager loading (old behavior)
  - `Metadata::is_empty()` and `PhotoStack::image_count()` / `PhotoStack::is_metadata_loaded()` helpers
  - `--metadata` / `-m` flag on CLI `scan` command to opt into metadata loading
  - `photostax_stack_load_metadata` FFI function for per-stack metadata loading
  - `photostax_repo_scan_paginated` accepts `load_metadata` parameter
  - `scanWithMetadata()` and `loadMetadata()` in TypeScript binding
  - `ScanWithMetadata()` and `LoadMetadata()` in .NET binding
  - `scanPaginated()` accepts optional `loadMetadata` parameter in TypeScript
  - `ScanPaginated()` accepts optional `loadMetadata` parameter in .NET

## [0.1.4] - 2026-03-08

### Added

- Pagination support across all layers for efficient web rendering
  - `PaginationParams` and `PaginatedResult<T>` types in `photostax-core`
  - `paginate_stacks()` function to apply offset/limit to any collection of photo stacks
  - `photostax_repo_scan_paginated` and `photostax_search_paginated` FFI functions with `FfiPaginatedResult`
  - `--limit` and `--offset` flags on `scan` and `search` CLI commands
  - `scanPaginated()` and `searchPaginated()` methods in TypeScript binding
  - `ScanPaginated()` and `SearchPaginated()` methods in .NET binding

## [0.1.3] - 2026-03-07

### Fixed

- Release workflow: added delays between crate publishes for index propagation
- Release workflow: removed `continue-on-error` from publish steps to surface real failures

## [0.1.2] - 2026-03-07

### Fixed

- Regenerated package-lock.json to sync with version bump
- Removed environment branch_policy restrictions blocking tag-based deployments

## [0.1.1] - 2026-03-07

### Fixed

- Release workflow: package platform archives to avoid asset name collisions
- Release workflow: npm version command `--allow-same-version` flag
- Release workflow: switch npm publish to OIDC Trusted Publishing
- CI: pin `dtolnay/rust-toolchain` and `taiki-e/install-action` to commit SHAs
- CI: correct YAML indentation for toolchain `with:` blocks
- CI: exclude `bindings/typescript` from Cargo workspace
- CI: bump `actions/cache` v5, `actions/upload-artifact` v7, `actions/download-artifact` v8

## [0.1.0] - 2026-02-24

### Added

- Initial release of `photostax-core` library
  - `PhotoStack` abstraction for grouping front, enhanced, and back scans
  - `Repository` trait with `LocalRepository` implementation
  - EXIF metadata reading support
  - XMP sidecar files for custom metadata and EXIF overrides

- Initial release of `photostax-cli` tool
  - Directory scanning for photo stacks
  - Basic inspection commands

- Initial release of `photostax-ffi` C bindings
  - Repository creation and scanning
  - Version information

[0.1.7]: https://github.com/JeromySt/photostax/compare/v0.1.5...v0.1.7
[0.1.5]: https://github.com/JeromySt/photostax/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/JeromySt/photostax/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/JeromySt/photostax/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/JeromySt/photostax/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/JeromySt/photostax/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/JeromySt/photostax/releases/tag/v0.1.0

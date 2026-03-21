# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.3] - 2026-03-21

### Added
- **QueryResult pagination type** — `query()` now accepts an optional `page_size` parameter and returns a `QueryResult` with cursor-based pagination: `current_page()`, `get_page(n)`, `next_page()`, `prev_page()`, `next_stack()` (auto-paging iterator)
- **Variant-based image reading (FFI)** — new `photostax_read_image_variant(repo, stack_id, variant)` function for reading images by variant index (0=original, 1=enhanced, 2=back)

### Fixed
- **.NET image reading** — `ReadOriginalImage()`/`ReadEnhancedImage()`/`ReadBackImage()` now use variant-based FFI instead of broken path-based reading that returned "Stack not found: present"
- **.NET PhotoStack properties** — replaced `OriginalPath`/`EnhancedPath`/`BackPath` string properties with `HasOriginal`/`HasEnhanced`/`HasBack` boolean properties

### Changed
- `StackManager::query()` signature now takes `page_size: Option<usize>` instead of `pagination: Option<&PaginationParams>` — pass `None` for all results in one page

## [0.4.2] - 2026-03-21

### Fixed
- Scanner profile parameter now correctly applied in snapshot rescan (`ffi/src/snapshot.rs`)

## [0.4.1] - 2026-03-21

### Fixed
- Scanner profile parameter now correctly applied in repository scan (`ffi/src/repository.rs`, `bindings/typescript/src/lib.rs`)

## [0.4.0] - 2026-03-20

### ⚠️ BREAKING CHANGES

This is a major architecture redesign. See [MIGRATION.md](docs/MIGRATION.md) for upgrade guidance.

#### PhotoStack-Centric API
- **PhotoStack** is now the primary user-facing object for all I/O operations
- `stack.original.read()`, `stack.enhanced.hash()`, `stack.back.rotate()` — per-image operations via `ImageRef`
- `stack.metadata.read()`, `stack.metadata.write()` — metadata operations via `MetadataRef`
- `stack.original.is_present()` replaces `stack.original.is_some()`

#### Repository Trait Simplified
- Removed: `load_metadata()`, `get_stack()`, `read_image()`, `write_metadata()`, `rotate_stack()`
- Added: `generation()`, `set_classifier()`, `subscribe()`
- Repository is now a pure structural provider — creates PhotoStacks with embedded handles

#### SessionManager (formerly StackManager)
- `SessionManager` type alias added (StackManager still works)
- Removed per-stack routing methods (load_metadata, write_metadata, rotate_stack, read_image)
- Added `query(&SearchQuery) -> ScanSnapshot` for unified search + pagination
- Added `subscribe_cache_events()` for cache change notifications

#### New Traits
- `ImageHandle` — per-file I/O abstraction (read, stream, hash, dimensions, rotate)
- `MetadataHandle` — per-stack metadata I/O (load, write)
- `ImageClassifier` — pluggable image classification (DI'd, owned by SessionManager)

#### New Types
- `ImageRef` — user-facing accessor wrapping `Arc<dyn ImageHandle>` with caching
- `MetadataRef` — user-facing accessor wrapping `Arc<dyn MetadataHandle>` with lazy loading
- `LocalImageHandle` / `LocalMetadataHandle` — filesystem-backed implementations
- `RepoEvent`, `HandleEvent`, `SnapshotEvent`, `StalenessReason` — notification types

#### ScanSnapshot Improvements
- Added `repo_generations` for O(1) staleness detection
- Added `is_stale()` method
- Snapshots now contain PhotoStacks with live handles

#### Search
- Added `repo_id` filter to `SearchQuery` via `with_repo_id()`

### Removed
- `Option<ImageFile>` fields on PhotoStack (replaced by `ImageRef`)
- `Metadata` field on PhotoStack (replaced by `MetadataRef`)
- `PhotoStack::format()` method
- `Serialize`/`Deserialize` derives on PhotoStack
- Per-stack operations on Repository and StackManager traits

## [0.3.0] - 2026-03-20

### Added

- **Foreign repository support** — host languages (.NET, TypeScript) can now implement custom repository backends (e.g., OneDrive, Google Drive, Azure Blob Storage) and register them with `StackManager`
- Core: `RepositoryProvider` trait with `location()`, `list_entries()`, `open_read()`, `open_write()` — hosts provide I/O primitives while Rust handles all scanning, naming convention parsing, and metadata logic
- Core: `ForeignRepository` wraps any `RepositoryProvider` into a full `Repository + FileAccess` implementation; reuses the backend-agnostic `scan_entries()` scanner
- Core: `scan_entries()` extracted from `scan_directory()` — accepts abstract `Vec<FileEntry>` instead of requiring filesystem access
- FFI: C-compatible callback types (`FfiProviderCallbacks`, `FfiFileEntry`, `FfiStreamHandle`, etc.) and `photostax_manager_add_foreign_repo()` function
- FFI: `FfiRepositoryProvider` wraps C function pointers into `RepositoryProvider`; `FfiReader` (Read+Seek) and `FfiWriter` (Write) handle callback-based streaming with proper Drop cleanup
- .NET: `IRepositoryProvider` interface with `Location`, `ListEntries()`, `OpenRead()`, `OpenWrite()` methods
- .NET: `FileEntry` record type and `StackManager.AddRepo(IRepositoryProvider)` overload with `ProviderBridge` marshaling
- TypeScript: `RepositoryProvider` interface with `location`, `listEntries()`, `readFile()`, `writeFile()` methods
- TypeScript: `StackManager.addForeignRepo(provider)` with thread-local `Env` stashing for safe JS↔Rust callbacks
- 20+ new tests for foreign repository (13 core with MockProvider, 7 FFI with mock C callbacks)

### Changed

- Scanner refactored: `scan_directory()` now delegates to `scan_entries()` which is fully backend-agnostic
- `FileEntry` struct added to scanner module (name, folder, path, size)
- Regenerated `photostax.h` C header with new types and functions

## [0.2.2] - 2026-03-20

### Added

- `StackManager` class in TypeScript and .NET bindings for multi-repo management
- FFI: `photostax_manager_new()`, `photostax_manager_add_repo()`, `photostax_manager_repo_count()`
- TypeScript: `StackManager` with `addRepo()`, `repoCount`, and all scan/query/rotate methods
- .NET: `StackManager` with `AddRepo()`, `RepoCount`, and all Scan/Query/Rotate methods
- 6 new FFI tests for multi-repo StackManager functions

### Changed

- Binding READMEs now document StackManager architecture and multi-repo usage

## [0.2.1] - 2026-03-19

### Added

- `StackManager::query(&SearchQuery, Option<&PaginationParams>)` — unified search + pagination in one call
- `PaginatedResult::next_page()` — returns `Option<PaginationParams>` for natural page iteration
- `SearchQuery` now implements `Serialize`/`Deserialize` for JSON interop
- FFI: `photostax_query()` — unified C-compatible query function (search + paginate)
- FFI: `folder` field on `FfiPhotoStack`
- TypeScript: `query(filter?, offset?, limit?)` method on `PhotostaxRepository`
- TypeScript: `name` and `folder` fields on `JsPhotoStack`
- .NET: `Query()` method on `PhotostaxRepository`
- .NET: `Name` and `Folder` properties on `PhotoStack`

### Changed

- `StackManager::stacks()` is deprecated in favor of `query()`
- All binding layers (FFI, TypeScript, .NET, CLI) now use `query()` internally
- CLI `resolve_stack()` uses text search via `query()` instead of linear scan

### Fixed

- CLI no longer requires exact ID match — partial text search resolves stacks

## [0.2.0] - 2026-03-19

### Added

- `ImageFile` struct with lazy content hashing for duplicate detection
- `HashingReader` for opportunistic hash computation during reads
- `FileAccess` trait for backend-polymorphic file I/O with locking semantics
- `StackManager` for unified multi-repository cache with O(1) lookups
- `Repository::watch()` for filesystem change notifications
- `StackEvent`/`CacheEvent` enums for reactive notification cascade
- `Repository::location()` and `id()` for URI-based repo identification
- `PhotoStack::content_hash()` for Merkle-style stack duplicate detection
- `PhotoStack::name` and `folder` fields for human-readable display
- `make_stack_id()` for deterministic opaque stack ID generation
- Overlap detection when registering repos with StackManager

### Changed

- **BREAKING**: Stack IDs are now opaque SHA-256 hashes (16 hex chars) instead of file stems
- **BREAKING**: `PhotoStack` image fields changed from `Option<PathBuf>` to `Option<ImageFile>`
- **BREAKING**: `ImageFile.path` is `String` (not `PathBuf`) to support cloud URIs
- **BREAKING**: `read_image()` returns `Box<dyn ReadSeek>` instead of `Vec<u8>`
- **BREAKING**: `read_image()` takes `&str` instead of `&Path`
- **BREAKING**: `Repository` trait now requires `FileAccess` supertrait
- **BREAKING**: `Repository` trait gained `location()` and `id()` methods
- All binding layers (FFI, TypeScript, CLI) now wrap `StackManager` internally
- Recursive scanning no longer produces colliding IDs for same-named files in different subfolders
- Scanner `Variant` and `classify_stem` are now public

## [0.1.13] - 2026-03-17

### Added

- `CreateSnapshot` now accepts `ScannerProfile` and progress callback for single-pass scanning
  - Rust: `ScanSnapshot::from_scan_with_progress(repo, profile, load_metadata, progress)`
  - FFI: `photostax_create_snapshot_with_progress` with C callback + user_data
  - TypeScript: `createSnapshotWithProgress(profile?, loadMetadata?, callback?)`
  - .NET: `CreateSnapshot(ScannerProfile, bool, Action<ScanPhase, int, int>?)`
- Eliminates redundant re-scanning when creating snapshots with progress reporting

## [0.1.12] - 2026-03-17

### Added

- `ScannerProfile` enum for declaring FastFoto scanner configuration
  - `EnhancedAndBack` — `_a` = enhanced, `_b` = back (no disk I/O)
  - `EnhancedOnly` — `_a` = enhanced only (no disk I/O)
  - `OriginalOnly` — no `_a`/`_b` expected (no disk I/O)
  - `Auto` — unknown config, uses pixel analysis for ambiguous `_a` (disk I/O)
- Multi-pass scan with progress callback (`scan_with_progress`)
  - Pass 1: fast directory scan with per-stack progress reporting
  - Pass 2 (Auto only): classification with per-stack progress reporting
  - FFI: `photostax_repo_scan_with_progress(repo, profile, callback, user_data)`
  - TypeScript: `scanWithProgress(profile?, callback?)` with `{ phase, current, total }`
  - .NET: `ScanWithProgress(ScannerProfile, Action<ScanPhase, int, int>?)`
  - CLI: `--profile` flag on `scan` command with live progress on stderr

## [0.1.11] - 2026-03-16

### Added

- Automatic classification of ambiguous `_a` images as enhanced-front or back-of-photo
  - Pixel variance analysis: low variance + light mean = back of photo; high variance = enhanced front
  - `ClassifyMode` enum: `Auto` (default, analyzes during scan) and `Skip` (legacy behavior)
  - `scan()` now runs classification by default; use `scan_with_classification(ClassifyMode::Skip)` to opt out
  - Best-effort: images that fail to decode or are too small are left unchanged
- Selective rotation targeting with `RotationTarget` enum
  - `All` (default) — rotates original + enhanced + back
  - `Front` — rotates original + enhanced only
  - `Back` — rotates back only
  - FFI: `photostax_rotate_stack` accepts `target` int parameter (0=All, 1=Front, 2=Back)
  - TypeScript: `rotateStack()` accepts optional `target` string (`all`/`front`/`back`)
  - .NET: `RotateStack()` accepts optional `RotationTarget` enum parameter
  - CLI: `rotate` command accepts `--target` flag (`all`/`front`/`back`)

## [0.1.10] - 2026-03-16

### Fixed

- Fix clippy lint: use `!is_empty()` instead of `len() > 0` in snapshot pagination tests
- Remove unused `Repository` import in `ffi/src/snapshot.rs`

## [0.1.9] - 2026-03-16

### Added

- Snapshot-based pagination for consistent page counts across all layers
  - `ScanSnapshot` captures a point-in-time view of stacks — page requests always return consistent totals
  - `ScanSnapshot::get_page(offset, limit)` is pure in-memory and never fails, even if files are deleted
  - `ScanSnapshot::filter(query)` creates a filtered sub-snapshot for search+paginate workflows
  - `ScanSnapshot::check_status(repo)` detects staleness (added/removed files) so callers know when to refresh
  - `SnapshotStatus` reports `is_stale`, `added`, `removed`, `snapshot_count`, `current_count`
  - FFI: `photostax_create_snapshot`, `_get_page`, `_total_count`, `_check_status`, `_filter`, `_free`
  - TypeScript: `createSnapshot()`, `checkSnapshotStatus()` on repo; `ScanSnapshot` class with `getPage()`, `filter()`, `totalCount`
  - .NET: `CreateSnapshot()`, `CheckSnapshotStatus()` on repo; `ScanSnapshot : IDisposable` with `GetPage()`, `Filter()`

## [0.1.8] - 2026-03-14

### Added

- Stack ID allowlist filter for `SearchQuery` across all layers
  - `SearchQuery::with_ids(Vec<String>)` builder method — filters results to only include stacks with matching IDs
  - `stack_ids` field in FFI JSON query format (`{"stack_ids": ["IMG_001", "IMG_002"]}`)
  - `--id` flag on CLI `search` command (comma-separated or repeated: `--id IMG_001,IMG_002`)
  - `stackIds` field on TypeScript `JsSearchQuery` object
  - `WithIds(params string[] ids)` on .NET `SearchQuery` builder

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

[0.4.0]: https://github.com/JeromySt/photostax/compare/v0.3.0...v0.4.0
[0.2.0]: https://github.com/JeromySt/photostax/compare/v0.1.13...v0.2.0
[0.1.10]: https://github.com/JeromySt/photostax/compare/v0.1.9...v0.1.10
[0.1.9]: https://github.com/JeromySt/photostax/compare/v0.1.8...v0.1.9
[0.1.8]: https://github.com/JeromySt/photostax/compare/v0.1.7...v0.1.8
[0.1.7]: https://github.com/JeromySt/photostax/compare/v0.1.5...v0.1.7
[0.1.5]: https://github.com/JeromySt/photostax/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/JeromySt/photostax/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/JeromySt/photostax/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/JeromySt/photostax/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/JeromySt/photostax/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/JeromySt/photostax/releases/tag/v0.1.0

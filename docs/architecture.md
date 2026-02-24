# Architecture

This document describes the overall architecture of the photostax library.

## System Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              Applications                                    │
├─────────────┬─────────────────┬─────────────────┬───────────────────────────┤
│   .NET      │   TypeScript    │      CLI        │     (Future bindings)     │
│   (C#)      │   (Node.js)     │  (photostax-cli)│                           │
├─────────────┴─────────────────┴─────────────────┴───────────────────────────┤
│                            photostax-ffi                                     │
│                        (C-compatible FFI layer)                              │
├─────────────────────────────────────────────────────────────────────────────┤
│                           photostax-core                                     │
│              (Rust core: scanning, metadata, storage, search)                │
├─────────────────────────────────────────────────────────────────────────────┤
│                          Storage Backends                                    │
│           ┌──────────────┬──────────────┬──────────────┐                    │
│           │ Local FS     │ OneDrive     │ Google Drive │                    │
│           │ (implemented)│ (planned)    │ (planned)    │                    │
│           └──────────────┴──────────────┴──────────────┘                    │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Component Responsibilities

### photostax-core (Rust)

The core library is the single source of truth for all business logic:

| Module | Responsibility |
|--------|----------------|
| `photo_stack` | `PhotoStack` and `Metadata` types representing grouped photos |
| `repository` | `Repository` trait for storage backend abstraction |
| `scanner` | Directory scanning and FastFoto file grouping logic |
| `search` | Query builder for filtering stacks by metadata |
| `metadata` | EXIF reading, XMP read/write, sidecar database support |
| `backends` | Storage backend implementations (local filesystem) |

### photostax-ffi (C FFI)

Exposes the Rust core through a C-compatible interface:

- Opaque handle-based API for memory safety
- JSON serialization for complex types
- Thread-safe design with `Send + Sync` bounds
- Error handling via result codes and error strings

### Language Bindings

Each binding provides idiomatic wrappers:

| Binding | Technology | Notes |
|---------|------------|-------|
| .NET | P/Invoke | NuGet package, handles marshalling |
| TypeScript | napi-rs | Native Node.js addon, async support |
| CLI | Rust (clap) | Direct core dependency, no FFI |

## Data Flow

### Scan → Group → Enrich → Search

```
1. SCAN
   ├─ Walk directory tree
   └─ Collect all .jpg/.jpeg/.tif/.tiff files

2. GROUP
   ├─ Parse FastFoto naming pattern (<name>, <name>_a, <name>_b)
   ├─ Group files by base name
   └─ Create PhotoStack objects

3. ENRICH
   ├─ Read EXIF tags from original/enhanced files
   ├─ Read XMP tags (embedded or sidecar)
   ├─ Load custom tags from sidecar database
   └─ Merge into unified Metadata

4. SEARCH
   ├─ Build query with filters
   ├─ Match against indexed metadata
   └─ Return matching PhotoStacks
```

### Metadata Flow

```
Image Files                    Sidecar Database
┌─────────────┐                ┌─────────────────┐
│ EXIF Tags   │──┐             │ Custom Tags     │
│ (read-only) │  │             │ (read/write)    │
├─────────────┤  │             └────────┬────────┘
│ XMP Tags    │  │                      │
│ (read/write)│  │                      │
└──────┬──────┘  │                      │
       │         │                      │
       └─────────┴──────────────────────┘
                         │
                         ▼
              ┌──────────────────────┐
              │ Unified Metadata     │
              │ Priority: XMP > EXIF │
              │ Custom always merged │
              └──────────────────────┘
```

## Thread Safety

- All core types are `Send + Sync`
- Repository operations are internally synchronized
- FFI handles are thread-safe (Arc-wrapped internally)

## Error Handling

| Layer | Strategy |
|-------|----------|
| Core | `Result<T, Error>` with typed error enum |
| FFI | Error codes + `photostax_last_error()` string |
| Bindings | Native exceptions (C#) / Error objects (TS) |

## Future Plans

- **Cloud backends**: OneDrive, Google Drive via OAuth
- **Async API**: Non-blocking I/O for large repositories
- **Incremental scanning**: Watch for file changes
- **Batch operations**: Bulk metadata updates

---

[← Back to main README](../README.md)

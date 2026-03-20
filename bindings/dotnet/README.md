# Photostax .NET Binding

**A .NET wrapper for the photostax library — access Epson FastFoto photo repositories from C#.**

[![NuGet](https://img.shields.io/nuget/v/Photostax.svg)](https://www.nuget.org/packages/Photostax)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](https://github.com/JeromySt/photostax#license)

## Overview

This package provides idiomatic C# access to Epson FastFoto photo repositories. It groups scanner output files (original, enhanced, back scans) into `PhotoStack` objects and provides metadata reading and writing.

## Architecture

`PhotostaxRepository` is a managed wrapper around the Rust `StackManager` — the unified cache and query engine that powers photostax. Each `PhotostaxRepository` instance creates a `StackManager` internally with a single repository, giving you:

- **O(1) stack lookups** by opaque ID
- **Unified `Query()` API** for search + pagination in one call
- **Lazy metadata loading** — scan first, load EXIF/XMP on demand
- **Filesystem watching** and reactive cache updates (Rust-only for now)

> **Multi-repo support:** The Rust core supports managing multiple repositories through a single `StackManager` (via `add_repo()`). This capability is now also available in the .NET binding via the `StackManager` class.

### Multi-repo with StackManager

```csharp
using var mgr = new StackManager();
mgr.AddRepo("/photos/2024", recursive: true);
mgr.AddRepo("/photos/2023", recursive: true);

mgr.Scan();
Console.WriteLine($"Managing {mgr.RepoCount} repos");

// Query across all repos
var page = mgr.Query(new SearchQuery().WithText("birthday"), offset: 0, limit: 20);
foreach (var stack in page.Items)
    Console.WriteLine($"{stack.Name} ({stack.Id})");
```

### Custom Repository Providers

Register a custom backend (e.g., cloud storage) by implementing `IRepositoryProvider`:

```csharp
using Photostax;

public class OneDriveProvider : IRepositoryProvider
{
    public string Location => "onedrive://user/photos";

    public IReadOnlyList<FileEntry> ListEntries(string prefix, bool recursive)
    {
        // Return file listings from OneDrive API
        return new List<FileEntry>
        {
            new("IMG_001.jpg", "vacation", "onedrive://user/photos/vacation/IMG_001.jpg", 2048576),
            new("IMG_001_a.jpg", "vacation", "onedrive://user/photos/vacation/IMG_001_a.jpg", 2148576),
        };
    }

    public Stream OpenRead(string path) => DownloadFromOneDrive(path);
    public Stream OpenWrite(string path) => CreateUploadStream(path);
}

using var mgr = new StackManager();
mgr.AddRepo(new OneDriveProvider(), recursive: true);
mgr.Scan();
```

The host provides I/O primitives while Rust handles all scanning, file grouping, naming convention parsing, and metadata operations.

## Installation

```bash
dotnet add package Photostax
```

## Quick Start

```csharp
using Photostax;

// Open a repository
using var repo = new PhotostaxRepository("/path/to/photos");

// Query all stacks (no filter, no pagination)
var all = repo.Query();

foreach (var stack in all.Items)
{
    // stack.Id is a 16-char hex hash (e.g. "a1b2c3d4e5f67890")
    // stack.Name is the human-readable display name
    Console.WriteLine($"{stack.Name} ({stack.Id})");
    Console.WriteLine($"  Folder: {stack.Folder ?? "(root)"}");
    Console.WriteLine($"  Original: {stack.OriginalPath}");
    Console.WriteLine($"  Enhanced: {stack.EnhancedPath}");
    Console.WriteLine($"  Back: {stack.BackPath}");
    Console.WriteLine($"  Format: {stack.Format}");
}

// Search with filter and pagination using Query()
var page = repo.Query(
    new SearchQuery().WithText("birthday").WithHasBack(true),
    offset: 0,
    limit: 20
);

Console.WriteLine($"Page has {page.Items.Count} of {page.TotalCount} total");
Console.WriteLine($"Has more: {page.HasMore}");

// Iterate pages
foreach (var stack in page.Items)
    Console.WriteLine($"{stack.Name} ({stack.Id})");

if (page.HasMore)
{
    var nextPage = repo.Query(
        new SearchQuery().WithText("birthday"),
        offset: 20, limit: 20);
}

// Read image bytes
var imageData = repo.ReadImage(all.Items[0].OriginalPath!);

// Write metadata (use stack.Id for lookups)
var metadata = new Metadata().WithCustomTag("album", "Family Photos");
repo.WriteMetadata(all.Items[0].Id, metadata);
```

> **Note — Stack IDs changed in v0.2.x:** Stack IDs are now opaque 16-character
> hex strings (truncated SHA-256 hashes) rather than human-readable stems.
> Use `stack.Name` when displaying a stack to users and `stack.Id` for
> programmatic lookups such as `GetStack()` or `WriteMetadata()`.

## API Overview

### PhotostaxRepository

The main entry point for working with photo repositories.

| Method | Description |
|--------|-------------|
| `Query()` | **(v0.2.x)** Search and paginate in one call. Accepts an optional `SearchQuery`, `offset`, and `limit`. Returns a `PagedResult<PhotoStack>`. |
| `Scan()` | Discover all photo stacks in the repository |
| `GetStack(id)` | Get a specific stack by its opaque hash ID |
| `ReadImage(path)` | Read raw image bytes |
| `WriteMetadata(id, metadata)` | Write metadata to a stack |
| `Search(query)` | Find stacks matching a query (convenience wrapper around `Query()`) |
| `ScanPaginated(offset, limit)` | Scan with pagination (convenience wrapper around `Query()`) |
| `SearchPaginated(query, offset, limit)` | Search with pagination (convenience wrapper around `Query()`) |

### StackManager

Multi-repository manager for unified access across directories and custom backends.

| Method | Description |
|--------|-------------|
| `new StackManager()` | Create an empty manager |
| `AddRepo(path, ...)` | Register a local directory |
| `AddRepo(IRepositoryProvider, ...)` | Register a custom repository provider |
| `RepoCount` | Number of registered repositories |
| `StackCount` | Total stacks in cache |
| `Scan()` | Scan all registered repos |
| `ScanWithMetadata()` | Scan with full EXIF/XMP loading |
| `GetStack(id)` | Retrieve a single stack by opaque ID |
| `LoadMetadata(id)` | Load metadata for a specific stack |
| `ReadImage(path)` | Read raw image bytes |
| `WriteMetadata(id, metadata)` | Write metadata to a stack |
| `Query(filter?, offset, limit)` | Search + paginate across all repos |
| `RotateStack(id, degrees, target?)` | Rotate images in a stack |
| `CreateSnapshot(loadMetadata?)` | Create a point-in-time snapshot |

### IRepositoryProvider

Interface for custom repository backends:

```csharp
public interface IRepositoryProvider
{
    string Location { get; }
    IReadOnlyList<FileEntry> ListEntries(string prefix, bool recursive);
    Stream OpenRead(string path);
    Stream OpenWrite(string path);
}

public record FileEntry(string Name, string Folder, string Path, long Size);
```

#### `Query()` — Preferred Search & Pagination API

`Query()` is the recommended way to search and paginate as of v0.2.x.
The older `Search()`, `ScanPaginated()`, and `SearchPaginated()` methods still
work but are now convenience wrappers around `Query()`.

```csharp
// All stacks (no filter, no pagination)
var all = repo.Query();

// With filter and pagination
var page = repo.Query(
    new SearchQuery().WithText("birthday").WithHasBack(true),
    offset: 0,
    limit: 20
);

// Iterate pages
foreach (var stack in page.Items)
    Console.WriteLine($"{stack.Name} ({stack.Id})");

if (page.HasMore)
{
    var nextPage = repo.Query(
        new SearchQuery().WithText("birthday"),
        offset: 20, limit: 20);
}
```

### PhotoStack

Represents a photo stack with its associated images and metadata.

| Property | Type | Description |
|----------|------|-------------|
| `Id` | `string` | Opaque 16-char hex hash (SHA-256). Use for lookups. |
| `Name` | `string` | **(v0.2.x)** Human-readable display name for the stack. |
| `Folder` | `string?` | **(v0.2.x)** Subfolder within the repository, or `null` if at root. |
| `OriginalPath` | `string?` | Path to original scan |
| `EnhancedPath` | `string?` | Path to enhanced scan |
| `BackPath` | `string?` | Path to back scan |
| `Metadata` | `Metadata` | EXIF, XMP, and custom tags |
| `Format` | `ImageFormat?` | JPEG, TIFF, or Unknown |

### SearchQuery

Builder for constructing search queries.

```csharp
var query = new SearchQuery()
    .WithText("vacation")           // Free-text search
    .WithExifFilter("Make", "EPSON") // EXIF tag filter
    .WithCustomFilter("album", "2020") // Custom tag filter
    .WithHasBack(true)              // Has back scan
    .WithHasEnhanced(true);         // Has enhanced scan
```

## Building from Source

### Prerequisites

- [.NET SDK 8.0+](https://dotnet.microsoft.com/download)
- [Rust toolchain](https://rustup.rs/) (for building native library)

### Build Steps

```bash
# 1. Build native library
cd <repo_root>
cargo build --release -p photostax-ffi

# 2. Build .NET library
cd bindings/dotnet
dotnet build

# 3. Run tests
dotnet test
```

### Native Library Location

The native library must be in your application's runtime directory:

| Platform | File |
|----------|------|
| Windows | `photostax_ffi.dll` |
| macOS | `libphotostax_ffi.dylib` |
| Linux | `libphotostax_ffi.so` |

## Running Tests

```bash
cd bindings/dotnet
dotnet test

# Skip integration tests (no native library required)
dotnet test --filter "Category!=Integration"
```

## License

Licensed under either of [Apache License, Version 2.0](../../LICENSE-APACHE) or [MIT License](../../LICENSE-MIT) at your option.

---

[← Back to main README](../../README.md) | [FFI Documentation](../../ffi/README.md)

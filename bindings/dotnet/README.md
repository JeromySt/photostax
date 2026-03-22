# Photostax .NET Binding

**A .NET wrapper for the photostax library — access Epson FastFoto photo repositories from C#.**

[![NuGet](https://img.shields.io/nuget/v/Photostax.svg)](https://www.nuget.org/packages/Photostax)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](https://github.com/JeromySt/photostax#license)

## Overview

This package provides idiomatic C# access to Epson FastFoto photo repositories. It groups scanner output files (original, enhanced, back scans) into `PhotoStack` objects with lazy, cached accessors for image data and metadata.

## Architecture

`PhotostaxRepository` is a managed wrapper around the Rust `SessionManager` (formerly `StackManager`) — the unified cache and query engine that powers photostax. Each `PhotostaxRepository` instance creates a `SessionManager` internally with a single repository, giving you:

- **O(1) stack lookups** by opaque ID
- **PhotoStack-centric I/O** — `stack.Original.Read()`, `stack.Metadata.Read()`, etc.
- **Lazy, cached accessors** — image data and metadata loaded on demand
- **ScanSnapshot queries** — `Query()` returns a `QueryResult` for page-based navigation

> **Multi-repo support:** Use the `StackManager` class to manage multiple repositories through a single cache.

### Multi-repo with StackManager

```csharp
using var mgr = new StackManager();
mgr.AddRepo("/photos/2024", recursive: true);
mgr.AddRepo("/photos/2023", recursive: true);
Console.WriteLine($"Managing {mgr.RepoCount} repos");

// Query across all repos — returns a QueryResult (auto-scans on first call)
var result = mgr.Query(new SearchQuery().WithText("birthday"), pageSize: 20);
foreach (var stack in result.CurrentPage)
{
    Console.WriteLine($"{stack.Name} ({stack.Id})");

    // Per-stack image I/O (no manager methods needed)
    if (stack.Original.IsPresent)
    {
        using var stream = stack.Original.Read();
        // ... process image ...
    }
}
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
// Query() auto-scans on first call
```

The host providesI/O primitives while Rust handles all scanning, file grouping, naming convention parsing, and metadata operations.

## Installation

```bash
dotnet add package Photostax
```

## Quick Start

```csharp
using Photostax;

// Open a repository
using var repo = new PhotostaxRepository("/path/to/photos");

// Query all stacks — Query() is the sole entry point for retrieving stacks
var result = repo.Query(pageSize: 20);

// Iterate the first page
foreach (var stack in result.CurrentPage)
{
    Console.WriteLine($"{stack.Name}: {stack.Original.IsPresent}");
    Console.WriteLine($"  Folder: {stack.Folder ?? "(root)"}");

    // Read images via ImageRef (lazy, cached)
    if (stack.Original.IsPresent)
    {
        using var stream = stack.Original.Read();
        Console.WriteLine($"  Original: {stack.Original.Size} bytes");
        Console.WriteLine($"  Hash: {stack.Original.Hash()}");
    }

    // Read metadata via MetadataRef (lazy-loaded)
    var meta = stack.Metadata.Read();
    Console.WriteLine($"  Camera: {meta.ExifTags.GetValueOrDefault("Make", "unknown")}");
}

// Navigate remaining pages
while (result.NextPage() is { } page)
{
    foreach (var stack in page)
    {
        Console.WriteLine($"{stack.Name} ({stack.Id})");
    }
}

// Filtered query — pass a SearchQuery to filter instead of scan
var filtered = repo.Query(
    new SearchQuery().WithText("birthday").WithHasBack(true),
    pageSize: 20
);
Console.WriteLine($"Page has {filtered.CurrentPage.Count} of {filtered.TotalCount} total");
```

## API Overview

### PhotostaxRepository

The main entry point for working with photo repositories.

| Method | Description |
|--------|-------------|
| `Query(query?, pageSize, onProgress?)` | Returns a `QueryResult` for page-based navigation. Auto-scans on first call. |

### StackManager

Multi-repository manager for unified access across directories and custom backends.

| Method | Description |
|--------|-------------|
| `new StackManager()` | Create an empty manager |
| `AddRepo(path, ...)` | Register a local directory |
| `AddRepo(IRepositoryProvider, ...)` | Register a custom repository provider |
| `RepoCount` | Number of registered repositories |
| `Query(query?, pageSize, onProgress?)` | Search across all repos, returns `QueryResult`. Auto-scans on first call. |

### PhotoStack (v0.4.0)

All I/O operations are accessed directly on the stack object:

| Property | Type | Description |
|----------|------|-------------|
| `Id` | `string` | Opaque 16-char hex hash (SHA-256). Use for lookups. |
| `Name` | `string` | Human-readable display name for the stack. |
| `Folder` | `string?` | Subfolder within the repository, or `null` if at root. |
| `Original` | `ImageRef` | Original front scan accessor — `Read()`, `Hash()`, `Dimensions()`, `Rotate()` |
| `Enhanced` | `ImageRef` | Enhanced scan accessor |
| `Back` | `ImageRef` | Back scan accessor |
| `Metadata` | `MetadataRef` | Metadata accessor — `Read()`, `Write()` |

### ImageRef

Lazy, cached accessor for a single image variant:

| Method | Description |
|--------|-------------|
| `IsPresent` | Whether this image variant exists |
| `Read()` | Read image bytes as a `Stream` |
| `Hash()` | SHA-256 content hash (cached after first call) |
| `Dimensions()` | Image width and height (cached) |
| `Size` | File size in bytes |
| `Rotate(degrees)` | Rotate image in place |

### MetadataRef

Lazy accessor for stack metadata:

| Method | Description |
|--------|-------------|
| `Read()` | Load EXIF, XMP, and custom tags (lazy-loaded) |
| `Write(metadata)` | Write metadata back |

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

### SearchQuery

Builder for constructing search queries.

```csharp
var query = new SearchQuery()
    .WithText("vacation")           // Free-text search
    .WithExifFilter("Make", "EPSON") // EXIF tag filter
    .WithCustomFilter("album", "2020") // Custom tag filter
    .WithHasBack(true)              // Has back scan
    .WithRepoId("a1b2c3d4");       // Filter by repository (v0.4.0)
```

### QueryResult

Page-based query result with navigation:

| Property/Method | Type | Description |
|----------------|------|-------------|
| `CurrentPage` | `IReadOnlyList<PhotoStack>` | Stacks on the current page |
| `TotalCount` | `int` | Total matching stacks across all pages |
| `PageCount` | `int` | Total number of pages |
| `CurrentPageIndex` | `int` | Zero-based index of current page |
| `PageSize` | `int` | Items per page |
| `NextPage()` | `IReadOnlyList<PhotoStack>?` | Advance to next page, returns it (or null) |
| `PreviousPage()` | `IReadOnlyList<PhotoStack>?` | Go to previous page (or null) |
| `SetPage(index)` | `IReadOnlyList<PhotoStack>?` | Jump to specific page (or null) |
| `Query(query)` | `QueryResult` | Sub-query on current result set |

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

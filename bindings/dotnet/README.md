# Photostax .NET Binding

**A .NET wrapper for the photostax library — access Epson FastFoto photo repositories from C#.**

[![NuGet](https://img.shields.io/nuget/v/Photostax.svg)](https://www.nuget.org/packages/Photostax)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](https://github.com/JeromySt/photostax#license)

## Overview

This package provides idiomatic C# access to Epson FastFoto photo repositories. It groups scanner output files (original, enhanced, back scans) into `PhotoStack` objects and provides metadata reading and writing.

## Installation

```bash
dotnet add package Photostax
```

## Quick Start

```csharp
using Photostax;

// Open a repository
using var repo = new PhotostaxRepository("/path/to/photos");

// Scan for all photo stacks
var stacks = repo.Scan();
foreach (var stack in stacks)
{
    Console.WriteLine($"Stack: {stack.Id}");
    Console.WriteLine($"  Original: {stack.OriginalPath}");
    Console.WriteLine($"  Enhanced: {stack.EnhancedPath}");
    Console.WriteLine($"  Back: {stack.BackPath}");
    Console.WriteLine($"  Format: {stack.Format}");
}

// Search for specific photos
var query = new SearchQuery()
    .WithText("vacation")
    .WithExifFilter("Make", "EPSON")
    .WithHasBack(true);

var results = repo.Search(query);

// Paginate results
var page = repo.ScanPaginated(offset: 0, limit: 20);
Console.WriteLine($"Page has {page.Items.Count} of {page.TotalCount} total");
Console.WriteLine($"Has more: {page.HasMore}");

// Read image bytes
var imageData = repo.ReadImage(stacks[0].OriginalPath!);

// Write metadata
var metadata = new Metadata().WithCustomTag("album", "Family Photos");
repo.WriteMetadata(stacks[0].Id, metadata);
```

## API Overview

### PhotostaxRepository

The main entry point for working with photo repositories.

| Method | Description |
|--------|-------------|
| `Scan()` | Discover all photo stacks in the repository |
| `GetStack(id)` | Get a specific stack by ID |
| `ReadImage(path)` | Read raw image bytes |
| `WriteMetadata(id, metadata)` | Write metadata to a stack |
| `Search(query)` | Find stacks matching a query |
| `ScanPaginated(offset, limit)` | Scan with pagination (offset/limit) |
| `SearchPaginated(query, offset, limit)` | Search with pagination (offset/limit) |

### PhotoStack

Represents a photo stack with its associated images and metadata.

| Property | Type | Description |
|----------|------|-------------|
| `Id` | `string` | Unique stack identifier |
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

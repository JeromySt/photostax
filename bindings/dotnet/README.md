# Photostax .NET Binding

A .NET wrapper for the `photostax-ffi` native library, providing idiomatic C# access to Epson FastFoto photo repositories.

## Installation

### NuGet Package

```bash
dotnet add package Photostax
```

### Building from Source

1. Clone the repository
2. Build the native library (see [Building Native Library](#building-native-library))
3. Build the .NET library:

```bash
cd bindings/dotnet
dotnet build
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

// Read image bytes
var imageData = repo.ReadImage(stacks[0].OriginalPath!);

// Write metadata
var metadata = new Metadata().WithCustomTag("album", "Family Photos");
repo.WriteMetadata(stacks[0].Id, metadata);
```

## API Reference

### PhotostaxRepository

The main entry point for working with photo repositories.

```csharp
public sealed class PhotostaxRepository : IDisposable
{
    // Open a repository at the specified path
    public PhotostaxRepository(string directoryPath);
    
    // Scan for all photo stacks
    public IReadOnlyList<PhotoStack> Scan();
    
    // Get a specific stack by ID
    public PhotoStack GetStack(string id);
    
    // Read image bytes
    public byte[] ReadImage(string path);
    
    // Write metadata to a stack
    public void WriteMetadata(string stackId, Metadata metadata);
    
    // Search for stacks matching a query
    public IReadOnlyList<PhotoStack> Search(SearchQuery query);
}
```

### PhotoStack

Represents a photo stack with its associated images and metadata.

```csharp
public sealed class PhotoStack
{
    public string Id { get; }
    public string? OriginalPath { get; }
    public string? EnhancedPath { get; }
    public string? BackPath { get; }
    public Metadata Metadata { get; }
    public bool HasAnyImage { get; }
    public ImageFormat? Format { get; }
}
```

### Metadata

Metadata associated with a photo stack, including EXIF, XMP, and custom tags.

```csharp
public sealed class Metadata
{
    public IReadOnlyDictionary<string, string> ExifTags { get; }
    public IReadOnlyDictionary<string, string> XmpTags { get; }
    public IReadOnlyDictionary<string, object?> CustomTags { get; }
    
    // Create a new metadata with a custom tag added/updated
    public Metadata WithCustomTag(string key, object? value);
}
```

### SearchQuery

Builder for constructing search queries.

```csharp
public sealed class SearchQuery
{
    public SearchQuery WithText(string text);
    public SearchQuery WithExifFilter(string key, string contains);
    public SearchQuery WithCustomFilter(string key, string contains);
    public SearchQuery WithHasBack(bool hasBack);
    public SearchQuery WithHasEnhanced(bool hasEnhanced);
}
```

### ImageFormat

Enum representing supported image formats.

```csharp
public enum ImageFormat
{
    Jpeg,
    Png,
    Tiff,
    Unknown
}
```

## Building Native Library

The native library must be built before using this binding:

```bash
# From repository root
cargo build --release -p photostax-ffi
```

The resulting library will be in `target/release/`:
- Windows: `photostax_ffi.dll`
- macOS: `libphotostax_ffi.dylib`
- Linux: `libphotostax_ffi.so`

Ensure the native library is in your application's runtime directory or system library path.

## Running Tests

```bash
cd bindings/dotnet
dotnet test
```

To run integration tests (requires native library):

```bash
dotnet test --filter "Category!=Integration"  # Skip integration tests
dotnet test                                    # Run all tests (requires native lib)
```

## License

This project is dual-licensed under MIT or Apache-2.0.

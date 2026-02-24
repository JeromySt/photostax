# photostax-ffi

**C FFI bindings for the photostax-core library.**

[![Crates.io](https://img.shields.io/crates/v/photostax-ffi.svg)](https://crates.io/crates/photostax-ffi)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](https://github.com/JeromySt/photostax#license)

## Overview

This crate provides C-compatible FFI bindings for `photostax-core`, enabling integration with C, C++, C#, Python, and other languages via P/Invoke or similar mechanisms.

## Installation

For language-specific bindings, use the official packages:

| Language | Package | Install |
|----------|---------|---------|
| .NET/C# | `Photostax` | `dotnet add package Photostax` |
| TypeScript | `@photostax/core` | `npm install @photostax/core` |

For direct FFI usage, build from source (see below).

## Building from Source

```sh
cargo build --release --package photostax-ffi
```

This produces:
- `libphotostax_ffi.so` (Linux)
- `libphotostax_ffi.dylib` (macOS)
- `photostax_ffi.dll` (Windows)

The C header file is generated at `ffi/photostax.h`.

## Quick Start (C)

```c
#include <stdio.h>
#include "photostax.h"

int main() {
    printf("photostax version: %s\n", photostax_version());

    PhotostaxRepository repo = photostax_repository_new("/path/to/photos");
    if (repo) {
        int count = photostax_repository_scan_count(repo);
        printf("Found %d photo stacks\n", count);
        photostax_repository_free(repo);
    }
    return 0;
}
```

## Quick Start (C# P/Invoke)

```csharp
using System.Runtime.InteropServices;

public static class PhotoStax
{
    [DllImport("photostax_ffi")]
    public static extern IntPtr photostax_repository_new(string path);

    [DllImport("photostax_ffi")]
    public static extern void photostax_repository_free(IntPtr repo);

    [DllImport("photostax_ffi")]
    public static extern int photostax_repository_scan_count(IntPtr repo);

    [DllImport("photostax_ffi")]
    public static extern IntPtr photostax_version();
}
```

## API Overview

### Repository Functions

| Function | Description |
|----------|-------------|
| `photostax_repository_new(path)` | Create a repository handle |
| `photostax_repository_free(repo)` | Free a repository handle |
| `photostax_repository_scan_count(repo)` | Scan and return photo stack count |
| `photostax_repository_scan_json(repo)` | Scan and return JSON array of stacks |

### Metadata Functions

| Function | Description |
|----------|-------------|
| `photostax_stack_metadata_json(stack)` | Get metadata as JSON |
| `photostax_repository_write_metadata(repo, id, json)` | Write metadata to a stack |

### Utility Functions

| Function | Description |
|----------|-------------|
| `photostax_version()` | Get library version string |
| `photostax_last_error()` | Get last error message |
| `photostax_string_free(str)` | Free a string returned by FFI |

## Memory Management

- **Handles** returned by `*_new()` must be freed with corresponding `*_free()`
- **Strings** returned by `*_json()` functions must be freed with `photostax_string_free()`
- **Const strings** (like `photostax_version()`) are owned by the library and must not be freed

See [docs/bindings-guide.md](../docs/bindings-guide.md) for detailed FFI conventions.

## License

Licensed under either of [Apache License, Version 2.0](../LICENSE-APACHE) or [MIT License](../LICENSE-MIT) at your option.

---

[← Back to main README](../README.md) | [Bindings Guide](../docs/bindings-guide.md)

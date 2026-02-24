# photostax-ffi

**C FFI bindings for the photostax-core library.**

[![Crates.io](https://img.shields.io/crates/v/photostax-ffi.svg)](https://crates.io/crates/photostax-ffi)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](https://github.com/JeromySt/photostax#license)

## Overview

This crate provides C-compatible FFI bindings for `photostax-core`, enabling integration with C, C++, C#, Python, and other languages via P/Invoke or similar mechanisms.

## Building

```sh
cargo build --release --package photostax-ffi
```

This produces:
- `libphotostax_ffi.so` (Linux)
- `libphotostax_ffi.dylib` (macOS)
- `photostax_ffi.dll` (Windows)

## Usage (C)

```c
#include <stdio.h>

// Forward declarations
void* photostax_repository_new(const char* path);
void photostax_repository_free(void* repo);
int photostax_repository_scan_count(void* repo);
const char* photostax_version(void);

int main() {
    printf("photostax version: %s\n", photostax_version());

    void* repo = photostax_repository_new("/path/to/photos");
    if (repo) {
        int count = photostax_repository_scan_count(repo);
        printf("Found %d photo stacks\n", count);
        photostax_repository_free(repo);
    }
    return 0;
}
```

## Usage (C# P/Invoke)

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

## API

| Function | Description |
|----------|-------------|
| `photostax_repository_new(path)` | Create a repository handle |
| `photostax_repository_free(repo)` | Free a repository handle |
| `photostax_repository_scan_count(repo)` | Scan and return photo stack count |
| `photostax_version()` | Get library version string |

## License

Licensed under either of [Apache License, Version 2.0](../LICENSE-APACHE) or [MIT License](../LICENSE-MIT) at your option.

See the [main repository](https://github.com/JeromySt/photostax) for more details.

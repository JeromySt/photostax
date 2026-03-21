# Language Bindings Guide

This document explains how to create new language bindings for photostax.

## Architecture Overview

All language bindings communicate with the Rust core through the FFI layer:

```
Your Language Binding
        │
        ▼
   Native Interop (P/Invoke, napi-rs, ctypes, etc.)
        │
        ▼
   photostax-ffi (C-compatible shared library)
        │
        ▼
   photostax-core (Rust implementation)
```

## FFI Layer Design

### Shared Library Output

Building `photostax-ffi` produces platform-specific shared libraries:

| Platform | File | Notes |
|----------|------|-------|
| Windows | `photostax_ffi.dll` | Copy to application directory |
| macOS | `libphotostax_ffi.dylib` | Install to `/usr/local/lib` or bundle |
| Linux | `libphotostax_ffi.so` | Install to `/usr/lib` or use `LD_LIBRARY_PATH` |

### API Conventions

The FFI layer follows these conventions:

#### 1. Opaque Handles

Complex types are exposed as opaque pointers:

```c
// Opaque handle type
typedef void* PhotostaxRepository;

// Create handle
PhotostaxRepository photostax_repository_new(const char* path);

// Use handle
int photostax_repository_scan_count(PhotostaxRepository repo);

// Free handle
void photostax_repository_free(PhotostaxRepository repo);
```

#### 2. Error Handling

Functions return error codes with details available via `photostax_last_error()`:

```c
typedef enum {
    PHOTOSTAX_OK = 0,
    PHOTOSTAX_ERR_NULL_POINTER = 1,
    PHOTOSTAX_ERR_INVALID_PATH = 2,
    PHOTOSTAX_ERR_IO_ERROR = 3,
    PHOTOSTAX_ERR_PARSE_ERROR = 4,
} PhotostaxResult;

// Get last error message (thread-local)
const char* photostax_last_error(void);

// Clear last error
void photostax_clear_error(void);
```

#### 3. String Handling

Strings use null-terminated UTF-8:

```c
// Input: Caller owns the string, FFI copies if needed
PhotostaxRepository photostax_repository_new(const char* path);

// Output: FFI owns the string, valid until next call or free
const char* photostax_stack_get_id(PhotostaxStack stack);

// Output with explicit ownership transfer
char* photostax_get_json(PhotostaxRepository repo);  // Caller must free
void photostax_string_free(char* str);
```

#### 4. Complex Data via JSON

Complex structures are serialized as JSON strings:

```c
// Returns JSON array of PhotoStack objects
// Caller must free with photostax_string_free
char* photostax_repository_scan_json(PhotostaxRepository repo);

// Returns JSON object with metadata
char* photostax_stack_metadata_json(PhotostaxStack stack);
```

#### 5. Pagination

Paginated variants return an `FfiPaginatedResult` struct:

```c
typedef struct {
    FfiPhotoStack* data;   // Array of photo stacks
    size_t len;            // Number of items in this page
    size_t total_count;    // Total matching items across all pages
    size_t offset;         // Offset used for this page
    size_t limit;          // Limit used for this page
    bool has_more;         // Whether more items exist beyond this page
} FfiPaginatedResult;

// Paginated scan
FfiPaginatedResult photostax_repo_scan_paginated(
    const PhotostaxRepo* repo, size_t offset, size_t limit);

// Paginated search
FfiPaginatedResult photostax_search_paginated(
    const PhotostaxRepo* repo, const char* query_json,
    size_t offset, size_t limit);

// Free paginated result memory
void photostax_paginated_result_free(FfiPaginatedResult result);
```

## Creating a New Binding

### Step 1: Choose Your Interop Technology

| Language | Recommended Technology |
|----------|----------------------|
| C# / .NET | P/Invoke (`DllImport`) |
| Python | ctypes or cffi |
| Node.js | napi-rs or node-ffi-napi |
| Go | cgo |
| Ruby | FFI gem |
| Java | JNI or JNA |

### Step 2: Define Native Function Declarations

Example for Python with ctypes:

```python
import ctypes
from ctypes import c_char_p, c_void_p, c_int

# Load the shared library
lib = ctypes.CDLL("libphotostax_ffi.so")

# Declare function signatures
lib.photostax_repository_new.argtypes = [c_char_p]
lib.photostax_repository_new.restype = c_void_p

lib.photostax_repository_free.argtypes = [c_void_p]
lib.photostax_repository_free.restype = None

lib.photostax_repository_scan_json.argtypes = [c_void_p]
lib.photostax_repository_scan_json.restype = c_char_p

lib.photostax_string_free.argtypes = [c_char_p]
lib.photostax_string_free.restype = None
```

### Step 3: Create Idiomatic Wrappers

Wrap the raw FFI calls in language-idiomatic classes:

```python
import json
from contextlib import contextmanager

class PhotostaxRepository:
    def __init__(self, path: str):
        self._handle = lib.photostax_repository_new(path.encode('utf-8'))
        if not self._handle:
            raise RuntimeError(lib.photostax_last_error().decode('utf-8'))
    
    def __del__(self):
        if self._handle:
            lib.photostax_repository_free(self._handle)
    
    def scan(self) -> list:
        json_ptr = lib.photostax_repository_scan_json(self._handle)
        if not json_ptr:
            raise RuntimeError(lib.photostax_last_error().decode('utf-8'))
        try:
            return json.loads(json_ptr.decode('utf-8'))
        finally:
            lib.photostax_string_free(json_ptr)
    
    def __enter__(self):
        return self
    
    def __exit__(self, *args):
        if self._handle:
            lib.photostax_repository_free(self._handle)
            self._handle = None
```

### Step 4: Handle Memory Management

**Critical rules:**

1. **Handles must be freed**: Every `*_new()` needs a corresponding `*_free()`
2. **Strings from FFI**: Check ownership - some are borrowed, some need `photostax_string_free()`
3. **Use RAII patterns**: Wrap handles in classes with destructors
4. **Thread safety**: Handles are `Send + Sync`, but not `Clone`

### Step 5: Parse JSON Responses

Define types matching the JSON schema:

```python
from dataclasses import dataclass
from typing import Optional, Dict, Any

@dataclass
class PhotoStack:
    id: str
    original: Optional[str]
    enhanced: Optional[str]
    back: Optional[str]
    metadata: 'Metadata'
    
    @classmethod
    def from_json(cls, data: dict) -> 'PhotoStack':
        return cls(
            id=data['id'],
            original=data.get('original'),
            enhanced=data.get('enhanced'),
            back=data.get('back'),
            metadata=Metadata.from_json(data['metadata'])
        )

@dataclass
class Metadata:
    exif_tags: Dict[str, str]
    xmp_tags: Dict[str, str]
    custom_tags: Dict[str, Any]
    
    @classmethod
    def from_json(cls, data: dict) -> 'Metadata':
        return cls(
            exif_tags=data.get('exifTags', {}),
            xmp_tags=data.get('xmpTags', {}),
            custom_tags=data.get('customTags', {})
        )
```

## Memory Management Rules

### Handle Lifecycle

```
┌─────────────────────────────────────────────────────────────┐
│ photostax_repository_new(path)                              │
│         │                                                   │
│         ▼                                                   │
│    Handle Created ─── Use in subsequent calls ───┐          │
│                                                  │          │
│                                                  ▼          │
│                           photostax_repository_free(handle) │
│                                      │                      │
│                                      ▼                      │
│                               Handle Invalid                │
└─────────────────────────────────────────────────────────────┘
```

### String Ownership

| Function Pattern | Ownership | Action Required |
|-----------------|-----------|-----------------|
| `const char* photostax_*()` | FFI owns | Do not free, copy if needed |
| `char* photostax_*_json()` | Caller owns | Must call `photostax_string_free()` |
| `void photostax_*(const char*)` | FFI copies | Safe to free after call |

## Error Handling Patterns

### Check Every Return Value

```python
def repository_new_safe(path: str):
    handle = lib.photostax_repository_new(path.encode('utf-8'))
    if not handle:
        error = lib.photostax_last_error()
        if error:
            raise RuntimeError(error.decode('utf-8'))
        raise RuntimeError("Unknown error creating repository")
    return handle
```

### Use Result Codes

```python
result = lib.photostax_repository_write_metadata(handle, stack_id, json_data)
if result != 0:  # PHOTOSTAX_OK
    error = lib.photostax_last_error()
    raise RuntimeError(f"Write failed: {error.decode('utf-8')}")
```

## Testing Your Binding

### Unit Tests

Test each wrapper independently:

```python
def test_repository_creation():
    with PhotostaxRepository("/path/to/photos") as repo:
        assert repo._handle is not None

def test_query_returns_list():
    with PhotostaxRepository("/path/to/photos") as repo:
        stacks = repo.query()  # query() auto-scans on first call
        assert isinstance(stacks, list)
```

### Integration Tests

Test against real photo directories:

```python
def test_full_workflow():
    with PhotostaxRepository("./test_photos") as repo:
        stacks = repo.query()  # query() auto-scans on first call
        assert len(stacks) > 0
        
        stack = stacks[0]
        assert stack.id is not None
        assert stack.metadata.exif_tags.get('Make') == 'EPSON'
```

### Memory Leak Tests

Use memory profilers to verify no leaks:

```python
import tracemalloc

tracemalloc.start()
for _ in range(1000):
    with PhotostaxRepository("/path") as repo:
        _ = repo.query()  # query() auto-scans on first call
current, peak = tracemalloc.get_traced_memory()
tracemalloc.stop()
assert current < 1_000_000  # Less than 1MB retained
```

## Packaging and Distribution

### Native Library Distribution

Options for distributing the native library:

1. **Bundled in package**: Include prebuilt binaries for all platforms
2. **System install**: Require users to install globally
3. **Build from source**: Include Rust build in package install

### Example Package Structure (Python)

```
photostax-python/
├── pyproject.toml
├── src/
│   └── photostax/
│       ├── __init__.py
│       ├── _native.py      # FFI declarations
│       ├── repository.py   # Wrapper classes
│       └── types.py        # Data classes
├── native/
│   ├── linux-x64/libphotostax_ffi.so
│   ├── darwin-x64/libphotostax_ffi.dylib
│   ├── darwin-arm64/libphotostax_ffi.dylib
│   └── win32-x64/photostax_ffi.dll
└── tests/
```

## Existing Bindings Reference

Study the existing bindings for patterns:

| Binding | Location | Technology |
|---------|----------|------------|
| .NET/C# | `bindings/dotnet/` | P/Invoke |
| TypeScript | `bindings/typescript/` | napi-rs |

---

[← Back to main README](../README.md) | [Architecture →](architecture.md)

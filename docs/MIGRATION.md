# Migration Guide

## v0.4.x → v0.5.0

### ⚠️ Breaking: query() is the sole entry point

v0.5.0 removes `get_stack()`, `get_stack_mut()`, `scan()`, and other direct-access methods. `query()` is now the only way to retrieve stacks.

### Retrieving stacks

**Before (v0.4.x):**
```rust
// Rust — get a single stack by ID
let stack = mgr.get_stack("abc123")?;

// C# — get a stack
var stack = mgr.GetStack("abc123");

// TypeScript — get a stack
const stack = mgr.getStack("abc123");
```

**After (v0.5.0):**
```rust
// Rust — query with ID filter
let result = mgr.query(
    Some(&SearchQuery::new().with_ids(vec!["abc123".into()])),
    None, None,
)?;
let stack = result.current_page().first().unwrap();

// C# — query with ID filter
var result = mgr.Query(new SearchQuery().WithIds("abc123"));
var stack = result.CurrentPage.First();

// TypeScript — query with ID filter
const result = mgr.query({ stackIds: ["abc123"] });
const stack = result.currentPage()[0];
```

### Scanning

**Before (v0.4.x):**
```rust
// Rust
mgr.scan()?;
let stacks = mgr.all_stacks();

// C#
repo.Scan();
repo.ScanWithProgress((phase, cur, total) => { });
```

**After (v0.5.0):**
```rust
// Rust — query() auto-scans on first call
let result = mgr.query(None, None, None)?;

// With progress callback
let result = mgr.query(None, None, Some(&mut |p| {
    println!("{:?}: {}/{}", p.phase, p.current, p.total);
}))?;

// C# — Query() auto-scans on first call
var result = repo.Query(onProgress: (phase, cur, total) => {
    Console.WriteLine($"{phase}: {cur}/{total}");
});
```

### Pagination (QueryResult replaces ScanSnapshot)

**Before (v0.4.x):**
```rust
// Rust — ScanSnapshot with get_page()
let snap = mgr.query(Some(&query), None)?;
let page = snap.get_page(0, 20);
if page.has_more {
    let page2 = snap.get_page(20, 20);
}
```

**After (v0.5.0):**
```rust
// Rust — QueryResult with page navigation
let mut result = mgr.query(Some(&query), Some(20), None)?;
let page1 = result.current_page();         // first page
let page2 = result.next_page();            // Option<&[PhotoStack]>
let page3 = result.next_page();            // next page or None
result.set_page(0);                        // jump to specific page

// C#
var result = mgr.Query(query, pageSize: 20);
var page1 = result.CurrentPage;
var page2 = result.NextPage();             // IReadOnlyList<PhotoStack>?
result.SetPage(0);                         // jump back

// TypeScript
const result = mgr.query(query, 20);
const page1 = result.currentPage();
const page2 = result.nextPage();           // array or null
result.setPage(0);                         // jump back
```

### PhotoStack accessor changes

**Before (v0.4.x):**
```rust
// Direct field access
println!("{}", stack.name);
println!("{}", stack.id);
if stack.original.is_present() { }
```

**After (v0.5.0):**
```rust
// Method accessors (fields are behind Arc<RwLock>)
println!("{}", stack.name());
println!("{}", stack.id());
if stack.has_original() { }
```

### Removed methods summary

| Removed | Replacement |
|---------|-------------|
| `mgr.get_stack(id)` | `mgr.query(SearchQuery::new().with_ids(...))` |
| `mgr.get_stack_mut(id)` | Removed — PhotoStack uses `Arc<RwLock>` for shared mutation |
| `mgr.scan()` | `mgr.query()` auto-scans |
| `mgr.all_stacks()` | `mgr.query(None, None, None)` |
| `mgr.stacks()` | `mgr.query(None, None, None)` |
| `mgr.snapshot()` | `mgr.query()` returns QueryResult |
| `mgr.rescan()` | `mgr.invalidate_cache()` then `mgr.query()` |
| `snap.get_page(offset, limit)` | `result.current_page()` / `result.next_page()` |
| C#: `repo.Scan()`, `Search()` | `repo.Query()` |
| TS: `mgr.getStack(id)` | `mgr.query({ stackIds: [id] })` |

---

## v0.3.x → v0.4.0

### ⚠️ Breaking: PhotoStack-Centric API

v0.4.0 is a major architecture redesign. I/O operations move from `Repository`/`StackManager` onto the `PhotoStack` itself via `ImageRef` and `MetadataRef` accessors.

### Reading images

**Before (v0.3.x):**
```rust
// Rust — read via repository
let data = repo.read_image(&stack.original.as_ref().unwrap().path)?;

// TypeScript — read via manager
const buf = mgr.readImage(stack.original);

// C# — read via manager
var bytes = mgr.ReadImage(stack.OriginalPath);
```

**After (v0.4.0):**
```rust
// Rust — read via ImageRef on the stack
let mut reader = stack.original.read()?;

// TypeScript — read via ImageRef
const buf = stack.original.read();

// C# — read via ImageRef
using var stream = stack.Original.Read();
```

### Reading metadata

**Before (v0.3.x):**
```rust
// Rust
let meta = mgr.load_metadata(&stack.id)?;

// TypeScript
const meta = mgr.loadMetadata(stack.id);

// C#
var meta = mgr.LoadMetadata(stack.Id);
```

**After (v0.4.0):**
```rust
// Rust — lazy-loaded MetadataRef
let meta = stack.metadata.read()?;

// TypeScript
const meta = stack.metadata.read();

// C#
var meta = stack.Metadata.Read();
```

### Writing metadata

**Before (v0.3.x):**
```rust
// Rust
mgr.write_metadata(&stack.id, &metadata)?;

// TypeScript
mgr.writeMetadata(stack.id, metadata);

// C#
mgr.WriteMetadata(stack.Id, metadata);
```

**After (v0.4.0):**
```rust
// Rust
stack.metadata.write(&metadata)?;

// TypeScript
stack.metadata.write(metadata);

// C#
stack.Metadata.Write(metadata);
```

### Rotating images

**Before (v0.3.x):**
```rust
// Rust
mgr.rotate_stack(&stack.id, Rotation::Cw90, RotationTarget::All)?;

// TypeScript
mgr.rotateStack(stack.id, 90);

// C#
mgr.RotateStack(stack.Id, 90);
```

**After (v0.4.0):**
```rust
// Rust — rotate specific variant via ImageRef
stack.original.rotate(Rotation::Cw90)?;
stack.back.rotate(Rotation::Cw90)?;

// TypeScript
stack.original.rotate(90);
stack.back.rotate(90);

// C#
stack.Original.Rotate(90);
stack.Back.Rotate(90);
```

### Checking image presence

**Before (v0.3.x):**
```rust
if stack.back.is_some() { /* has back scan */ }
```

**After (v0.4.0):**
```rust
if stack.back.is_present() { /* has back scan */ }
```

### Querying stacks

**Before (v0.3.x):**
```rust
// Rust — returned PaginatedResult directly
let page = mgr.query(&query, Some(&PaginationParams { offset: 0, limit: 20 }));
```

**After (v0.4.0):**
```rust
// Rust — returns ScanSnapshot, paginate from snapshot
let snap = mgr.query(&query);
let page = snap.get_page(0, 20);

// Check staleness with O(1)
if snap.is_stale() {
    let fresh = mgr.query(&query);
}
```

### PhotoStack field changes

| v0.3.x | v0.4.0 |
|--------|--------|
| `stack.original: Option<ImageFile>` | `stack.original: ImageRef` |
| `stack.enhanced: Option<ImageFile>` | `stack.enhanced: ImageRef` |
| `stack.back: Option<ImageFile>` | `stack.back: ImageRef` |
| `stack.metadata: Metadata` (eager) | `stack.metadata: MetadataRef` (lazy) |
| `stack.format()` | Removed |
| `#[derive(Serialize, Deserialize)]` | Removed from PhotoStack |

### Repository trait changes

| Removed | Replacement |
|---------|-------------|
| `repo.load_metadata(id)` | `stack.metadata.read()` |
| `repo.get_stack(id)` | `mgr.get_stack(id)` |
| `repo.read_image(path)` | `stack.original.read()` |
| `repo.write_metadata(id, meta)` | `stack.metadata.write(meta)` |
| `repo.rotate_stack(id, rot, target)` | `stack.original.rotate(rot)` |

| Added | Purpose |
|-------|---------|
| `repo.generation()` | Monotonic counter for staleness detection |
| `repo.set_classifier(c)` | Pluggable image classification via DI |
| `repo.subscribe()` | Event notifications |

### StackManager → SessionManager

`SessionManager` is the new name. `StackManager` remains as a type alias for backward compatibility.

```rust
// Both work:
use photostax_core::stack_manager::SessionManager;
use photostax_core::stack_manager::StackManager; // still available
```

### New: SearchQuery repo filter

```rust
let query = SearchQuery::new()
    .with_text("vacation")
    .with_repo_id("a1b2c3d4"); // filter to specific repo
```

---

## v0.2.2 → v0.3.0

### New: Foreign Repository Support

v0.3.0 introduces the ability for host languages to implement custom repository backends. This enables support for cloud storage providers (OneDrive, Google Drive, Azure Blob Storage) where the host handles I/O while Rust handles scanning logic.

**Rust (core):**
```rust
use photostax_core::backends::foreign::{ForeignRepository, RepositoryProvider};
use photostax_core::scanner::FileEntry;

struct MyCloudProvider { /* ... */ }

impl RepositoryProvider for MyCloudProvider {
    fn location(&self) -> &str { "cloud://my-photos" }
    fn list_entries(&self, _prefix: &str, _recursive: bool) -> io::Result<Vec<FileEntry>> {
        // Return file listings from your cloud storage
        Ok(vec![])
    }
    fn open_read(&self, path: &str) -> io::Result<Box<dyn ReadSeek>> {
        // Open a file for reading from your cloud storage
        todo!()
    }
    fn open_write(&self, path: &str) -> io::Result<Box<dyn Write + Send>> {
        // Open a file for writing to your cloud storage
        todo!()
    }
}

let provider = MyCloudProvider { };
let repo = ForeignRepository::new(Box::new(provider));
let mut mgr = StackManager::new();
mgr.add_repo(Box::new(repo), ScannerProfile::Auto).unwrap();
```

**TypeScript:**
```typescript
import { StackManager, type RepositoryProvider, type FileEntry } from '@photostax/core';

const provider: RepositoryProvider = {
  location: 'cloud://my-photos',
  listEntries: (prefix: string, recursive: boolean): FileEntry[] => {
    return [{ name: 'IMG_001.jpg', folder: '', path: 'cloud://my-photos/IMG_001.jpg', size: 1024 }];
  },
  readFile: (path: string): Buffer => fs.readFileSync(localCachePath),
  writeFile: (path: string, data: Buffer): void => { /* upload */ },
};

const mgr = new StackManager();
mgr.addForeignRepo(provider, { recursive: true });
// query() auto-scans on first call
```

**.NET:**
```csharp
using Photostax;

public class MyCloudProvider : IRepositoryProvider
{
    public string Location => "cloud://my-photos";

    public IReadOnlyList<FileEntry> ListEntries(string prefix, bool recursive)
        => new List<FileEntry>(); // Return cloud file listings

    public Stream OpenRead(string path) => /* download stream */;
    public Stream OpenWrite(string path) => /* upload stream */;
}

using var mgr = new StackManager();
mgr.AddRepo(new MyCloudProvider(), recursive: true);
// Query() auto-scans on first call
```

### No Breaking Changes

This release is additive. All existing APIs continue to work unchanged.

---

## v0.2.0 → v0.2.1

This is a non-breaking release. All existing code continues to work. The main change is a new unified `query()` method that replaces the separate search/paginate pattern.

### New: `StackManager::query()`

**Before (v0.2.0):** Search and pagination were separate operations.

```rust
// v0.2.0 — search then paginate separately
let stacks = manager.stacks();
let filtered = filter_stacks(&stacks, &query);
let page = paginate_stacks(&filtered, &PaginationParams { offset: 0, limit: 20 });
```

**After (v0.2.1):** Use `query()` for search + pagination in one call.

```rust
// v0.2.1 — unified query
let page = manager.query(&query, Some(&PaginationParams { offset: 0, limit: 20 }));

// All stacks (replaces stacks())
let all = manager.query(&SearchQuery::new(), None);

// Iterate pages naturally
if let Some(next) = page.next_page() {
    let page2 = manager.query(&query, Some(&next));
}
```

### Deprecations

- `StackManager::stacks()` — use `query(&SearchQuery::new(), None)` instead

### Binding updates

**TypeScript:**
```typescript
// v0.2.1 — unified query
const page = repo.query({ text: 'birthday' }, 0, 20);
const all = repo.query(); // all stacks
```

**.NET:**
```csharp
// v0.2.1 — unified query
var page = repo.Query("birthday", offset: 0, limit: 20);
var all = repo.Query(); // all stacks
```

### FFI

- New: `photostax_query(repo, query_json, offset, limit)` — unified search + paginate
- New: `folder` field on `FfiPhotoStack`
- Existing FFI functions remain available

---

## v0.1.x → v0.2.0

This guide covers all breaking changes in photostax v0.2.0 and how to update your code.

## Stack IDs are now opaque hashes

**Before (v0.1.x):** Stack IDs were the file stem (e.g., `IMG_001`), which could collide when scanning subfolders with same-named files.

**After (v0.2.0):** Stack IDs are opaque SHA-256 hashes (16 hex chars), globally unique across subfolders.

```rust
// v0.1.x
assert_eq!(stack.id, "IMG_001");

// v0.2.0
assert_eq!(stack.id, "a3f7b2c91e4d8f06"); // opaque hash
println!("Display name: {}", stack.name);   // "IMG_001"
println!("Subfolder: {}", stack.folder);     // "2024/January"
```

**What to change:**
- Use `stack.name` for display purposes (the old human-readable stem).
- Use `stack.folder` for subfolder context.
- Any persisted stack IDs (databases, config files) must be regenerated by re-scanning — the old file-stem IDs are no longer produced.
- Do not parse or depend on the format of stack IDs; treat them as opaque strings.

## PhotoStack image fields changed from PathBuf to ImageFile

**Before (v0.1.x):** Image fields were `Option<PathBuf>`.

**After (v0.2.0):** Image fields are `Option<ImageFile>`, where `ImageFile` contains `path`, `content_hash`, and `size`.

```rust
// v0.1.x
if let Some(ref path) = stack.original {
    println!("Original: {}", path.display());
}

// v0.2.0
if let Some(ref file) = stack.original {
    println!("Original: {}", file.path);       // String, not PathBuf
    println!("Hash: {}", file.content_hash());  // lazy SHA-256
    println!("Size: {}", file.size);
}
```

**What to change:**
- Replace `stack.original` (and `enhanced`, `back`) access with `stack.original.as_ref().map(|f| &f.path)` when you only need the path.
- `ImageFile.path` is now a `String` (not `PathBuf`) to support cloud URIs — use it directly instead of calling `.display()`.
- If you were pattern-matching on `Option<PathBuf>`, update to `Option<ImageFile>`.

## read_image returns a stream, not a byte array

**Before (v0.1.x):**

```rust
let bytes: Vec<u8> = repo.read_image(Path::new("photo.jpg"))?;
```

**After (v0.2.0):**

```rust
let mut reader: Box<dyn ReadSeek> = repo.read_image("photo.jpg")?;
// Read into a buffer as needed
let mut buf = Vec::new();
reader.read_to_end(&mut buf)?;
```

**What to change:**
- The parameter changed from `&Path` to `&str`.
- The return type changed from `Vec<u8>` to `Box<dyn ReadSeek>` — this keeps memory bounded for large TIFFs.
- If you need all bytes at once, call `read_to_end()` on the returned reader.
- The stream is seekable, so you can use it with image decoders that require `Seek`.

## Repository trait now requires FileAccess

**Before (v0.1.x):** The `Repository` trait stood alone.

**After (v0.2.0):** `Repository` has a `FileAccess` supertrait.

```rust
// v0.2.0 — custom backend must implement both traits
impl FileAccess for MyBackend {
    fn open_read(&self, path: &str) -> Result<Box<dyn ReadSeek>> { /* ... */ }
    fn open_write(&self, path: &str) -> Result<Box<dyn Write>> { /* ... */ }
    // hash_file() has a default impl using open_read()
}

impl Repository for MyBackend {
    // ... existing methods ...
}
```

**What to change:**
- If you have a custom `Repository` implementation, also implement `FileAccess`.
- `open_read()` and `open_write()` are the two required methods.
- `hash_file()` has a default implementation that uses `open_read()`, so you only need to override it if you have a more efficient way to compute hashes.

## Repository trait gained location() and id()

**After (v0.2.0):**

```rust
impl Repository for MyBackend {
    fn location(&self) -> String {
        "mycloud://bucket/photos".to_string()  // canonical URI
    }

    fn id(&self) -> String {
        "a1b2c3d4".to_string()  // short hash identifier
    }

    // ... existing methods ...
}
```

**What to change:**
- Custom backends must implement `location()` (returns a canonical URI, e.g., `file:///C:/photos`) and `id()` (returns a short hash identifier).
- `LocalRepository` provides these automatically based on the filesystem path.

## StackManager is the new primary API

**Before (v0.1.x):** You created a `LocalRepository` and called scan/get_stack/rotate directly.

**After (v0.2.0):** Use `StackManager` as the primary API. It provides a unified cache with O(1) lookups and multi-repo support.

```rust
// v0.1.x
let repo = LocalRepository::new("/photos");
let stacks = repo.scan()?;
let stack = stacks.iter().find(|s| s.id == "IMG_001");

// v0.2.0 — single repo
let repo = LocalRepository::new("/photos");
let profile = ScannerProfile::Auto;
let manager = StackManager::single(repo, profile);
manager.scan()?;
let stack = manager.get_stack("a3f7b2c91e4d8f06");

// v0.2.0 — multiple repos
let mut manager = StackManager::new();
manager.add_repo(repo1)?;
manager.add_repo(repo2)?;
manager.scan()?;
```

**What to change:**
- Wrap your repository in a `StackManager` via `StackManager::single(repo, profile)`.
- For multi-repo setups, use `StackManager::new()` then `add_repo()` for each repository.
- **Binding layers** (FFI, TypeScript, .NET): The existing constructors (`PhotostaxRepository`, etc.) already wrap `StackManager` internally — no changes needed in binding consumer code unless you want multi-repo support.

## Filesystem watching (new feature)

v0.2.0 adds reactive filesystem watching:

```rust
// Start watching for changes
let rx = repo.watch()?;

// Process events
while let Ok(event) = rx.recv() {
    match manager.apply_event(event) {
        CacheEvent::StackAdded(id) => println!("New stack: {id}"),
        CacheEvent::StackUpdated(id) => println!("Updated: {id}"),
        CacheEvent::StackRemoved(id) => println!("Removed: {id}"),
    }
}
```

No migration needed — this is purely additive.

## Content hashing (new feature)

v0.2.0 adds content-based hashing for duplicate detection:

```rust
// Per-file hash (lazy, computed on first access)
let hash = image_file.content_hash();  // 16 hex chars, SHA-256 prefix

// Per-stack Merkle hash (combines all file hashes)
let stack_hash = stack.content_hash();

// Zero extra I/O when using HashingReader (hashes computed during normal reads)
let reader = HashingReader::new(file);
// ... read the file normally ...
let hash = reader.finalize();  // hash available without extra read pass
```

No migration needed — this is purely additive.

## Summary of type changes

| v0.1.x | v0.2.0 |
|--------|--------|
| `stack.id` = `"IMG_001"` (file stem) | `stack.id` = `"a3f7b2c91e4d8f06"` (opaque hash) |
| `stack.original: Option<PathBuf>` | `stack.original: Option<ImageFile>` |
| `ImageFile` — N/A | `ImageFile { path: String, content_hash, size }` |
| `repo.read_image(&Path) → Vec<u8>` | `repo.read_image(&str) → Box<dyn ReadSeek>` |
| `Repository` trait (standalone) | `Repository: FileAccess` (supertrait) |
| Direct repo calls | `StackManager` wraps repos |
| No `location()` / `id()` | Required on `Repository` |

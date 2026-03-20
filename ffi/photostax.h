#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

/**
 * Opaque handle to a [`StackManager`].
 *
 * This type is opaque to C code and should only be manipulated through
 * the FFI functions. Create with [`photostax_repo_open`] and free with
 * [`photostax_repo_free`].
 *
 * Internally uses [`RefCell`] because `StackManager` mutation methods
 * (`scan`, `load_metadata`, `rotate_stack`, etc.) require `&mut self`,
 * while the FFI functions receive `*const PhotostaxRepo`.
 *
 * [`StackManager`]: photostax_core::stack_manager::StackManager
 * [`photostax_repo_open`]: crate::repository::photostax_repo_open
 * [`photostax_repo_free`]: crate::repository::photostax_repo_free
 */
typedef struct PhotostaxRepo PhotostaxRepo;

/**
 * Opaque handle to a scan snapshot.
 */
typedef struct PhotostaxSnapshot PhotostaxSnapshot;

/**
 * Result type for FFI calls.
 *
 * On success, `success` is true and `error_message` is null.
 * On failure, `success` is false and `error_message` contains the error.
 *
 * # Memory Ownership
 *
 * - If `error_message` is non-null, caller must free it with [`photostax_string_free`]
 *
 * [`photostax_string_free`]: crate::repository::photostax_string_free
 */
typedef struct FfiResult {
  /**
   * True if the operation succeeded.
   */
  bool success;
  /**
   * Error message (null on success, must be freed on failure).
   */
  char *error_message;
} FfiResult;

/**
 * A photo stack returned across FFI.
 *
 * All string pointers are owned by this struct and must be freed by calling
 * [`photostax_stack_free`]. Null pointers indicate absent values.
 *
 * # Memory Ownership
 *
 * - Caller receives ownership of the entire struct
 * - Call [`photostax_stack_free`] to release all memory
 * - Do not free individual string fields separately
 *
 * [`photostax_stack_free`]: crate::repository::photostax_stack_free
 */
typedef struct FfiPhotoStack {
  /**
   * Stack identifier (never null). This is an opaque hash.
   */
  char *id;
  /**
   * Human-readable stack name, typically the file stem (never null).
   */
  char *name;
  /**
   * Subfolder name within the repository (null if root level).
   */
  char *folder;
  /**
   * Path to original image (null if absent).
   */
  char *original;
  /**
   * Path to enhanced image (null if absent).
   */
  char *enhanced;
  /**
   * Path to back image (null if absent).
   */
  char *back;
  /**
   * JSON-serialized metadata (never null, may be "{}").
   */
  char *metadata_json;
} FfiPhotoStack;

/**
 * Array of photo stacks.
 *
 * # Memory Ownership
 *
 * - Caller receives ownership of the entire array
 * - Call [`photostax_stack_array_free`] to release all memory
 * - Do not free individual stacks separately after freeing the array
 *
 * [`photostax_stack_array_free`]: crate::repository::photostax_stack_array_free
 */
typedef struct FfiPhotoStackArray {
  /**
   * Pointer to array of stacks (null if len == 0).
   */
  struct FfiPhotoStack *data;
  /**
   * Number of stacks in the array.
   */
  uintptr_t len;
} FfiPhotoStackArray;

/**
 * C-compatible progress callback function pointer.
 *
 * Parameters:
 * - `phase`: 0 = Scanning, 1 = Classifying, 2 = Complete
 * - `current`: items processed so far in current phase
 * - `total`: total items in current phase
 * - `user_data`: opaque pointer passed through from the caller
 */
typedef void (*ScanProgressFn)(int32_t phase, uintptr_t current, uintptr_t total, void *user_data);

/**
 * Paginated result of photo stacks returned across FFI.
 *
 * Contains a page of stacks along with pagination metadata needed
 * for rendering pagination controls in a web UI.
 *
 * # Memory Ownership
 *
 * - Caller receives ownership of the entire result
 * - Call [`photostax_paginated_result_free`] to release all memory
 *
 * [`photostax_paginated_result_free`]: crate::repository::photostax_paginated_result_free
 */
typedef struct FfiPaginatedResult {
  /**
   * Pointer to array of stacks in this page (null if len == 0).
   */
  struct FfiPhotoStack *data;
  /**
   * Number of stacks in this page.
   */
  uintptr_t len;
  /**
   * Total number of stacks across all pages (before pagination).
   */
  uintptr_t total_count;
  /**
   * The offset used for this page.
   */
  uintptr_t offset;
  /**
   * The page size limit used for this page.
   */
  uintptr_t limit;
  /**
   * Whether there are more items beyond this page.
   */
  bool has_more;
} FfiPaginatedResult;

/**
 * A file entry returned by the foreign list_entries callback.
 *
 * All string pointers must remain valid until the `free_entries` callback
 * is called. The Rust side copies these strings immediately.
 */
typedef struct FfiFileEntry {
  /**
   * File name including extension (e.g., "IMG_001_a.jpg"). Never null.
   */
  const char *name;
  /**
   * Relative folder path using forward slashes (empty string for root). Never null.
   */
  const char *folder;
  /**
   * Full path or URI to the file. Never null.
   */
  const char *path;
  /**
   * File size in bytes.
   */
  uint64_t size;
} FfiFileEntry;

/**
 * Result of a list_entries callback.
 */
typedef struct FfiFileEntryArray {
  /**
   * Pointer to array of entries (null if len == 0).
   */
  const struct FfiFileEntry *data;
  /**
   * Number of entries.
   */
  uintptr_t len;
  /**
   * Non-zero indicates an error (entries are invalid).
   */
  int32_t error;
} FfiFileEntryArray;

/**
 * Result of an open_read or open_write callback.
 */
typedef struct FfiStreamHandle {
  /**
   * Opaque stream handle. Zero indicates failure.
   */
  uint64_t handle;
  /**
   * Non-zero indicates an error.
   */
  int32_t error;
} FfiStreamHandle;

/**
 * Result of a read callback.
 */
typedef struct FfiReadResult {
  /**
   * Number of bytes actually read.
   */
  uintptr_t bytes_read;
  /**
   * Non-zero indicates an error.
   */
  int32_t error;
} FfiReadResult;

/**
 * Result of a seek callback.
 */
typedef struct FfiSeekResult {
  /**
   * New position after seeking.
   */
  uint64_t position;
  /**
   * Non-zero indicates an error.
   */
  int32_t error;
} FfiSeekResult;

/**
 * Result of a write callback.
 */
typedef struct FfiWriteResult {
  /**
   * Number of bytes actually written.
   */
  uintptr_t bytes_written;
  /**
   * Non-zero indicates an error.
   */
  int32_t error;
} FfiWriteResult;

/**
 * Callback function pointers for a foreign repository provider.
 *
 * The host language fills this struct with function pointers that implement
 * file I/O operations. The `ctx` pointer is passed through to every callback
 * and can be used to maintain state in the host language (e.g., a managed
 * object reference, a COM pointer, or a JavaScript reference).
 *
 * # Lifetime
 *
 * The `ctx` pointer and all callback functions must remain valid for the
 * lifetime of the repository (until the `StackManager` handle is freed).
 *
 * # Thread Safety
 *
 * Callbacks may be invoked from any Rust thread. Host implementations must
 * be thread-safe or serialize access internally.
 */
typedef struct FfiProviderCallbacks {
  /**
   * Opaque context pointer passed to every callback.
   */
  void *ctx;
  /**
   * Location URI for this repository (e.g., "onedrive://user/Photos").
   * Must be a valid null-terminated UTF-8 string. Remains valid for
   * the lifetime of the provider.
   */
  const char *location;
  /**
   * List file entries under a prefix.
   *
   * - `ctx`: the context pointer
   * - `prefix`: null-terminated UTF-8 folder prefix (empty string for root)
   * - `recursive`: whether to recurse into subdirectories
   *
   * Returns an `FfiFileEntryArray`. The caller (Rust) copies entries
   * immediately, then calls `free_entries` so the host can release memory.
   */
  struct FfiFileEntryArray (*list_entries)(void *ctx, const char *prefix, bool recursive);
  /**
   * Free an entry array previously returned by `list_entries`.
   */
  void (*free_entries)(void *ctx, struct FfiFileEntryArray entries);
  /**
   * Open a file for reading.
   *
   * Returns an `FfiStreamHandle` with a non-zero handle on success.
   */
  struct FfiStreamHandle (*open_read)(void *ctx, const char *path);
  /**
   * Read bytes from a stream.
   *
   * - `handle`: stream handle from `open_read`
   * - `buf`: buffer to read into
   * - `len`: maximum number of bytes to read
   */
  struct FfiReadResult (*read)(void *ctx, uint64_t handle, uint8_t *buf, uintptr_t len);
  /**
   * Seek within a stream.
   *
   * - `handle`: stream handle from `open_read`
   * - `offset`: byte offset
   * - `whence`: 0 = from start, 1 = from current, 2 = from end
   */
  struct FfiSeekResult (*seek)(void *ctx, uint64_t handle, int64_t offset, int32_t whence);
  /**
   * Close a read stream.
   */
  void (*close_read)(void *ctx, uint64_t handle);
  /**
   * Open a file for writing.
   *
   * Returns an `FfiStreamHandle` with a non-zero handle on success.
   */
  struct FfiStreamHandle (*open_write)(void *ctx, const char *path);
  /**
   * Write bytes to a stream.
   *
   * - `handle`: stream handle from `open_write`
   * - `buf`: bytes to write
   * - `len`: number of bytes to write
   */
  struct FfiWriteResult (*write)(void *ctx, uint64_t handle, const uint8_t *buf, uintptr_t len);
  /**
   * Close a write stream.
   */
  void (*close_write)(void *ctx, uint64_t handle);
} FfiProviderCallbacks;

/**
 * Staleness information returned by [`photostax_snapshot_check_status`].
 */
typedef struct FfiSnapshotStatus {
  /**
   * `true` when the filesystem no longer matches the snapshot.
   */
  bool is_stale;
  /**
   * Number of stacks captured in the snapshot.
   */
  uintptr_t snapshot_count;
  /**
   * Number of stacks currently on disk.
   */
  uintptr_t current_count;
  /**
   * New stacks on disk that were not in the snapshot.
   */
  uintptr_t added;
  /**
   * Snapshot stacks no longer present on disk.
   */
  uintptr_t removed;
} FfiSnapshotStatus;

/**
 * Create a new local repository handle.
 *
 * # Safety
 * The `path` must be a valid null-terminated UTF-8 string.
 * The returned pointer must be freed with `photostax_repository_free`.
 */
LocalRepository *photostax_repository_new(const char *path);

/**
 * Free a repository handle.
 *
 * # Safety
 * The `repo` must be a valid pointer returned by `photostax_repository_new`,
 * or null (in which case this is a no-op).
 */
void photostax_repository_free(LocalRepository *repo);

/**
 * Scan the repository and return the count of photo stacks found.
 * Returns -1 on error.
 *
 * # Safety
 * The `repo` must be a valid pointer returned by `photostax_repository_new`.
 */
int32_t photostax_repository_scan_count(const LocalRepository *repo);

/**
 * Get the version string of the library.
 *
 * # Safety
 * The returned string is statically allocated and must not be freed.
 */
const char *photostax_version(void);

/**
 * Get metadata for a stack as a JSON string.
 *
 * Returns a JSON object with `exif_tags`, `xmp_tags`, and `custom_tags` fields.
 *
 * # Safety
 *
 * - `repo` must be a valid pointer from [`photostax_repo_open`]
 * - `stack_id` must be a valid null-terminated UTF-8 string
 * - Returns null on error
 * - Caller owns the returned string and must call [`photostax_string_free`]
 *
 * [`photostax_repo_open`]: crate::repository::photostax_repo_open
 * [`photostax_string_free`]: crate::repository::photostax_string_free
 */
char *photostax_get_metadata(const struct PhotostaxRepo *repo, const char *stack_id);

/**
 * Get a specific EXIF tag value.
 *
 * # Safety
 *
 * - `repo` must be a valid pointer from [`photostax_repo_open`]
 * - `stack_id` and `tag_name` must be valid null-terminated UTF-8 strings
 * - Returns null if tag not found or on error
 * - Caller owns the returned string and must call [`photostax_string_free`]
 *
 * [`photostax_repo_open`]: crate::repository::photostax_repo_open
 * [`photostax_string_free`]: crate::repository::photostax_string_free
 */
char *photostax_get_exif_tag(const struct PhotostaxRepo *repo,
                             const char *stack_id,
                             const char *tag_name);

/**
 * Get a specific custom tag value as JSON.
 *
 * # Safety
 *
 * - `repo` must be a valid pointer from [`photostax_repo_open`]
 * - `stack_id` and `tag_name` must be valid null-terminated UTF-8 strings
 * - Returns null if tag not found or on error
 * - Caller owns the returned string and must call [`photostax_string_free`]
 *
 * [`photostax_repo_open`]: crate::repository::photostax_repo_open
 * [`photostax_string_free`]: crate::repository::photostax_string_free
 */
char *photostax_get_custom_tag(const struct PhotostaxRepo *repo,
                               const char *stack_id,
                               const char *tag_name);

/**
 * Set a custom tag value.
 *
 * # Safety
 *
 * - `repo` must be a valid pointer from [`photostax_repo_open`]
 * - `stack_id`, `tag_name`, and `value_json` must be valid null-terminated UTF-8 strings
 * - `value_json` must be valid JSON
 *
 * [`photostax_repo_open`]: crate::repository::photostax_repo_open
 */
struct FfiResult photostax_set_custom_tag(const struct PhotostaxRepo *repo,
                                          const char *stack_id,
                                          const char *tag_name,
                                          const char *value_json);

/**
 * Create a new repository from a directory path.
 *
 * # Safety
 *
 * - `path` must be a valid null-terminated UTF-8 string
 * - Returns null if `path` is null or invalid
 * - Caller owns the returned pointer and must call [`photostax_repo_free`]
 */
struct PhotostaxRepo *photostax_repo_open(const char *path);

/**
 * Create a new repository with recursive subdirectory scanning.
 *
 * When `recursive` is true, the scanner will descend into all subdirectories.
 * This is required when the photo library uses FastFoto's folder-based
 * organisation (e.g. `1984_Mexico/`, `SteveJones/`).
 *
 * # Safety
 *
 * - `path` must be a valid null-terminated UTF-8 string
 * - Returns null if `path` is null or invalid
 * - Caller owns the returned pointer and must call [`photostax_repo_free`]
 */
struct PhotostaxRepo *photostax_repo_open_recursive(const char *path, bool recursive);

/**
 * Free a repository handle.
 *
 * # Safety
 *
 * - `repo` must be a pointer returned by [`photostax_repo_open`], or null
 * - After calling, `repo` is invalid and must not be used
 */
void photostax_repo_free(struct PhotostaxRepo *repo);

/**
 * Scan the repository and return all photo stacks.
 *
 * # Safety
 *
 * - `repo` must be a valid pointer from [`photostax_repo_open`]
 * - Returns empty array if `repo` is null or scan fails
 * - Caller owns the returned array and must call [`photostax_stack_array_free`]
 */
struct FfiPhotoStackArray photostax_repo_scan(const struct PhotostaxRepo *repo);

/**
 * Scan with a [`ScannerProfile`] and optional progress callback.
 *
 * # Parameters
 *
 * - `repo` — valid pointer from [`photostax_repo_open`]
 * - `profile` — scanner profile (0=Auto, 1=EnhancedAndBack, 2=EnhancedOnly, 3=OriginalOnly)
 * - `callback` — optional progress callback invoked per-step (may be null)
 * - `user_data` — opaque pointer forwarded to the callback (may be null)
 *
 * # Safety
 *
 * - `repo` must be a valid pointer from [`photostax_repo_open`]
 * - `callback` and `user_data` must be valid for the duration of the call
 * - Caller owns the returned array and must call [`photostax_stack_array_free`]
 */
struct FfiPhotoStackArray photostax_repo_scan_with_progress(const struct PhotostaxRepo *repo,
                                                            int32_t profile,
                                                            ScanProgressFn callback,
                                                            void *user_data);

/**
 *
 * # Safety
 *
 * - `repo` must be a valid pointer from [`photostax_repo_open`]
 * - `id` must be a valid null-terminated UTF-8 string
 * - Returns null if not found or on error
 * - Caller owns the returned pointer and must call [`photostax_stack_free`]
 */
struct FfiPhotoStack *photostax_repo_get_stack(const struct PhotostaxRepo *repo, const char *id);

/**
 * Read image bytes from a stack's image variant.
 *
 * The `stack_id` identifies the stack and `variant` selects which image:
 * - `0` = original
 * - `1` = enhanced
 * - `2` = back
 *
 * # Safety
 *
 * - `repo` must be a valid pointer from [`photostax_repo_open`]
 * - `stack_id` must be a valid null-terminated UTF-8 string
 * - `out_data` must be a valid pointer to receive the data pointer
 * - `out_len` must be a valid pointer to receive the data length
 * - On success, caller owns `*out_data` and must call [`photostax_bytes_free`]
 */
struct FfiResult photostax_read_image(const struct PhotostaxRepo *repo,
                                      const char *stack_id,
                                      uint8_t **out_data,
                                      uintptr_t *out_len);

/**
 * Write metadata to a stack.
 *
 * # Safety
 *
 * - `repo` must be a valid pointer from [`photostax_repo_open`]
 * - `stack_id` must be a valid null-terminated UTF-8 string
 * - `metadata_json` must be a valid null-terminated JSON string
 */
struct FfiResult photostax_write_metadata(const struct PhotostaxRepo *repo,
                                          const char *stack_id,
                                          const char *metadata_json);

/**
 * Rotate images in a photo stack by the given number of degrees.
 *
 * Accepted `degrees` values: `90`, `-90`, `180`, `-180`, `270`.
 * The `target` parameter controls which images are rotated:
 * - `0` = all images (original + enhanced + back)
 * - `1` = front only (original + enhanced)
 * - `2` = back only
 *
 * Returns the updated stack with refreshed metadata on success.
 *
 * # Safety
 *
 * - `repo` must be a valid pointer from [`photostax_repo_open`]
 * - `stack_id` must be a valid null-terminated UTF-8 string
 * - On success, caller owns the returned pointer and must call [`photostax_stack_free`]
 * - Returns null on error; inspect the result for the error message
 */
struct FfiPhotoStack *photostax_rotate_stack(const struct PhotostaxRepo *repo,
                                             const char *stack_id,
                                             int32_t degrees,
                                             int32_t target);

/**
 * Scan the repository and return a paginated result.
 *
 * When `load_metadata` is true, EXIF/XMP/sidecar metadata is loaded for each
 * stack in the returned page. When false, stacks contain only paths and
 * folder-derived metadata (faster for large repositories).
 *
 * # Safety
 *
 * - `repo` must be a valid pointer from [`photostax_repo_open`]
 * - Returns empty result if `repo` is null or scan fails
 * - Caller owns the returned result and must call [`photostax_paginated_result_free`]
 */
struct FfiPaginatedResult photostax_repo_scan_paginated(const struct PhotostaxRepo *repo,
                                                        uintptr_t offset,
                                                        uintptr_t limit,
                                                        bool load_metadata);

/**
 * Unified query: search + paginate the cache in a single call.
 *
 * This is the preferred way to retrieve stacks. Combines filtering and
 * pagination into one operation without intermediate allocations.
 *
 * # Parameters
 *
 * - `repo` — repository handle from [`photostax_repo_open`]
 * - `query_json` — JSON-serialized [`SearchQuery`], or null to match all stacks
 * - `offset` — number of items to skip (0-based)
 * - `limit` — maximum items to return; 0 means return all matching stacks
 *
 * # Safety
 *
 * - `repo` must be a valid pointer from [`photostax_repo_open`]
 * - `query_json`, if non-null, must be a valid null-terminated UTF-8 string
 * - Caller owns the returned result and must call [`photostax_paginated_result_free`]
 */
struct FfiPaginatedResult photostax_query(const struct PhotostaxRepo *repo,
                                          const char *query_json,
                                          uintptr_t offset,
                                          uintptr_t limit);

/**
 * Create an empty [`StackManager`] with no repositories.
 *
 * Use [`photostax_manager_add_repo`] to register repositories, then
 * [`photostax_repo_scan`] (or any other repo function) to operate on them.
 * The returned handle is compatible with all existing `photostax_repo_*`
 * functions.
 *
 * # Safety
 *
 * - Caller owns the returned handle and must free it with [`photostax_repo_free`]
 * - Returns null on internal error
 */
struct PhotostaxRepo *photostax_manager_new(void);

/**
 * Add a repository to an existing [`StackManager`].
 *
 * The `path` is a filesystem directory. Set `recursive` to scan subdirectories.
 * `profile` controls scanner classification: 0 = Auto, 1 = EnhancedOnly,
 * 2 = EnhancedAndBack, 3 = Skip.
 *
 * All subsequent scan/query/get operations on this handle will include stacks
 * from every registered repository.
 *
 * # Safety
 *
 * - `mgr` must be a valid pointer from [`photostax_manager_new`] or
 *   [`photostax_repo_open`]
 * - `path` must be a valid null-terminated UTF-8 string
 * - Returns an [`FfiResult`] indicating success or failure
 */
struct FfiResult photostax_manager_add_repo(struct PhotostaxRepo *mgr,
                                            const char *path,
                                            bool recursive,
                                            int32_t profile);

/**
 * Return the number of repositories registered with a [`StackManager`].
 *
 * # Safety
 *
 * - `mgr` must be a valid pointer from [`photostax_manager_new`] or
 *   [`photostax_repo_open`]
 * - Returns 0 if `mgr` is null
 */
uintptr_t photostax_manager_repo_count(const struct PhotostaxRepo *mgr);

/**
 * Add a foreign (host-language-provided) repository to a [`StackManager`].
 *
 * The host language provides I/O callbacks via [`FfiProviderCallbacks`].
 * The Rust core handles scanning, naming conventions, and metadata operations.
 *
 * `recursive` and `profile` control scanning behaviour (same as
 * [`photostax_manager_add_repo`]).
 *
 * # Safety
 *
 * - `mgr` must be a valid pointer from [`photostax_manager_new`] or
 *   [`photostax_repo_open`]
 * - `callbacks` must contain valid function pointers and a valid `ctx`
 * - The `ctx` pointer and all callbacks must remain valid for the lifetime
 *   of the manager handle
 * - `callbacks.location` must be a valid null-terminated UTF-8 string
 */
struct FfiResult photostax_manager_add_foreign_repo(struct PhotostaxRepo *mgr,
                                                    struct FfiProviderCallbacks callbacks,
                                                    bool recursive,
                                                    int32_t profile);

/**
 * Load full metadata (EXIF, XMP, sidecar) for a specific stack and return it
 * as a JSON string.
 *
 * This is the lazy-loading counterpart: call after [`photostax_repo_scan`] to
 * retrieve a single stack's metadata on demand.
 *
 * # Safety
 *
 * - `repo` must be a valid pointer from [`photostax_repo_open`]
 * - `stack_id` must be a valid null-terminated UTF-8 string
 * - Returns null on error or if the stack is not found
 * - Caller owns the returned string and must call [`photostax_string_free`]
 */
char *photostax_stack_load_metadata(const struct PhotostaxRepo *repo, const char *stack_id);

/**
 * Free a paginated result.
 *
 * # Safety
 *
 * - `result` must have been returned by a paginated FFI function
 * - After calling, all pointers within `result` are invalid
 */
void photostax_paginated_result_free(struct FfiPaginatedResult result);

/**
 * Free a photo stack array.
 *
 * # Safety
 *
 * - `array` must have been returned by an FFI function (e.g., [`photostax_repo_scan`])
 * - After calling, all pointers within `array` are invalid
 */
void photostax_stack_array_free(struct FfiPhotoStackArray array);

/**
 * Free a single photo stack.
 *
 * # Safety
 *
 * - `stack` must have been returned by [`photostax_repo_get_stack`]
 * - After calling, `stack` and all its strings are invalid
 */
void photostax_stack_free(struct FfiPhotoStack *stack);

/**
 * Free a string allocated by photostax.
 *
 * # Safety
 *
 * - `s` must have been allocated by a photostax FFI function, or be null
 * - After calling, `s` is invalid and must not be used
 */
void photostax_string_free(char *s);

/**
 * Free a byte buffer allocated by photostax.
 *
 * # Safety
 *
 * - `data` and `len` must have been returned by a photostax FFI function
 * - After calling, `data` is invalid and must not be used
 */
void photostax_bytes_free(uint8_t *data, uintptr_t len);

/**
 * Search/filter stacks. `query_json` is a JSON-serialized SearchQuery.
 *
 * # Query JSON Format
 *
 * ```json
 * {
 *   "exif_filters": [["Make", "EPSON"], ["Model", "FastFoto"]],
 *   "custom_filters": [["album", "Family"]],
 *   "text_query": "birthday",
 *   "has_back": true,
 *   "has_enhanced": null
 * }
 * ```
 *
 * All fields are optional. An empty object `{}` matches all stacks.
 *
 * # Safety
 *
 * - `repo` must be a valid pointer from [`photostax_repo_open`]
 * - `query_json` must be a valid null-terminated JSON string
 * - Returns empty array on null pointers or errors
 * - Caller owns the returned array and must call [`photostax_stack_array_free`]
 *
 * [`photostax_repo_open`]: crate::repository::photostax_repo_open
 * [`photostax_stack_array_free`]: crate::repository::photostax_stack_array_free
 */
struct FfiPhotoStackArray photostax_search(const struct PhotostaxRepo *repo,
                                           const char *query_json);

/**
 * Search/filter stacks with pagination. `query_json` is a JSON-serialized SearchQuery.
 *
 * # Query JSON Format
 *
 * Same as [`photostax_search`], but results are paginated.
 *
 * # Safety
 *
 * - `repo` must be a valid pointer from [`photostax_repo_open`]
 * - `query_json` must be a valid null-terminated JSON string
 * - Returns empty result on null pointers or errors
 * - Caller owns the returned result and must call [`photostax_paginated_result_free`]
 *
 * [`photostax_repo_open`]: crate::repository::photostax_repo_open
 * [`photostax_paginated_result_free`]: crate::repository::photostax_paginated_result_free
 */
struct FfiPaginatedResult photostax_search_paginated(const struct PhotostaxRepo *repo,
                                                     const char *query_json,
                                                     uintptr_t offset,
                                                     uintptr_t limit);

/**
 * Create a snapshot from a lightweight scan (no file-based metadata).
 *
 * # Safety
 *
 * - `repo` must be a valid pointer from [`photostax_repo_open`]
 * - Returns null on error
 * - Caller owns the returned pointer and must call [`photostax_snapshot_free`]
 *
 * [`photostax_repo_open`]: crate::repository::photostax_repo_open
 */
struct PhotostaxSnapshot *photostax_create_snapshot(const struct PhotostaxRepo *repo,
                                                    bool load_metadata);

/**
 * Create a snapshot with a scanner profile and optional progress callback.
 *
 * Combines scanning, classification, optional metadata loading, and
 * snapshot creation in a single pass — no redundant re-scanning.
 *
 * # Parameters
 *
 * - `profile` — scanner profile (0=Auto, 1=EnhancedAndBack, 2=EnhancedOnly, 3=OriginalOnly)
 * - `load_metadata` — if true, EXIF/XMP/sidecar is loaded for every stack
 * - `callback` — optional progress callback (may be null)
 * - `user_data` — opaque pointer forwarded to callback (may be null)
 *
 * # Safety
 *
 * - `repo` must be a valid pointer from [`photostax_repo_open`]
 * - `callback` and `user_data` must be valid for the duration of the call
 * - Returns null on error
 * - Caller owns the returned pointer and must call [`photostax_snapshot_free`]
 */
struct PhotostaxSnapshot *photostax_create_snapshot_with_progress(const struct PhotostaxRepo *repo,
                                                                  int32_t profile,
                                                                  bool load_metadata,
                                                                  ScanProgressFn callback,
                                                                  void *user_data);

/**
 * Get the total number of stacks in the snapshot.
 *
 * # Safety
 *
 * - `snapshot` must be a valid pointer from [`photostax_create_snapshot`]
 * - Returns 0 on null pointer
 */
uintptr_t photostax_snapshot_total_count(const struct PhotostaxSnapshot *snapshot);

/**
 * Get a page of stacks from the snapshot.
 *
 * This is a pure in-memory operation — it never accesses the filesystem
 * and always returns a consistent page.
 *
 * # Safety
 *
 * - `snapshot` must be a valid pointer from [`photostax_create_snapshot`]
 * - Returns empty result on null pointer
 * - Caller owns the returned result and must call [`photostax_paginated_result_free`]
 *
 * [`photostax_paginated_result_free`]: crate::repository::photostax_paginated_result_free
 */
struct FfiPaginatedResult photostax_snapshot_get_page(const struct PhotostaxSnapshot *snapshot,
                                                      uintptr_t offset,
                                                      uintptr_t limit);

/**
 * Check whether a snapshot is still current.
 *
 * Performs a fast re-scan (no metadata I/O) and compares against the
 * snapshot to report added/removed stacks.
 *
 * # Safety
 *
 * - `repo` must be a valid pointer from [`photostax_repo_open`]
 * - `snapshot` must be a valid pointer from [`photostax_create_snapshot`]
 * - Returns a zeroed status with `is_stale = true` on error
 *
 * [`photostax_repo_open`]: crate::repository::photostax_repo_open
 */
struct FfiSnapshotStatus photostax_snapshot_check_status(const struct PhotostaxRepo *repo,
                                                         const struct PhotostaxSnapshot *snapshot);

/**
 * Create a new snapshot by filtering an existing one.
 *
 * The `query_json` format is the same as [`photostax_search`].
 * Returns a new snapshot containing only matching stacks.
 *
 * # Safety
 *
 * - `snapshot` must be a valid pointer from [`photostax_create_snapshot`]
 * - `query_json` must be a valid null-terminated JSON string
 * - Returns null on error
 * - Caller owns the returned pointer and must call [`photostax_snapshot_free`]
 *
 * [`photostax_search`]: crate::search::photostax_search
 */
struct PhotostaxSnapshot *photostax_snapshot_filter(const struct PhotostaxSnapshot *snapshot,
                                                    const char *query_json);

/**
 * Free a snapshot handle.
 *
 * # Safety
 *
 * - `snapshot` must be a valid pointer from [`photostax_create_snapshot`]
 *   or [`photostax_snapshot_filter`], or null (no-op).
 */
void photostax_snapshot_free(struct PhotostaxSnapshot *snapshot);

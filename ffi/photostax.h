#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

/**
 * Opaque handle to a LocalRepository.
 *
 * This type is opaque to C code and should only be manipulated through
 * the FFI functions. Create with [`photostax_repo_open`] and free with
 * [`photostax_repo_free`].
 *
 * [`photostax_repo_open`]: crate::repository::photostax_repo_open
 * [`photostax_repo_free`]: crate::repository::photostax_repo_free
 */
typedef struct PhotostaxRepo PhotostaxRepo;

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
   * Stack identifier (never null).
   */
  char *id;
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
 * Get a single stack by ID.
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
 * Read image bytes.
 *
 * # Safety
 *
 * - `repo` must be a valid pointer from [`photostax_repo_open`]
 * - `path` must be a valid null-terminated UTF-8 string (file path)
 * - `out_data` must be a valid pointer to receive the data pointer
 * - `out_len` must be a valid pointer to receive the data length
 * - On success, caller owns `*out_data` and must call [`photostax_bytes_free`]
 */
struct FfiResult photostax_read_image(const struct PhotostaxRepo *repo,
                                      const char *path,
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

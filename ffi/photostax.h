#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

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

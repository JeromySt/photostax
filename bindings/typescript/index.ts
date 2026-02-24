/**
 * @photostax/core - Node.js binding for photostax
 *
 * Provides access to Epson FastFoto photo stack repositories from Node.js.
 * Uses a native addon built with napi-rs for high performance.
 *
 * @packageDocumentation
 */

// Try to load the native addon
let nativeBinding: NativeBinding | null = null;
let loadError: Error | null = null;

interface NativeBinding {
  PhotostaxRepository: typeof PhotostaxRepository;
}

try {
  // The native addon is built by napi-rs and placed next to this file
  // It will be named based on platform: photostax.win32-x64-msvc.node, etc.
  nativeBinding = require('./photostax.node') as NativeBinding;
} catch (e) {
  loadError = e instanceof Error ? e : new Error(String(e));
}

/**
 * Metadata associated with a photo stack.
 *
 * Combines three sources of metadata:
 * - EXIF tags from image files
 * - XMP tags from embedded/sidecar data
 * - Custom tags from the sidecar database
 */
export interface Metadata {
  /** Standard EXIF tags (Make, Model, DateTime, etc.) */
  exifTags: Record<string, string>;
  /** XMP/Dublin Core metadata */
  xmpTags: Record<string, string>;
  /** Custom application metadata (JSON values) */
  customTags: Record<string, unknown>;
}

/**
 * A unified photo stack from an Epson FastFoto scanner.
 *
 * Groups related image files (original, enhanced, back) into a single unit.
 */
export interface PhotoStack {
  /** Unique identifier (base filename without suffix) */
  id: string;
  /** Path to the original front scan */
  original: string | null;
  /** Path to the enhanced/color-corrected scan */
  enhanced: string | null;
  /** Path to the back-of-photo scan */
  back: string | null;
  /** Combined metadata from all sources */
  metadata: Metadata;
}

/**
 * A key-value filter for search queries.
 */
export interface KeyValueFilter {
  /** Tag name to filter on */
  key: string;
  /** Value substring to search for */
  value: string;
}

/**
 * Query parameters for searching photo stacks.
 *
 * All filters use AND logic - a stack must match all criteria.
 */
export interface SearchQuery {
  /** Free-text search across ID and metadata */
  text?: string;
  /** EXIF tag filters */
  exifFilters?: KeyValueFilter[];
  /** Custom tag filters */
  customFilters?: KeyValueFilter[];
  /** Filter by back scan presence */
  hasBack?: boolean;
  /** Filter by enhanced scan presence */
  hasEnhanced?: boolean;
}

/**
 * Convert camelCase Metadata to snake_case for native binding.
 */
function toNativeMetadata(meta: Partial<Metadata>): {
  exif_tags: Record<string, string>;
  xmp_tags: Record<string, string>;
  custom_tags: Record<string, unknown>;
} {
  return {
    exif_tags: meta.exifTags ?? {},
    xmp_tags: meta.xmpTags ?? {},
    custom_tags: meta.customTags ?? {},
  };
}

/**
 * Convert snake_case native metadata to camelCase.
 */
function fromNativeMetadata(native: {
  exif_tags: Record<string, string>;
  xmp_tags: Record<string, string>;
  custom_tags: Record<string, unknown>;
}): Metadata {
  return {
    exifTags: native.exif_tags ?? {},
    xmpTags: native.xmp_tags ?? {},
    customTags: native.custom_tags ?? {},
  };
}

/**
 * Convert native PhotoStack to TypeScript interface.
 */
function fromNativeStack(native: {
  id: string;
  original: string | null;
  enhanced: string | null;
  back: string | null;
  metadata: {
    exif_tags: Record<string, string>;
    xmp_tags: Record<string, string>;
    custom_tags: Record<string, unknown>;
  };
}): PhotoStack {
  return {
    id: native.id,
    original: native.original,
    enhanced: native.enhanced,
    back: native.back,
    metadata: fromNativeMetadata(native.metadata),
  };
}

/**
 * Convert TypeScript SearchQuery to native format.
 */
function toNativeSearchQuery(query: SearchQuery): {
  text: string | null;
  exif_filters: Array<{ key: string; value: string }> | null;
  custom_filters: Array<{ key: string; value: string }> | null;
  has_back: boolean | null;
  has_enhanced: boolean | null;
} {
  return {
    text: query.text ?? null,
    exif_filters: query.exifFilters ?? null,
    custom_filters: query.customFilters ?? null,
    has_back: query.hasBack ?? null,
    has_enhanced: query.hasEnhanced ?? null,
  };
}

/**
 * A repository for accessing Epson FastFoto photo stacks.
 *
 * Provides methods to scan, retrieve, and modify photo stacks
 * from a local filesystem directory.
 *
 * @example
 * ```typescript
 * import { PhotostaxRepository } from '@photostax/core';
 *
 * const repo = new PhotostaxRepository('/path/to/photos');
 * const stacks = repo.scan();
 *
 * for (const stack of stacks) {
 *   console.log(stack.id, stack.metadata.exifTags['Make']);
 * }
 * ```
 */
export class PhotostaxRepository {
  private _native: unknown;

  /**
   * Create a new repository rooted at the given directory.
   *
   * @param directoryPath - Path to the directory containing photo files
   * @throws Error if the native addon is not available
   */
  constructor(directoryPath: string) {
    if (!nativeBinding) {
      throw new Error(
        `Native addon not loaded. Build it first with 'npm run build'. ` +
          `Original error: ${loadError?.message ?? 'unknown'}`
      );
    }
    this._native = new nativeBinding.PhotostaxRepository(directoryPath);
  }

  /**
   * Scan the repository and return all photo stacks.
   *
   * Groups files by FastFoto naming convention and enriches each stack
   * with EXIF, XMP, and sidecar metadata.
   *
   * @returns Array of photo stacks
   * @throws Error if the directory cannot be accessed
   */
  scan(): PhotoStack[] {
    const native = this._native as { scan(): unknown[] };
    return native.scan().map((s) =>
      fromNativeStack(
        s as {
          id: string;
          original: string | null;
          enhanced: string | null;
          back: string | null;
          metadata: {
            exif_tags: Record<string, string>;
            xmp_tags: Record<string, string>;
            custom_tags: Record<string, unknown>;
          };
        }
      )
    );
  }

  /**
   * Retrieve a single photo stack by its ID.
   *
   * @param id - The stack identifier (base filename without _a/_b suffix)
   * @returns The photo stack
   * @throws Error if the stack is not found
   */
  getStack(id: string): PhotoStack {
    const native = this._native as { get_stack(id: string): unknown };
    return fromNativeStack(
      native.get_stack(id) as {
        id: string;
        original: string | null;
        enhanced: string | null;
        back: string | null;
        metadata: {
          exif_tags: Record<string, string>;
          xmp_tags: Record<string, string>;
          custom_tags: Record<string, unknown>;
        };
      }
    );
  }

  /**
   * Read the raw bytes of an image file.
   *
   * @param path - Path to the image file (from a PhotoStack)
   * @returns Buffer containing the image data
   * @throws Error if the file cannot be read
   */
  readImage(path: string): Buffer {
    const native = this._native as { read_image(path: string): Buffer };
    return native.read_image(path);
  }

  /**
   * Write metadata to a photo stack.
   *
   * XMP tags are written directly to image files (or sidecar for TIFF).
   * Custom and EXIF tags are stored in the sidecar database.
   *
   * @param stackId - ID of the stack to update
   * @param metadata - Metadata to write (partial update supported)
   * @throws Error if the stack is not found or write fails
   */
  writeMetadata(stackId: string, metadata: Partial<Metadata>): void {
    const native = this._native as {
      write_metadata(
        stackId: string,
        metadata: {
          exif_tags: Record<string, string>;
          xmp_tags: Record<string, string>;
          custom_tags: Record<string, unknown>;
        }
      ): void;
    };
    native.write_metadata(stackId, toNativeMetadata(metadata));
  }

  /**
   * Search for photo stacks matching the given criteria.
   *
   * All filters are combined with AND logic.
   *
   * @param query - Search criteria
   * @returns Array of matching photo stacks
   * @throws Error if the repository cannot be scanned
   *
   * @example
   * ```typescript
   * // Find EPSON photos with back scans
   * const results = repo.search({
   *   exifFilters: [{ key: 'Make', value: 'EPSON' }],
   *   hasBack: true
   * });
   * ```
   */
  search(query: SearchQuery): PhotoStack[] {
    const native = this._native as {
      search(query: {
        text: string | null;
        exif_filters: Array<{ key: string; value: string }> | null;
        custom_filters: Array<{ key: string; value: string }> | null;
        has_back: boolean | null;
        has_enhanced: boolean | null;
      }): unknown[];
    };
    return native.search(toNativeSearchQuery(query)).map((s) =>
      fromNativeStack(
        s as {
          id: string;
          original: string | null;
          enhanced: string | null;
          back: string | null;
          metadata: {
            exif_tags: Record<string, string>;
            xmp_tags: Record<string, string>;
            custom_tags: Record<string, unknown>;
          };
        }
      )
    );
  }
}

/**
 * Check if the native addon is available.
 *
 * @returns true if the native addon was loaded successfully
 */
export function isNativeAvailable(): boolean {
  return nativeBinding !== null;
}

/**
 * Get the error that occurred when loading the native addon.
 *
 * @returns The load error, or null if loaded successfully
 */
export function getNativeLoadError(): Error | null {
  return loadError;
}

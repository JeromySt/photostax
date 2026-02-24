/**
 * Type declarations for @photostax/core
 *
 * @packageDocumentation
 */

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
 * A repository for accessing Epson FastFoto photo stacks.
 *
 * Provides methods to scan, retrieve, and modify photo stacks
 * from a local filesystem directory.
 */
export declare class PhotostaxRepository {
  /**
   * Create a new repository rooted at the given directory.
   *
   * @param directoryPath - Path to the directory containing photo files
   * @throws Error if the native addon is not available
   */
  constructor(directoryPath: string);

  /**
   * Scan the repository and return all photo stacks.
   *
   * @returns Array of photo stacks
   * @throws Error if the directory cannot be accessed
   */
  scan(): PhotoStack[];

  /**
   * Retrieve a single photo stack by its ID.
   *
   * @param id - The stack identifier (base filename without _a/_b suffix)
   * @returns The photo stack
   * @throws Error if the stack is not found
   */
  getStack(id: string): PhotoStack;

  /**
   * Read the raw bytes of an image file.
   *
   * @param path - Path to the image file (from a PhotoStack)
   * @returns Buffer containing the image data
   */
  readImage(path: string): Buffer;

  /**
   * Write metadata to a photo stack.
   *
   * @param stackId - ID of the stack to update
   * @param metadata - Metadata to write (partial update supported)
   */
  writeMetadata(stackId: string, metadata: Partial<Metadata>): void;

  /**
   * Search for photo stacks matching the given criteria.
   *
   * @param query - Search criteria
   * @returns Array of matching photo stacks
   */
  search(query: SearchQuery): PhotoStack[];
}

/**
 * Check if the native addon is available.
 */
export declare function isNativeAvailable(): boolean;

/**
 * Get the error that occurred when loading the native addon.
 */
export declare function getNativeLoadError(): Error | null;

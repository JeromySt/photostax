using System.Runtime.InteropServices;

namespace Photostax.Native;

/// <summary>
/// FFI result type for operations that may fail.
/// </summary>
[StructLayout(LayoutKind.Sequential)]
internal struct FfiResult
{
    /// <summary>
    /// True if the operation succeeded.
    /// </summary>
    [MarshalAs(UnmanagedType.I1)]
    public bool Success;

    /// <summary>
    /// Error message (null on success, must be freed on failure).
    /// </summary>
    public IntPtr ErrorMessage;
}

/// <summary>
/// A photo stack returned across FFI.
/// </summary>
[StructLayout(LayoutKind.Sequential)]
internal struct FfiPhotoStack
{
    /// <summary>
    /// Stack identifier (never null).
    /// </summary>
    public IntPtr Id;

    /// <summary>
    /// Human-readable display name (never null).
    /// </summary>
    public IntPtr Name;

    /// <summary>
    /// Subfolder within the repository (null if root level).
    /// </summary>
    public IntPtr Folder;

    /// <summary>
    /// Path to original image (null if absent).
    /// </summary>
    public IntPtr Original;

    /// <summary>
    /// Path to enhanced image (null if absent).
    /// </summary>
    public IntPtr Enhanced;

    /// <summary>
    /// Path to back image (null if absent).
    /// </summary>
    public IntPtr Back;

    /// <summary>
    /// JSON-serialized metadata (never null, may be "{}").
    /// </summary>
    public IntPtr MetadataJson;
}

/// <summary>
/// Array of photo stacks from FFI.
/// </summary>
[StructLayout(LayoutKind.Sequential)]
internal struct FfiPhotoStackArray
{
    /// <summary>
    /// Pointer to array of stacks (null if len == 0).
    /// </summary>
    public IntPtr Data;

    /// <summary>
    /// Number of stacks in the array.
    /// </summary>
    public nuint Len;
}

/// <summary>
/// Paginated result of photo stacks from FFI.
/// </summary>
[StructLayout(LayoutKind.Sequential)]
internal struct FfiPaginatedResult
{
    /// <summary>
    /// Pointer to array of stacks in this page (null if len == 0).
    /// </summary>
    public IntPtr Data;

    /// <summary>
    /// Number of stacks in this page.
    /// </summary>
    public nuint Len;

    /// <summary>
    /// Total number of stacks across all pages.
    /// </summary>
    public nuint TotalCount;

    /// <summary>
    /// The offset used for this page.
    /// </summary>
    public nuint Offset;

    /// <summary>
    /// The page size limit used for this page.
    /// </summary>
    public nuint Limit;

    /// <summary>
    /// Whether there are more items beyond this page.
    /// </summary>
    [MarshalAs(UnmanagedType.I1)]
    public bool HasMore;
}

/// <summary>
/// Staleness information for a snapshot from FFI.
/// </summary>
[StructLayout(LayoutKind.Sequential)]
internal struct FfiSnapshotStatus
{
    /// <summary>
    /// True when the filesystem no longer matches the snapshot.
    /// </summary>
    [MarshalAs(UnmanagedType.I1)]
    public bool IsStale;

    /// <summary>
    /// Number of stacks in the snapshot.
    /// </summary>
    public nuint SnapshotCount;

    /// <summary>
    /// Number of stacks currently on disk.
    /// </summary>
    public nuint CurrentCount;

    /// <summary>
    /// New stacks on disk that were not in the snapshot.
    /// </summary>
    public nuint Added;

    /// <summary>
    /// Snapshot stacks no longer present on disk.
    /// </summary>
    public nuint Removed;
}

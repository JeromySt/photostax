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
/// Array of opaque PhotostaxStack handle pointers from FFI.
/// </summary>
[StructLayout(LayoutKind.Sequential)]
internal struct FfiStackHandleArray
{
    /// <summary>
    /// Pointer to array of PhotostaxStack* handle pointers (null if len == 0).
    /// </summary>
    public IntPtr Handles;

    /// <summary>
    /// Number of handles in the array.
    /// </summary>
    public nuint Len;
}

/// <summary>
/// Paginated result of opaque PhotostaxStack handles from FFI.
/// </summary>
[StructLayout(LayoutKind.Sequential)]
internal struct FfiPaginatedHandleResult
{
    /// <summary>
    /// Pointer to array of PhotostaxStack* handle pointers (null if len == 0).
    /// </summary>
    public IntPtr Handles;

    /// <summary>
    /// Number of handles in this page.
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
/// Image dimensions result from FFI.
/// </summary>
[StructLayout(LayoutKind.Sequential)]
internal struct FfiDimensions
{
    /// <summary>
    /// Image width in pixels.
    /// </summary>
    public uint Width;

    /// <summary>
    /// Image height in pixels.
    /// </summary>
    public uint Height;

    /// <summary>
    /// True if dimensions were successfully retrieved.
    /// </summary>
    [MarshalAs(UnmanagedType.I1)]
    public bool Success;
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

// ── Foreign repository provider types ──────────────────────────────

/// <summary>
/// A file entry from a foreign repository provider.
/// </summary>
[StructLayout(LayoutKind.Sequential)]
internal struct FfiFileEntry
{
    /// <summary>
    /// File name including extension (e.g., "IMG_001_a.jpg"). Never null.
    /// </summary>
    public IntPtr Name;

    /// <summary>
    /// Containing folder path relative to the repository root. Never null, may be empty.
    /// </summary>
    public IntPtr Folder;

    /// <summary>
    /// Full path relative to the repository root. Never null.
    /// </summary>
    public IntPtr Path;

    /// <summary>
    /// File size in bytes.
    /// </summary>
    public ulong Size;
}

/// <summary>
/// Result of a list_entries callback.
/// </summary>
[StructLayout(LayoutKind.Sequential)]
internal struct FfiFileEntryArray
{
    /// <summary>
    /// Pointer to array of entries (null if len == 0).
    /// </summary>
    public IntPtr Data;

    /// <summary>
    /// Number of entries.
    /// </summary>
    public nuint Len;

    /// <summary>
    /// Non-zero indicates an error (entries are invalid).
    /// </summary>
    public int Error;
}

/// <summary>
/// Result of an open_read or open_write callback.
/// </summary>
[StructLayout(LayoutKind.Sequential)]
internal struct FfiStreamHandle
{
    /// <summary>
    /// Opaque stream handle. Zero indicates failure.
    /// </summary>
    public ulong Handle;

    /// <summary>
    /// Non-zero indicates an error.
    /// </summary>
    public int Error;
}

/// <summary>
/// Result of a read callback.
/// </summary>
[StructLayout(LayoutKind.Sequential)]
internal struct FfiReadResult
{
    /// <summary>
    /// Number of bytes actually read.
    /// </summary>
    public nuint BytesRead;

    /// <summary>
    /// Non-zero indicates an error.
    /// </summary>
    public int Error;
}

/// <summary>
/// Result of a seek callback.
/// </summary>
[StructLayout(LayoutKind.Sequential)]
internal struct FfiSeekResult
{
    /// <summary>
    /// New position after seeking.
    /// </summary>
    public ulong Position;

    /// <summary>
    /// Non-zero indicates an error.
    /// </summary>
    public int Error;
}

/// <summary>
/// Result of a write callback.
/// </summary>
[StructLayout(LayoutKind.Sequential)]
internal struct FfiWriteResult
{
    /// <summary>
    /// Number of bytes actually written.
    /// </summary>
    public nuint BytesWritten;

    /// <summary>
    /// Non-zero indicates an error.
    /// </summary>
    public int Error;
}

// ── Callback delegate types ────────────────────────────────────────

[UnmanagedFunctionPointer(CallingConvention.Cdecl)]
internal delegate FfiFileEntryArray ListEntriesDelegate(
    IntPtr ctx,
    [MarshalAs(UnmanagedType.LPUTF8Str)] string prefix,
    [MarshalAs(UnmanagedType.I1)] bool recursive);

[UnmanagedFunctionPointer(CallingConvention.Cdecl)]
internal delegate void FreeEntriesDelegate(IntPtr ctx, FfiFileEntryArray entries);

[UnmanagedFunctionPointer(CallingConvention.Cdecl)]
internal delegate FfiStreamHandle OpenReadDelegate(
    IntPtr ctx,
    [MarshalAs(UnmanagedType.LPUTF8Str)] string path);

[UnmanagedFunctionPointer(CallingConvention.Cdecl)]
internal delegate FfiReadResult ReadDelegate(IntPtr ctx, ulong handle, IntPtr buf, nuint len);

[UnmanagedFunctionPointer(CallingConvention.Cdecl)]
internal delegate FfiSeekResult SeekDelegate(IntPtr ctx, ulong handle, long offset, int whence);

[UnmanagedFunctionPointer(CallingConvention.Cdecl)]
internal delegate void CloseReadDelegate(IntPtr ctx, ulong handle);

[UnmanagedFunctionPointer(CallingConvention.Cdecl)]
internal delegate FfiStreamHandle OpenWriteDelegate(
    IntPtr ctx,
    [MarshalAs(UnmanagedType.LPUTF8Str)] string path);

[UnmanagedFunctionPointer(CallingConvention.Cdecl)]
internal delegate FfiWriteResult WriteDelegate(IntPtr ctx, ulong handle, IntPtr buf, nuint len);

[UnmanagedFunctionPointer(CallingConvention.Cdecl)]
internal delegate void CloseWriteDelegate(IntPtr ctx, ulong handle);

[UnmanagedFunctionPointer(CallingConvention.Cdecl)]
[return: MarshalAs(UnmanagedType.U1)]
internal delegate bool IsWritableDelegate(IntPtr ctx);

/// <summary>
/// Callback function pointers for a foreign repository provider.
/// Field order must match the C header exactly.
/// </summary>
[StructLayout(LayoutKind.Sequential)]
internal struct FfiProviderCallbacks
{
    public IntPtr Ctx;
    public IntPtr Location;
    public IntPtr ListEntries;
    public IntPtr FreeEntries;
    public IntPtr OpenRead;
    public IntPtr Read;
    public IntPtr Seek;
    public IntPtr CloseRead;
    public IntPtr OpenWrite;
    public IntPtr Write;
    public IntPtr CloseWrite;
    public IntPtr IsWritable;
}

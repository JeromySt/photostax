using System.Runtime.InteropServices;
using Photostax.Native;

namespace Photostax;

/// <summary>
/// A point-in-time snapshot of scanned photo stacks for consistent pagination.
/// </summary>
/// <remarks>
/// Pages from a snapshot always have a consistent total count, even if the
/// underlying filesystem changes between page requests. Dispose the snapshot
/// when you are done paging to free native memory.
/// </remarks>
public sealed class ScanSnapshot : IDisposable
{
    private readonly SnapshotSafeHandle _handle;
    private bool _disposed;

    internal ScanSnapshot(SnapshotSafeHandle handle)
    {
        _handle = handle;
    }

    /// <summary>
    /// Gets the native handle for interop.
    /// </summary>
    internal IntPtr Handle => _handle.DangerousGetHandle();

    /// <summary>
    /// Total number of stacks in the snapshot.
    /// </summary>
    public int TotalCount
    {
        get
        {
            ThrowIfDisposed();
            return (int)NativeMethods.photostax_snapshot_total_count(_handle.DangerousGetHandle());
        }
    }

    /// <summary>
    /// Get a page of stacks from the snapshot.
    /// This is a pure in-memory operation — it never touches the filesystem.
    /// </summary>
    /// <param name="offset">Number of stacks to skip (0-based).</param>
    /// <param name="limit">Maximum number of stacks to return per page.</param>
    /// <returns>A paginated result with items and metadata.</returns>
    public PaginatedResult<PhotoStack> GetPage(int offset, int limit)
    {
        ThrowIfDisposed();

        var result = NativeMethods.photostax_snapshot_get_page(
            _handle.DangerousGetHandle(),
            (nuint)offset,
            (nuint)limit);
        try
        {
            return ConvertPaginatedResult(result);
        }
        finally
        {
            NativeMethods.photostax_paginated_result_free(result);
        }
    }

    /// <summary>
    /// Filter the snapshot by a search query, returning a new snapshot.
    /// </summary>
    /// <param name="query">The search query (all filters are AND'd together).</param>
    /// <returns>A new snapshot containing only matching stacks.</returns>
    public ScanSnapshot Filter(SearchQuery query)
    {
        ArgumentNullException.ThrowIfNull(query);
        ThrowIfDisposed();

        var queryJson = query.ToJson();
        var ptr = NativeMethods.photostax_snapshot_filter(
            _handle.DangerousGetHandle(),
            queryJson);

        if (ptr == IntPtr.Zero)
            throw new PhotostaxException("Failed to filter snapshot.");

        return new ScanSnapshot(SnapshotSafeHandle.FromPointer(ptr));
    }

    /// <inheritdoc />
    public void Dispose()
    {
        if (!_disposed)
        {
            _handle.Dispose();
            _disposed = true;
        }
    }

    private void ThrowIfDisposed()
    {
        ObjectDisposedException.ThrowIf(_disposed, this);
    }

    private static PaginatedResult<PhotoStack> ConvertPaginatedResult(FfiPaginatedResult result)
    {
        var items = new List<PhotoStack>();

        if (result.Data != IntPtr.Zero && result.Len > 0)
        {
            var structSize = Marshal.SizeOf<FfiPhotoStack>();
            for (nuint i = 0; i < result.Len; i++)
            {
                var stackPtr = IntPtr.Add(result.Data, (int)i * structSize);
                var ffiStack = Marshal.PtrToStructure<FfiPhotoStack>(stackPtr);
                items.Add(PhotostaxRepository.ConvertStack(ffiStack));
            }
        }

        return new PaginatedResult<PhotoStack>(
            items,
            (int)result.TotalCount,
            (int)result.Offset,
            (int)result.Limit,
            result.HasMore);
    }
}

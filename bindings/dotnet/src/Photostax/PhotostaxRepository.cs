using System.Diagnostics.CodeAnalysis;
using System.Runtime.InteropServices;
using Photostax.Native;

namespace Photostax;

/// <summary>
/// Represents a local photo repository.
/// </summary>
/// <remarks>
/// This class is excluded from code coverage because all methods depend on the
/// native photostax_ffi library and cannot be unit-tested without it.
/// Integration tests with the native DLL provide coverage for this class.
/// </remarks>
[ExcludeFromCodeCoverage]
public sealed class PhotostaxRepository : IDisposable
{
    private readonly RepoSafeHandle _handle;
    private bool _disposed;

    /// <summary>
    /// Initializes a new instance of the <see cref="PhotostaxRepository"/> class.
    /// </summary>
    /// <param name="directoryPath">The path to the repository directory.</param>
    /// <param name="recursive">When <c>true</c>, subdirectories are scanned recursively.
    /// Required when the photo library uses FastFoto's folder-based organisation
    /// (e.g. <c>1984_Mexico/</c>, <c>Mexico/</c>).</param>
    /// <exception cref="ArgumentNullException">Thrown when <paramref name="directoryPath"/> is null.</exception>
    /// <exception cref="PhotostaxException">Thrown when the repository cannot be opened.</exception>
    public PhotostaxRepository(string directoryPath, bool recursive = false)
    {
        ArgumentNullException.ThrowIfNull(directoryPath);

        var ptr = NativeMethods.photostax_repo_open_recursive(directoryPath, recursive);
        if (ptr == IntPtr.Zero)
        {
            throw new PhotostaxException($"Failed to open repository at '{directoryPath}'");
        }

        _handle = RepoSafeHandle.FromPointer(ptr);
    }

    /// <summary>
    /// Scans the repository and returns all photo stacks.
    /// </summary>
    /// <returns>A list of photo stacks found in the repository.</returns>
    /// <exception cref="ObjectDisposedException">Thrown when the repository has been disposed.</exception>
    public IReadOnlyList<PhotoStack> Scan()
    {
        ThrowIfDisposed();

        var array = NativeMethods.photostax_repo_scan(_handle.DangerousGetHandle());
        try
        {
            return PhotoStack.ConvertHandleArray(array);
        }
        finally
        {
            NativeMethods.photostax_stack_handle_array_free(array);
        }
    }

    /// <summary>
    /// Scans with a <see cref="ScannerProfile"/> and optional progress callback.
    /// </summary>
    /// <remarks>
    /// <para>
    /// The <paramref name="profile"/> tells the engine how the FastFoto was configured:
    /// </para>
    /// <list type="bullet">
    ///   <item><see cref="ScannerProfile.Auto"/> — unknown config, uses pixel analysis for ambiguous _a (disk I/O)</item>
    ///   <item><see cref="ScannerProfile.EnhancedAndBack"/> — _a = enhanced, _b = back (no I/O)</item>
    ///   <item><see cref="ScannerProfile.EnhancedOnly"/> — _a = enhanced, no back files (no I/O)</item>
    ///   <item><see cref="ScannerProfile.OriginalOnly"/> — no _a or _b expected (no I/O)</item>
    /// </list>
    /// <para>
    /// The <paramref name="onProgress"/> callback is invoked for each progress step with the
    /// current phase, items processed, and total items.
    /// </para>
    /// </remarks>
    /// <param name="profile">FastFoto scanner configuration.</param>
    /// <param name="onProgress">Optional progress callback (phase, current, total).</param>
    /// <returns>A list of photo stacks.</returns>
    public IReadOnlyList<PhotoStack> ScanWithProgress(
        ScannerProfile profile = ScannerProfile.Auto,
        Action<ScanPhase, int, int>? onProgress = null)
    {
        ThrowIfDisposed();

        NativeMethods.ScanProgressCallback? nativeCallback = null;
        if (onProgress != null)
        {
            nativeCallback = (phase, current, total, _) =>
                onProgress((ScanPhase)phase, (int)current, (int)total);
        }

        var array = NativeMethods.photostax_repo_scan_with_progress(
            _handle.DangerousGetHandle(),
            (int)profile,
            nativeCallback,
            IntPtr.Zero);
        try
        {
            return PhotoStack.ConvertHandleArray(array);
        }
        finally
        {
            NativeMethods.photostax_stack_handle_array_free(array);
        }
    }

    /// <summary>
    /// Scans the repository and returns all photo stacks with full metadata loaded.
    /// </summary>
    /// <remarks>
    /// This is the slower path that reads EXIF, XMP, and sidecar data for every stack.
    /// Prefer <see cref="Scan"/> and then calling <c>stack.Metadata.Read()</c> for lazy-loading in large repositories.
    /// </remarks>
    /// <returns>A list of photo stacks with complete metadata.</returns>
    /// <exception cref="ObjectDisposedException">Thrown when the repository has been disposed.</exception>
    public IReadOnlyList<PhotoStack> ScanWithMetadata()
    {
        ThrowIfDisposed();

        var stacks = Scan();
        foreach (var stack in stacks)
        {
            try { stack.Metadata.Read(); }
            catch { /* skip stacks that fail metadata loading */ }
        }
        return stacks;
    }

    /// <summary>
    /// Searches for photo stacks matching the specified query.
    /// </summary>
    /// <param name="query">The search query.</param>
    /// <returns>A list of matching photo stacks.</returns>
    /// <exception cref="ArgumentNullException">Thrown when <paramref name="query"/> is null.</exception>
    /// <exception cref="ObjectDisposedException">Thrown when the repository has been disposed.</exception>
    public IReadOnlyList<PhotoStack> Search(SearchQuery query)
    {
        ArgumentNullException.ThrowIfNull(query);
        ThrowIfDisposed();

        var queryJson = query.ToJson();
        var array = NativeMethods.photostax_search(_handle.DangerousGetHandle(), queryJson);
        try
        {
            return PhotoStack.ConvertHandleArray(array);
        }
        finally
        {
            NativeMethods.photostax_stack_handle_array_free(array);
        }
    }

    /// <summary>
    /// Scans the repository and returns a paginated result of photo stacks.
    /// </summary>
    /// <param name="offset">Number of stacks to skip (0-based).</param>
    /// <param name="limit">Maximum number of stacks to return per page.</param>
    /// <param name="loadMetadata">When true, loads EXIF/XMP/sidecar metadata for each stack in the page.</param>
    /// <returns>A paginated result containing photo stacks and metadata.</returns>
    /// <exception cref="ObjectDisposedException">Thrown when the repository has been disposed.</exception>
    public PaginatedResult<PhotoStack> ScanPaginated(int offset, int limit, bool loadMetadata = false)
    {
        ThrowIfDisposed();

        var result = NativeMethods.photostax_repo_scan_paginated(
            _handle.DangerousGetHandle(),
            (nuint)offset,
            (nuint)limit,
            loadMetadata);
        try
        {
            return PhotoStack.ConvertPaginatedHandleResult(result);
        }
        finally
        {
            NativeMethods.photostax_paginated_handle_result_free(result);
        }
    }

    /// <summary>
    /// Searches for photo stacks with pagination.
    /// </summary>
    /// <param name="query">The search query.</param>
    /// <param name="offset">Number of stacks to skip (0-based).</param>
    /// <param name="limit">Maximum number of stacks to return per page.</param>
    /// <returns>A paginated result containing matching photo stacks and metadata.</returns>
    /// <exception cref="ArgumentNullException">Thrown when <paramref name="query"/> is null.</exception>
    /// <exception cref="ObjectDisposedException">Thrown when the repository has been disposed.</exception>
    public PaginatedResult<PhotoStack> SearchPaginated(SearchQuery query, int offset, int limit)
    {
        ArgumentNullException.ThrowIfNull(query);
        ThrowIfDisposed();

        var queryJson = query.ToJson();
        var result = NativeMethods.photostax_search_paginated(
            _handle.DangerousGetHandle(),
            queryJson,
            (nuint)offset,
            (nuint)limit);
        try
        {
            return PhotoStack.ConvertPaginatedHandleResult(result);
        }
        finally
        {
            NativeMethods.photostax_paginated_handle_result_free(result);
        }
    }

    /// <summary>
    /// Unified query: search and paginate the cache in a single call.
    /// </summary>
    /// <remarks>
    /// This is the preferred way to retrieve stacks. Combines filtering and
    /// pagination into one operation. Call <see cref="Scan"/> or <see cref="ScanWithMetadata"/>
    /// first to populate the cache.
    /// </remarks>
    /// <param name="query">Search criteria, or null to match all stacks.</param>
    /// <param name="offset">Number of stacks to skip (0-based).</param>
    /// <param name="limit">Maximum stacks to return. Use 0 to return all matching stacks.</param>
    /// <returns>A paginated result containing matching photo stacks and metadata.</returns>
    /// <exception cref="ObjectDisposedException">Thrown when the repository has been disposed.</exception>
    public PaginatedResult<PhotoStack> Query(SearchQuery? query = null, int offset = 0, int limit = 0)
    {
        ThrowIfDisposed();

        var queryJson = query?.ToJson();
        var result = NativeMethods.photostax_query(
            _handle.DangerousGetHandle(),
            queryJson,
            (nuint)offset,
            (nuint)limit);
        try
        {
            return PhotoStack.ConvertPaginatedHandleResult(result);
        }
        finally
        {
            NativeMethods.photostax_paginated_handle_result_free(result);
        }
    }

    /// <summary>
    /// Create a point-in-time snapshot for consistent pagination.
    /// </summary>
    /// <remarks>
    /// The snapshot captures the current set of stacks so that page requests
    /// always see the same total count and ordering, even if files are added
    /// or removed on disk between page calls.
    /// </remarks>
    /// <param name="loadMetadata">When true, loads EXIF/XMP/sidecar metadata for every stack.</param>
    /// <returns>A frozen snapshot that supports <see cref="ScanSnapshot.GetPage"/> and <see cref="ScanSnapshot.Filter"/>.</returns>
    /// <exception cref="PhotostaxException">Thrown when the scan fails.</exception>
    /// <exception cref="ObjectDisposedException">Thrown when the repository has been disposed.</exception>
    public ScanSnapshot CreateSnapshot(bool loadMetadata = false)
    {
        ThrowIfDisposed();

        var ptr = NativeMethods.photostax_create_snapshot(
            _handle.DangerousGetHandle(), loadMetadata);

        if (ptr == IntPtr.Zero)
            throw new PhotostaxException("Failed to create snapshot.");

        return new ScanSnapshot(SnapshotSafeHandle.FromPointer(ptr), _handle.DangerousGetHandle());
    }

    /// <summary>
    /// Creates a snapshot with a scanner profile and optional progress callback.
    /// </summary>
    /// <remarks>
    /// Combines scanning, classification, optional metadata loading, and snapshot
    /// creation in a single pass — no redundant re-scanning.
    /// </remarks>
    /// <param name="profile">FastFoto scanner configuration.</param>
    /// <param name="loadMetadata">When true, loads metadata for every stack.</param>
    /// <param name="onProgress">Optional progress callback (phase, current, total).</param>
    /// <returns>A frozen snapshot.</returns>
    /// <exception cref="PhotostaxException">Thrown when the scan fails.</exception>
    /// <exception cref="ObjectDisposedException">Thrown when the repository has been disposed.</exception>
    public ScanSnapshot CreateSnapshot(
        ScannerProfile profile,
        bool loadMetadata = false,
        Action<ScanPhase, int, int>? onProgress = null)
    {
        ThrowIfDisposed();

        NativeMethods.ScanProgressCallback? nativeCallback = null;
        if (onProgress != null)
        {
            nativeCallback = (phase, current, total, _) =>
                onProgress((ScanPhase)phase, (int)current, (int)total);
        }

        var ptr = NativeMethods.photostax_create_snapshot_with_progress(
            _handle.DangerousGetHandle(),
            (int)profile,
            loadMetadata,
            nativeCallback,
            IntPtr.Zero);

        if (ptr == IntPtr.Zero)
            throw new PhotostaxException("Failed to create snapshot.");

        return new ScanSnapshot(SnapshotSafeHandle.FromPointer(ptr), _handle.DangerousGetHandle());
    }

    /// <summary>
    /// Check whether a snapshot is still current.
    /// </summary>
    /// <remarks>
    /// Performs a fast re-scan and compares against the snapshot to detect
    /// added or removed stacks. Use this to decide when to create a new snapshot.
    /// </remarks>
    /// <param name="snapshot">The snapshot to check.</param>
    /// <returns>Status information including staleness and change counts.</returns>
    /// <exception cref="ObjectDisposedException">Thrown when the repository has been disposed.</exception>
    public SnapshotStatus CheckSnapshotStatus(ScanSnapshot snapshot)
    {
        ArgumentNullException.ThrowIfNull(snapshot);
        ThrowIfDisposed();

        var status = NativeMethods.photostax_snapshot_check_status(
            _handle.DangerousGetHandle(),
            snapshot.Handle);

        return new SnapshotStatus(
            status.IsStale,
            (int)status.SnapshotCount,
            (int)status.CurrentCount,
            (int)status.Added,
            (int)status.Removed);
    }

    /// <summary>
    /// Disposes the repository and releases all resources.
    /// </summary>
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

    private static string? GetErrorMessage(FfiResult result)
    {
        if (result.ErrorMessage == IntPtr.Zero)
            return null;

        var message = Marshal.PtrToStringUTF8(result.ErrorMessage);
        NativeMethods.photostax_string_free(result.ErrorMessage);
        return message;
    }
}

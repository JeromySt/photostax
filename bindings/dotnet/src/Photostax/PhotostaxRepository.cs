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
    /// Query the repository for stacks with optional filtering and pagination.
    /// </summary>
    /// <remarks>
    /// This is the sole entry point for retrieving stacks, matching the Rust
    /// <c>StackManager.query()</c> model. Pass <c>null</c> to match all stacks
    /// (equivalent to a scan), or provide a <see cref="SearchQuery"/> to filter.
    /// </remarks>
    /// <param name="query">Search criteria, or null to match all stacks.</param>
    /// <param name="pageSize">Number of stacks per page. Use 0 to put all stacks on a single page.</param>
    /// <param name="onProgress">Optional progress callback invoked during scanning phases.</param>
    /// <returns>A paginated query result with page-based navigation.</returns>
    /// <exception cref="ObjectDisposedException">Thrown when the repository has been disposed.</exception>
    public QueryResult Query(SearchQuery? query = null, int pageSize = 0, Action<ScanPhase, int, int>? onProgress = null)
    {
        ThrowIfDisposed();

        var queryJson = query?.ToJson();

        NativeMethods.ScanProgressCallback? nativeCallback = null;
        if (onProgress != null)
        {
            nativeCallback = (phase, current, total, _) =>
            {
                onProgress((ScanPhase)phase, (int)current, (int)total);
            };
        }

        var result = NativeMethods.photostax_query(
            _handle.DangerousGetHandle(),
            queryJson,
            (nuint)0,
            (nuint)0,
            nativeCallback,
            IntPtr.Zero);
        try
        {
            var stacks = PhotoStack.ConvertPaginatedHandleResultToList(result);
            return new QueryResult(stacks, pageSize);
        }
        finally
        {
            NativeMethods.photostax_paginated_handle_result_free(result);
            GC.KeepAlive(nativeCallback);
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

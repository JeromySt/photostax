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
            return ConvertStackArray(array);
        }
        finally
        {
            NativeMethods.photostax_stack_array_free(array);
        }
    }

    /// <summary>
    /// Scans the repository and returns all photo stacks with full metadata loaded.
    /// </summary>
    /// <remarks>
    /// This is the slower path that reads EXIF, XMP, and sidecar data for every stack.
    /// Prefer <see cref="Scan"/> + <see cref="LoadMetadata"/> for lazy-loading in large repositories.
    /// </remarks>
    /// <returns>A list of photo stacks with complete metadata.</returns>
    /// <exception cref="ObjectDisposedException">Thrown when the repository has been disposed.</exception>
    public IReadOnlyList<PhotoStack> ScanWithMetadata()
    {
        ThrowIfDisposed();

        // Scan to get lightweight stacks, then load metadata for each one
        var array = NativeMethods.photostax_repo_scan(_handle.DangerousGetHandle());
        try
        {
            var stacks = ConvertStackArray(array);
            var result = new List<PhotoStack>(stacks.Count);
            foreach (var stack in stacks)
            {
                var metadata = LoadMetadataCore(stack.Id);
                result.Add(new PhotoStack(stack.Id, stack.OriginalPath, stack.EnhancedPath, stack.BackPath, metadata ?? stack.Metadata));
            }
            return result;
        }
        finally
        {
            NativeMethods.photostax_stack_array_free(array);
        }
    }

    /// <summary>
    /// Loads full metadata (EXIF, XMP, sidecar) for a specific stack.
    /// </summary>
    /// <remarks>
    /// Use with <see cref="Scan"/> for lazy-loading: scan first to get lightweight
    /// stacks, then load metadata on demand for individual stacks.
    /// </remarks>
    /// <param name="stackId">The stack identifier.</param>
    /// <returns>The loaded metadata.</returns>
    /// <exception cref="ArgumentNullException">Thrown when <paramref name="stackId"/> is null.</exception>
    /// <exception cref="PhotostaxException">Thrown when the stack is not found or metadata cannot be loaded.</exception>
    /// <exception cref="ObjectDisposedException">Thrown when the repository has been disposed.</exception>
    public Metadata LoadMetadata(string stackId)
    {
        ArgumentNullException.ThrowIfNull(stackId);
        ThrowIfDisposed();

        return LoadMetadataCore(stackId)
            ?? throw new PhotostaxException($"Failed to load metadata for stack '{stackId}'");
    }

    /// <summary>
    /// Gets a single photo stack by its identifier.
    /// </summary>
    /// <param name="id">The stack identifier.</param>
    /// <returns>The photo stack.</returns>
    /// <exception cref="ArgumentNullException">Thrown when <paramref name="id"/> is null.</exception>
    /// <exception cref="PhotostaxException">Thrown when the stack is not found.</exception>
    /// <exception cref="ObjectDisposedException">Thrown when the repository has been disposed.</exception>
    public PhotoStack GetStack(string id)
    {
        ArgumentNullException.ThrowIfNull(id);
        ThrowIfDisposed();

        var ptr = NativeMethods.photostax_repo_get_stack(_handle.DangerousGetHandle(), id);
        if (ptr == IntPtr.Zero)
        {
            throw new PhotostaxException($"Stack '{id}' not found");
        }

        using var stackHandle = StackSafeHandle.FromPointer(ptr);
        return ConvertStack(Marshal.PtrToStructure<FfiPhotoStack>(ptr));
    }

    /// <summary>
    /// Reads the bytes of an image file.
    /// </summary>
    /// <param name="path">The path to the image file.</param>
    /// <returns>The image bytes.</returns>
    /// <exception cref="ArgumentNullException">Thrown when <paramref name="path"/> is null.</exception>
    /// <exception cref="PhotostaxException">Thrown when the image cannot be read.</exception>
    /// <exception cref="ObjectDisposedException">Thrown when the repository has been disposed.</exception>
    public byte[] ReadImage(string path)
    {
        ArgumentNullException.ThrowIfNull(path);
        ThrowIfDisposed();

        var result = NativeMethods.photostax_read_image(
            _handle.DangerousGetHandle(),
            path,
            out var dataPtr,
            out var len);

        if (!result.Success)
        {
            var errorMessage = GetErrorMessage(result);
            throw new PhotostaxException(errorMessage ?? $"Failed to read image at '{path}'");
        }

        using var bytesHandle = BytesSafeHandle.FromPointer(dataPtr, len);
        return bytesHandle.ToArray();
    }

    /// <summary>
    /// Writes metadata to a photo stack.
    /// </summary>
    /// <param name="stackId">The stack identifier.</param>
    /// <param name="metadata">The metadata to write.</param>
    /// <exception cref="ArgumentNullException">Thrown when <paramref name="stackId"/> or <paramref name="metadata"/> is null.</exception>
    /// <exception cref="PhotostaxException">Thrown when the metadata cannot be written.</exception>
    /// <exception cref="ObjectDisposedException">Thrown when the repository has been disposed.</exception>
    public void WriteMetadata(string stackId, Metadata metadata)
    {
        ArgumentNullException.ThrowIfNull(stackId);
        ArgumentNullException.ThrowIfNull(metadata);
        ThrowIfDisposed();

        var json = metadata.ToJson();
        var result = NativeMethods.photostax_write_metadata(
            _handle.DangerousGetHandle(),
            stackId,
            json);

        if (!result.Success)
        {
            var errorMessage = GetErrorMessage(result);
            throw new PhotostaxException(errorMessage ?? $"Failed to write metadata for stack '{stackId}'");
        }
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
            return ConvertStackArray(array);
        }
        finally
        {
            NativeMethods.photostax_stack_array_free(array);
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
            return ConvertPaginatedResult(result);
        }
        finally
        {
            NativeMethods.photostax_paginated_result_free(result);
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
            return ConvertPaginatedResult(result);
        }
        finally
        {
            NativeMethods.photostax_paginated_result_free(result);
        }
    }

    /// <summary>
    /// Rotates all images in a photo stack by the given number of degrees.
    /// </summary>
    /// <remarks>
    /// Every image file (original, enhanced, back) is decoded, rotated at the
    /// pixel level, and re-encoded on disk.  JPEG files are re-encoded (lossy).
    /// Returns the refreshed stack with updated metadata.
    /// </remarks>
    /// <param name="stackId">The stack identifier.</param>
    /// <param name="degrees">Rotation angle: 90, -90, 180, or -180.</param>
    /// <returns>The updated photo stack with refreshed metadata.</returns>
    /// <exception cref="ArgumentNullException">Thrown when <paramref name="stackId"/> is null.</exception>
    /// <exception cref="ArgumentException">Thrown when <paramref name="degrees"/> is not a valid rotation angle.</exception>
    /// <exception cref="PhotostaxException">Thrown when the stack is not found or rotation fails.</exception>
    /// <exception cref="ObjectDisposedException">Thrown when the repository has been disposed.</exception>
    public PhotoStack RotateStack(string stackId, int degrees)
    {
        ArgumentNullException.ThrowIfNull(stackId);
        ThrowIfDisposed();

        if (degrees != 90 && degrees != -90 && degrees != 180 && degrees != -180 && degrees != 270)
        {
            throw new ArgumentException(
                $"Invalid rotation: {degrees}°. Accepted values: 90, -90, 180, -180.",
                nameof(degrees));
        }

        var ptr = NativeMethods.photostax_rotate_stack(
            _handle.DangerousGetHandle(), stackId, degrees);

        if (ptr == IntPtr.Zero)
        {
            throw new PhotostaxException($"Failed to rotate stack '{stackId}' by {degrees}°");
        }

        using var stackHandle = StackSafeHandle.FromPointer(ptr);
        return ConvertStack(Marshal.PtrToStructure<FfiPhotoStack>(ptr));
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

        return new ScanSnapshot(SnapshotSafeHandle.FromPointer(ptr));
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

    private Metadata? LoadMetadataCore(string stackId)
    {
        var ptr = NativeMethods.photostax_stack_load_metadata(
            _handle.DangerousGetHandle(), stackId);
        if (ptr == IntPtr.Zero)
            return null;

        try
        {
            var json = Marshal.PtrToStringUTF8(ptr) ?? "{}";
            return Metadata.FromJson(json);
        }
        finally
        {
            NativeMethods.photostax_string_free(ptr);
        }
    }

    private static string? GetErrorMessage(FfiResult result)
    {
        if (result.ErrorMessage == IntPtr.Zero)
            return null;

        var message = Marshal.PtrToStringUTF8(result.ErrorMessage);
        NativeMethods.photostax_string_free(result.ErrorMessage);
        return message;
    }

    private static IReadOnlyList<PhotoStack> ConvertStackArray(FfiPhotoStackArray array)
    {
        if (array.Data == IntPtr.Zero || array.Len == 0)
            return [];

        var stacks = new List<PhotoStack>((int)array.Len);
        var structSize = Marshal.SizeOf<FfiPhotoStack>();

        for (nuint i = 0; i < array.Len; i++)
        {
            var stackPtr = IntPtr.Add(array.Data, (int)i * structSize);
            var ffiStack = Marshal.PtrToStructure<FfiPhotoStack>(stackPtr);
            stacks.Add(ConvertStack(ffiStack));
        }

        return stacks;
    }

    internal static PhotoStack ConvertStack(FfiPhotoStack ffi)
    {
        var id = Marshal.PtrToStringUTF8(ffi.Id) ?? throw new PhotostaxException("Stack ID is null");
        var original = ffi.Original != IntPtr.Zero ? Marshal.PtrToStringUTF8(ffi.Original) : null;
        var enhanced = ffi.Enhanced != IntPtr.Zero ? Marshal.PtrToStringUTF8(ffi.Enhanced) : null;
        var back = ffi.Back != IntPtr.Zero ? Marshal.PtrToStringUTF8(ffi.Back) : null;
        var metadataJson = Marshal.PtrToStringUTF8(ffi.MetadataJson) ?? "{}";
        var metadata = Metadata.FromJson(metadataJson);

        return new PhotoStack(id, original, enhanced, back, metadata);
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
                items.Add(ConvertStack(ffiStack));
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

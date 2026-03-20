using System.Diagnostics.CodeAnalysis;
using System.Runtime.InteropServices;
using Photostax.Native;

namespace Photostax;

/// <summary>
/// A multi-repository stack manager that merges stacks from multiple directories
/// into a single unified cache with O(1) lookups and globally unique IDs.
/// </summary>
/// <remarks>
/// <para>
/// Use <see cref="StackManager"/> when you need to manage multiple photo directories
/// as a single collection. For single-directory convenience, use <see cref="PhotostaxRepository"/>.
/// </para>
/// <para>
/// All stacks from every registered repository are accessible through a single cache.
/// Stack IDs are globally unique (opaque SHA-256 hashes) even when different directories
/// contain files with the same name.
/// </para>
/// </remarks>
[ExcludeFromCodeCoverage]
public sealed class StackManager : IDisposable
{
    private readonly RepoSafeHandle _handle;
    private bool _disposed;

    /// <summary>
    /// Creates an empty StackManager with no repositories.
    /// Call <see cref="AddRepo"/> to register directories before scanning.
    /// </summary>
    /// <exception cref="PhotostaxException">Thrown when the manager cannot be created.</exception>
    public StackManager()
    {
        var ptr = NativeMethods.photostax_manager_new();
        if (ptr == IntPtr.Zero)
        {
            throw new PhotostaxException("Failed to create StackManager");
        }

        _handle = RepoSafeHandle.FromPointer(ptr);
    }

    /// <summary>
    /// Gets the number of registered repositories.
    /// </summary>
    public int RepoCount
    {
        get
        {
            ThrowIfDisposed();
            return (int)NativeMethods.photostax_manager_repo_count(_handle.DangerousGetHandle());
        }
    }

    /// <summary>
    /// Registers a repository directory.
    /// </summary>
    /// <remarks>
    /// Multiple directories can be added — all will be scanned together and their
    /// stacks merged into a single cache with globally unique IDs. Overlapping
    /// directories within the same URI scheme are rejected.
    /// </remarks>
    /// <param name="directoryPath">Path to the directory containing FastFoto files.</param>
    /// <param name="recursive">When true, subdirectories are scanned recursively.</param>
    /// <param name="profile">FastFoto scanner configuration (default: Auto).</param>
    /// <exception cref="ArgumentNullException">Thrown when <paramref name="directoryPath"/> is null.</exception>
    /// <exception cref="PhotostaxException">Thrown when the path overlaps with an existing repo.</exception>
    /// <exception cref="ObjectDisposedException">Thrown when the manager has been disposed.</exception>
    public void AddRepo(
        string directoryPath,
        bool recursive = false,
        ScannerProfile profile = ScannerProfile.Auto)
    {
        ArgumentNullException.ThrowIfNull(directoryPath);
        ThrowIfDisposed();

        var result = NativeMethods.photostax_manager_add_repo(
            _handle.DangerousGetHandle(),
            directoryPath,
            recursive,
            (int)profile);

        if (!result.Success)
        {
            var errorMessage = GetErrorMessage(result);
            throw new PhotostaxException(
                errorMessage ?? $"Failed to add repository at '{directoryPath}'");
        }
    }

    /// <summary>
    /// Scans all registered repositories and returns all discovered photo stacks.
    /// </summary>
    public IReadOnlyList<PhotoStack> Scan()
    {
        ThrowIfDisposed();

        var array = NativeMethods.photostax_repo_scan(_handle.DangerousGetHandle());
        try
        {
            return PhotostaxRepository.ConvertStackArray(array);
        }
        finally
        {
            NativeMethods.photostax_stack_array_free(array);
        }
    }

    /// <summary>
    /// Gets a single photo stack by its opaque ID.
    /// </summary>
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
        return PhotostaxRepository.ConvertStack(Marshal.PtrToStructure<FfiPhotoStack>(ptr));
    }

    /// <summary>
    /// Loads full metadata (EXIF, XMP, sidecar) for a specific stack.
    /// </summary>
    public Metadata LoadMetadata(string stackId)
    {
        ArgumentNullException.ThrowIfNull(stackId);
        ThrowIfDisposed();

        var ptr = NativeMethods.photostax_stack_load_metadata(
            _handle.DangerousGetHandle(), stackId);
        if (ptr == IntPtr.Zero)
        {
            throw new PhotostaxException($"Failed to load metadata for stack '{stackId}'");
        }

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

    /// <summary>
    /// Reads the bytes of an image file, trying each registered repository.
    /// </summary>
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
    /// Writes metadata to a photo stack, routing to the correct repository.
    /// </summary>
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
            throw new PhotostaxException(
                errorMessage ?? $"Failed to write metadata for stack '{stackId}'");
        }
    }

    /// <summary>
    /// Unified query: search and paginate across all repositories in a single call.
    /// </summary>
    /// <param name="query">Search criteria, or null to match all stacks.</param>
    /// <param name="offset">Number of stacks to skip (0-based).</param>
    /// <param name="limit">Maximum stacks to return. Use 0 to return all matching stacks.</param>
    /// <returns>A paginated result containing matching photo stacks.</returns>
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
            return PhotostaxRepository.ConvertPaginatedResult(result);
        }
        finally
        {
            NativeMethods.photostax_paginated_result_free(result);
        }
    }

    /// <summary>
    /// Rotates images in a photo stack by the given number of degrees.
    /// </summary>
    public PhotoStack RotateStack(string stackId, int degrees, RotationTarget target = RotationTarget.All)
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
            _handle.DangerousGetHandle(), stackId, degrees, (int)target);

        if (ptr == IntPtr.Zero)
        {
            throw new PhotostaxException($"Failed to rotate stack '{stackId}' by {degrees}°");
        }

        using var stackHandle = StackSafeHandle.FromPointer(ptr);
        return PhotostaxRepository.ConvertStack(Marshal.PtrToStructure<FfiPhotoStack>(ptr));
    }

    /// <summary>
    /// Disposes the manager and releases all resources.
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

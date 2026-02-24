using System.Runtime.InteropServices;
using Photostax.Native;

namespace Photostax;

/// <summary>
/// Represents a local photo repository.
/// </summary>
public sealed class PhotostaxRepository : IDisposable
{
    private readonly RepoSafeHandle _handle;
    private bool _disposed;

    /// <summary>
    /// Initializes a new instance of the <see cref="PhotostaxRepository"/> class.
    /// </summary>
    /// <param name="directoryPath">The path to the repository directory.</param>
    /// <exception cref="ArgumentNullException">Thrown when <paramref name="directoryPath"/> is null.</exception>
    /// <exception cref="PhotostaxException">Thrown when the repository cannot be opened.</exception>
    public PhotostaxRepository(string directoryPath)
    {
        ArgumentNullException.ThrowIfNull(directoryPath);

        var ptr = NativeMethods.photostax_repo_open(directoryPath);
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

    private static PhotoStack ConvertStack(FfiPhotoStack ffi)
    {
        var id = Marshal.PtrToStringUTF8(ffi.Id) ?? throw new PhotostaxException("Stack ID is null");
        var original = ffi.Original != IntPtr.Zero ? Marshal.PtrToStringUTF8(ffi.Original) : null;
        var enhanced = ffi.Enhanced != IntPtr.Zero ? Marshal.PtrToStringUTF8(ffi.Enhanced) : null;
        var back = ffi.Back != IntPtr.Zero ? Marshal.PtrToStringUTF8(ffi.Back) : null;
        var metadataJson = Marshal.PtrToStringUTF8(ffi.MetadataJson) ?? "{}";
        var metadata = Metadata.FromJson(metadataJson);

        return new PhotoStack(id, original, enhanced, back, metadata);
    }
}

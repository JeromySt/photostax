using System.Runtime.InteropServices;
using Photostax.Native;

namespace Photostax;

/// <summary>
/// Provides lazy-loading access to a PhotoStack's metadata (EXIF, XMP, sidecar).
/// </summary>
public sealed class MetadataRef
{
    private readonly PhotoStack _stack;

    internal MetadataRef(PhotoStack stack)
    {
        _stack = stack;
    }

    /// <summary>Gets whether metadata has been loaded from disk.</summary>
    public bool IsLoaded => NativeMethods.photostax_stack_metadata_is_loaded(_stack.Handle);

    /// <summary>Lazily loads and returns the metadata. Cached after first call.</summary>
    /// <exception cref="PhotostaxException">Thrown when metadata loading fails.</exception>
    public Metadata Read()
    {
        _stack.ThrowIfDisposed();
        var ptr = NativeMethods.photostax_stack_metadata_read(_stack.Handle);
        if (ptr == IntPtr.Zero)
            throw new PhotostaxException("Failed to load metadata.");

        try
        {
            var json = Marshal.PtrToStringUTF8(ptr) ?? "{}";
            return Metadata.FromJson(json);
        }
        finally { NativeMethods.photostax_string_free(ptr); }
    }

    /// <summary>Returns cached metadata without loading, or null if not yet loaded.</summary>
    public Metadata? Cached
    {
        get
        {
            _stack.ThrowIfDisposed();
            var ptr = NativeMethods.photostax_stack_metadata_cached(_stack.Handle);
            if (ptr == IntPtr.Zero)
                return null;

            try
            {
                var json = Marshal.PtrToStringUTF8(ptr) ?? "{}";
                return Metadata.FromJson(json);
            }
            finally { NativeMethods.photostax_string_free(ptr); }
        }
    }

    /// <summary>Writes metadata to the sidecar file.</summary>
    /// <param name="metadata">The metadata to write.</param>
    /// <exception cref="ArgumentNullException">Thrown when <paramref name="metadata"/> is null.</exception>
    /// <exception cref="PhotostaxException">Thrown when writing fails.</exception>
    public void Write(Metadata metadata)
    {
        ArgumentNullException.ThrowIfNull(metadata);
        _stack.ThrowIfDisposed();

        var json = metadata.ToJson();
        var result = NativeMethods.photostax_stack_metadata_write(_stack.Handle, json);
        if (!result.Success)
        {
            var msg = result.ErrorMessage != IntPtr.Zero
                ? Marshal.PtrToStringUTF8(result.ErrorMessage) : "Unknown error";
            if (result.ErrorMessage != IntPtr.Zero)
                NativeMethods.photostax_string_free(result.ErrorMessage);
            throw new PhotostaxException(msg!);
        }
    }

    /// <summary>Clears cached metadata, forcing re-load on next Read().</summary>
    public void Invalidate()
    {
        _stack.ThrowIfDisposed();
        NativeMethods.photostax_stack_metadata_invalidate(_stack.Handle);
    }

    /// <summary>
    /// Reads the raw sidecar file bytes without parsing.
    /// </summary>
    /// <remarks>
    /// Returns the unprocessed sidecar content (e.g., XMP XML). Unlike
    /// <see cref="Read"/>, this bypasses all metadata parsing and merging.
    /// Useful when the raw file content is needed for AI ingestion or
    /// external tooling.
    /// </remarks>
    /// <returns>The raw sidecar bytes, or null if no sidecar exists.</returns>
    /// <exception cref="PhotostaxException">Thrown when reading fails.</exception>
    public byte[]? ReadRaw()
    {
        _stack.ThrowIfDisposed();
        var result = NativeMethods.photostax_metadata_read_raw(
            _stack.Handle, out var data, out var len);

        if (!result.Success)
        {
            var msg = result.ErrorMessage != IntPtr.Zero
                ? Marshal.PtrToStringUTF8(result.ErrorMessage) : "Unknown error";
            if (result.ErrorMessage != IntPtr.Zero)
                NativeMethods.photostax_string_free(result.ErrorMessage);
            throw new PhotostaxException(msg!);
        }

        if (data == IntPtr.Zero)
            return null;

        try
        {
            var bytes = new byte[(int)len];
            Marshal.Copy(data, bytes, 0, (int)len);
            return bytes;
        }
        finally
        {
            NativeMethods.photostax_bytes_free(data, len);
        }
    }

    /// <summary>
    /// Opens a read-only stream over the raw sidecar file.
    /// </summary>
    /// <remarks>
    /// Like <see cref="ReadRaw"/> but returns a <see cref="Stream"/> for
    /// streaming access without loading the entire file into memory.
    /// </remarks>
    /// <returns>A stream over the sidecar bytes, or null if no sidecar exists.</returns>
    /// <exception cref="PhotostaxException">Thrown when reading fails.</exception>
    public Stream? ReadRawStream()
    {
        var bytes = ReadRaw();
        return bytes != null ? new MemoryStream(bytes, writable: false) : null;
    }
}

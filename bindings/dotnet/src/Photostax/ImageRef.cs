using System.Runtime.InteropServices;
using Photostax.Native;

namespace Photostax;

/// <summary>
/// Provides access to a specific image variant (original, enhanced, or back)
/// within a PhotoStack. All operations delegate through FFI to the Rust core.
/// </summary>
public sealed class ImageRef
{
    private readonly PhotoStack _stack;
    private readonly int _variant;

    internal ImageRef(PhotoStack stack, int variant)
    {
        _stack = stack;
        _variant = variant;
    }

    /// <summary>Gets whether this image variant exists in the stack.</summary>
    public bool IsPresent => NativeMethods.photostax_stack_image_is_present(_stack.Handle, _variant);

    /// <summary>Gets whether the underlying image handle is still valid.</summary>
    public bool IsValid => NativeMethods.photostax_stack_image_is_valid(_stack.Handle, _variant);

    /// <summary>Gets the file size in bytes, or null if absent.</summary>
    public long? Size
    {
        get
        {
            var size = NativeMethods.photostax_stack_image_size(_stack.Handle, _variant);
            return size >= 0 ? size : null;
        }
    }

    /// <summary>Reads the full image data into a byte array.</summary>
    /// <exception cref="InvalidOperationException">Thrown when the variant is not present.</exception>
    /// <exception cref="PhotostaxException">Thrown when reading fails.</exception>
    public byte[] Read()
    {
        _stack.ThrowIfDisposed();
        if (!IsPresent)
            throw new InvalidOperationException($"Image variant {VariantName} is not present.");

        var result = NativeMethods.photostax_stack_image_read(
            _stack.Handle, _variant, out var dataPtr, out var len);

        if (!result.Success)
        {
            var msg = result.ErrorMessage != IntPtr.Zero
                ? Marshal.PtrToStringUTF8(result.ErrorMessage) : "Unknown error";
            if (result.ErrorMessage != IntPtr.Zero)
                NativeMethods.photostax_string_free(result.ErrorMessage);
            throw new PhotostaxException(msg!);
        }

        try
        {
            var bytes = new byte[len];
            Marshal.Copy(dataPtr, bytes, 0, (int)len);
            return bytes;
        }
        finally
        {
            NativeMethods.photostax_bytes_free(dataPtr, len);
        }
    }

    /// <summary>Computes or returns the cached SHA-256 hash of the image.</summary>
    /// <exception cref="InvalidOperationException">Thrown when the variant is not present.</exception>
    /// <exception cref="PhotostaxException">Thrown when hash computation fails.</exception>
    public string Hash()
    {
        _stack.ThrowIfDisposed();
        if (!IsPresent)
            throw new InvalidOperationException($"Image variant {VariantName} is not present.");

        var ptr = NativeMethods.photostax_stack_image_hash(_stack.Handle, _variant);
        if (ptr == IntPtr.Zero)
            throw new PhotostaxException($"Failed to compute hash for {VariantName} image.");

        try { return Marshal.PtrToStringUTF8(ptr)!; }
        finally { NativeMethods.photostax_string_free(ptr); }
    }

    /// <summary>Returns the image dimensions (width, height). Cached after first call.</summary>
    /// <exception cref="InvalidOperationException">Thrown when the variant is not present.</exception>
    /// <exception cref="PhotostaxException">Thrown when dimensions cannot be read.</exception>
    public (uint Width, uint Height) Dimensions()
    {
        _stack.ThrowIfDisposed();
        if (!IsPresent)
            throw new InvalidOperationException($"Image variant {VariantName} is not present.");

        var dims = NativeMethods.photostax_stack_image_dimensions(_stack.Handle, _variant);
        if (!dims.Success)
            throw new PhotostaxException($"Failed to get dimensions for {VariantName} image.");

        return (dims.Width, dims.Height);
    }

    /// <summary>Rotates the image on disk by the given degrees.</summary>
    /// <param name="degrees">Rotation in degrees: 90, -90, 180, -180, or 270.</param>
    /// <exception cref="InvalidOperationException">Thrown when the variant is not present.</exception>
    /// <exception cref="PhotostaxException">Thrown when rotation fails.</exception>
    public void Rotate(int degrees)
    {
        _stack.ThrowIfDisposed();
        if (!IsPresent)
            throw new InvalidOperationException($"Image variant {VariantName} is not present.");

        var result = NativeMethods.photostax_stack_image_rotate(_stack.Handle, _variant, degrees);
        if (!result.Success)
        {
            var msg = result.ErrorMessage != IntPtr.Zero
                ? Marshal.PtrToStringUTF8(result.ErrorMessage) : "Unknown error";
            if (result.ErrorMessage != IntPtr.Zero)
                NativeMethods.photostax_string_free(result.ErrorMessage);
            throw new PhotostaxException(msg!);
        }
    }

    /// <summary>Clears cached hash and dimensions, forcing re-computation on next access.</summary>
    public void InvalidateCaches()
    {
        _stack.ThrowIfDisposed();
        NativeMethods.photostax_stack_image_invalidate(_stack.Handle, _variant);
    }

    private string VariantName => _variant switch
    {
        0 => "original",
        1 => "enhanced",
        2 => "back",
        _ => "unknown"
    };
}

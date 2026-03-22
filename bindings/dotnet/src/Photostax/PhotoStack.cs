using System.Runtime.InteropServices;
using Photostax.Native;

namespace Photostax;

/// <summary>
/// Represents a photo stack with its associated images and metadata.
/// Each instance wraps an opaque native handle. Call Dispose() to free.
/// </summary>
public sealed class PhotoStack : IDisposable
{
    internal IntPtr Handle { get; private set; }

    /// <summary>Gets the original (raw scan) image accessor.</summary>
    public ImageRef Original { get; }

    /// <summary>Gets the enhanced (color-corrected) image accessor.</summary>
    public ImageRef Enhanced { get; }

    /// <summary>Gets the back-of-photo image accessor.</summary>
    public ImageRef Back { get; }

    /// <summary>Gets the metadata accessor (lazy-loading).</summary>
    public MetadataRef Metadata { get; }

    /// <summary>Gets the unique identifier (opaque SHA-256 hash).</summary>
    public string Id { get; }

    /// <summary>Gets the human-readable display name.</summary>
    public string Name { get; }

    /// <summary>Gets the subfolder within the repository, or null if root-level.</summary>
    public string? Folder { get; }

    /// <summary>Gets which image variants are present as a flags enum.</summary>
    public ImageVariants ImagesPresent
    {
        get
        {
            ThrowIfDisposed();
            var flags = ImageVariants.None;
            if (Original.IsPresent) flags |= ImageVariants.Original;
            if (Enhanced.IsPresent) flags |= ImageVariants.Enhanced;
            if (Back.IsPresent) flags |= ImageVariants.Back;
            return flags;
        }
    }

    /// <summary>Gets whether this stack has any image variant present.</summary>
    public bool HasAnyImage => ImagesPresent != ImageVariants.None;

    internal PhotoStack(IntPtr handle)
    {
        Handle = handle != IntPtr.Zero ? handle
            : throw new ArgumentException("Handle cannot be zero.", nameof(handle));

        // Cache immutable properties eagerly (avoids repeated FFI calls)
        Id = ReadString(NativeMethods.photostax_stack_id(handle)) ?? string.Empty;
        Name = ReadString(NativeMethods.photostax_stack_name(handle)) ?? Id;
        Folder = ReadString(NativeMethods.photostax_stack_folder(handle));

        Original = new ImageRef(this, 0);
        Enhanced = new ImageRef(this, 1);
        Back = new ImageRef(this, 2);
        Metadata = new MetadataRef(this);
    }

    private static string? ReadString(IntPtr ptr)
    {
        if (ptr == IntPtr.Zero) return null;
        try { return Marshal.PtrToStringUTF8(ptr); }
        finally { NativeMethods.photostax_string_free(ptr); }
    }

    /// <summary>
    /// Swaps front and back images when a photo was accidentally scanned backwards.
    /// After swap: original ← old back, back ← old original, enhanced ← deleted.
    /// Files are renamed on disk to match their new roles.
    /// </summary>
    /// <exception cref="PhotostaxException">Thrown when the swap fails (e.g., no back image).</exception>
    public void SwapFrontBack()
    {
        ThrowIfDisposed();

        var result = NativeMethods.photostax_stack_swap_front_back(Handle);
        if (!result.Success)
        {
            var msg = result.ErrorMessage != IntPtr.Zero
                ? Marshal.PtrToStringUTF8(result.ErrorMessage) : "Unknown error";
            if (result.ErrorMessage != IntPtr.Zero)
                NativeMethods.photostax_string_free(result.ErrorMessage);
            throw new PhotostaxException(msg!);
        }
    }

    /// <summary>
    /// Rotates images in this stack by the given number of degrees.
    /// </summary>
    /// <param name="degrees">Rotation in degrees: 90, -90, 180, -180, or 270.</param>
    /// <param name="target">Which images to rotate (default: all).</param>
    /// <exception cref="ArgumentException">Thrown for invalid degree values.</exception>
    /// <exception cref="PhotostaxException">Thrown when rotation fails.</exception>
    public void Rotate(int degrees, RotationTarget target = RotationTarget.All)
    {
        ThrowIfDisposed();

        if (degrees != 90 && degrees != -90 && degrees != 180 && degrees != -180 && degrees != 270)
            throw new ArgumentException(
                $"Invalid rotation: {degrees}°. Accepted values: 90, -90, 180, -180.",
                nameof(degrees));

        var variants = target switch
        {
            RotationTarget.Front => new[] { 0, 1 },
            RotationTarget.Back => new[] { 2 },
            _ => new[] { 0, 1, 2 },
        };

        foreach (var variant in variants)
        {
            if (!NativeMethods.photostax_stack_image_is_present(Handle, variant))
                continue;

            var result = NativeMethods.photostax_stack_image_rotate(Handle, variant, degrees);
            if (!result.Success)
            {
                var msg = result.ErrorMessage != IntPtr.Zero
                    ? Marshal.PtrToStringUTF8(result.ErrorMessage) : "Unknown error";
                if (result.ErrorMessage != IntPtr.Zero)
                    NativeMethods.photostax_string_free(result.ErrorMessage);
                throw new PhotostaxException(msg!);
            }
        }
    }

    internal void ThrowIfDisposed()
    {
        ObjectDisposedException.ThrowIf(Handle == IntPtr.Zero, this);
    }

    /// <inheritdoc />
    public void Dispose()
    {
        if (Handle != IntPtr.Zero)
        {
            NativeMethods.photostax_stack_free(Handle);
            Handle = IntPtr.Zero;
        }
    }

    // ── Static conversion helpers ──────────────────────────────────

    /// <summary>
    /// Convert an FfiStackHandleArray to a list of PhotoStacks.
    /// Takes ownership of individual handles (nulls them in the array so the
    /// subsequent array-free only releases the container).
    /// </summary>
    internal static IReadOnlyList<PhotoStack> ConvertHandleArray(FfiStackHandleArray array)
    {
        if (array.Handles == IntPtr.Zero || array.Len == 0)
            return [];

        var stacks = new List<PhotoStack>((int)array.Len);
        var ptrSize = IntPtr.Size;

        for (nuint i = 0; i < array.Len; i++)
        {
            var offset = (int)i * ptrSize;
            var handlePtr = Marshal.ReadIntPtr(array.Handles, offset);
            if (handlePtr != IntPtr.Zero)
            {
                stacks.Add(new PhotoStack(handlePtr));
                // Null out so the array free won't double-free this handle
                Marshal.WriteIntPtr(array.Handles, offset, IntPtr.Zero);
            }
        }

        return stacks;
    }

    /// <summary>
    /// Convert an FfiPaginatedHandleResult to a flat list, discarding pagination metadata.
    /// Takes ownership of individual handles.
    /// </summary>
    internal static IReadOnlyList<PhotoStack> ConvertPaginatedHandleResultToList(FfiPaginatedHandleResult result)
    {
        var items = new List<PhotoStack>();

        if (result.Handles != IntPtr.Zero && result.Len > 0)
        {
            var ptrSize = IntPtr.Size;
            for (nuint i = 0; i < result.Len; i++)
            {
                var offset = (int)i * ptrSize;
                var handlePtr = Marshal.ReadIntPtr(result.Handles, offset);
                if (handlePtr != IntPtr.Zero)
                {
                    items.Add(new PhotoStack(handlePtr));
                    Marshal.WriteIntPtr(result.Handles, offset, IntPtr.Zero);
                }
            }
        }

        return items.AsReadOnly();
    }

    /// <summary>
    /// Convert an FfiPaginatedHandleResult to a PaginatedResult.
    /// Takes ownership of individual handles.
    /// </summary>
    internal static PaginatedResult<PhotoStack> ConvertPaginatedHandleResult(FfiPaginatedHandleResult result)
    {
        var items = new List<PhotoStack>();

        if (result.Handles != IntPtr.Zero && result.Len > 0)
        {
            var ptrSize = IntPtr.Size;
            for (nuint i = 0; i < result.Len; i++)
            {
                var offset = (int)i * ptrSize;
                var handlePtr = Marshal.ReadIntPtr(result.Handles, offset);
                if (handlePtr != IntPtr.Zero)
                {
                    items.Add(new PhotoStack(handlePtr));
                    Marshal.WriteIntPtr(result.Handles, offset, IntPtr.Zero);
                }
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

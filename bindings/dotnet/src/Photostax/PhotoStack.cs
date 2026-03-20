using System.Runtime.InteropServices;
using Photostax.Native;

namespace Photostax;

/// <summary>
/// Represents a photo stack with its associated images and metadata.
/// Operations like loading metadata, writing metadata, rotating, and reading
/// images are available directly on the stack object.
/// </summary>
public sealed class PhotoStack
{
    private readonly IntPtr _managerHandle;

    /// <summary>
    /// Gets the unique identifier for this stack (opaque SHA-256 hash).
    /// </summary>
    public string Id { get; }

    /// <summary>
    /// Gets the human-readable display name (file stem, e.g. "IMG_001").
    /// </summary>
    public string Name { get; }

    /// <summary>
    /// Gets the subfolder within the repository, or null if root-level.
    /// </summary>
    public string? Folder { get; }

    /// <summary>
    /// Gets the path to the original image, or null if not present.
    /// </summary>
    public string? OriginalPath { get; }

    /// <summary>
    /// Gets the path to the enhanced image, or null if not present.
    /// </summary>
    public string? EnhancedPath { get; }

    /// <summary>
    /// Gets the path to the back image, or null if not present.
    /// </summary>
    public string? BackPath { get; }

    /// <summary>
    /// Gets the metadata associated with this stack.
    /// </summary>
    public Metadata Metadata { get; }

    /// <summary>
    /// Gets a value indicating whether this stack has any image.
    /// </summary>
    public bool HasAnyImage => OriginalPath != null || EnhancedPath != null || BackPath != null;

    /// <summary>
    /// Gets the image format, or null if no images are present.
    /// </summary>
    public ImageFormat? Format
    {
        get
        {
            var path = OriginalPath ?? EnhancedPath ?? BackPath;
            if (path == null)
                return null;

            var extension = Path.GetExtension(path).ToLowerInvariant();
            return extension switch
            {
                ".jpg" or ".jpeg" => ImageFormat.Jpeg,
                ".png" => ImageFormat.Png,
                ".tif" or ".tiff" => ImageFormat.Tiff,
                _ => ImageFormat.Unknown
            };
        }
    }

    /// <summary>
    /// Initializes a new instance of the <see cref="PhotoStack"/> class.
    /// </summary>
    internal PhotoStack(
        IntPtr managerHandle,
        string id,
        string name,
        string? folder,
        string? originalPath,
        string? enhancedPath,
        string? backPath,
        Metadata metadata)
    {
        _managerHandle = managerHandle;
        Id = id ?? throw new ArgumentNullException(nameof(id));
        Name = name ?? id;
        Folder = folder;
        OriginalPath = originalPath;
        EnhancedPath = enhancedPath;
        BackPath = backPath;
        Metadata = metadata ?? throw new ArgumentNullException(nameof(metadata));
    }

    /// <summary>
    /// Loads full metadata (EXIF, XMP, sidecar) for this stack on demand.
    /// </summary>
    /// <returns>The loaded metadata.</returns>
    /// <exception cref="PhotostaxException">Thrown when metadata loading fails.</exception>
    public Metadata LoadMetadata()
    {
        ThrowIfInvalid();

        var ptr = NativeMethods.photostax_stack_load_metadata(_managerHandle, Id);
        if (ptr == IntPtr.Zero)
            throw new PhotostaxException($"Failed to load metadata for stack '{Id}'");

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
    /// Writes metadata to this photo stack.
    /// </summary>
    /// <param name="metadata">The metadata to write.</param>
    /// <exception cref="PhotostaxException">Thrown when writing fails.</exception>
    public void WriteMetadata(Metadata metadata)
    {
        ArgumentNullException.ThrowIfNull(metadata);
        ThrowIfInvalid();

        var json = metadata.ToJson();
        var result = NativeMethods.photostax_write_metadata(_managerHandle, Id, json);

        if (!result.Success)
        {
            var errorMessage = result.ErrorMessage != IntPtr.Zero
                ? Marshal.PtrToStringUTF8(result.ErrorMessage)
                : null;
            if (result.ErrorMessage != IntPtr.Zero)
                NativeMethods.photostax_string_free(result.ErrorMessage);
            throw new PhotostaxException(
                errorMessage ?? $"Failed to write metadata for stack '{Id}'");
        }
    }

    /// <summary>
    /// Rotates images in this stack by the given number of degrees.
    /// </summary>
    /// <param name="degrees">Rotation in degrees: 90, -90, 180, or -180.</param>
    /// <param name="target">Which images to rotate (default: all).</param>
    /// <returns>A new <see cref="PhotoStack"/> reflecting the rotated state.</returns>
    /// <exception cref="ArgumentException">Thrown for invalid degree values.</exception>
    /// <exception cref="PhotostaxException">Thrown when rotation fails.</exception>
    public PhotoStack Rotate(int degrees, RotationTarget target = RotationTarget.All)
    {
        ThrowIfInvalid();

        if (degrees != 90 && degrees != -90 && degrees != 180 && degrees != -180 && degrees != 270)
            throw new ArgumentException(
                $"Invalid rotation: {degrees}°. Accepted values: 90, -90, 180, -180.",
                nameof(degrees));

        var ptr = NativeMethods.photostax_rotate_stack(_managerHandle, Id, degrees, (int)target);
        if (ptr == IntPtr.Zero)
            throw new PhotostaxException($"Failed to rotate stack '{Id}' by {degrees}°");

        using var stackHandle = StackSafeHandle.FromPointer(ptr);
        var ffi = Marshal.PtrToStructure<FfiPhotoStack>(ptr);
        return ConvertStack(_managerHandle, ffi);
    }

    /// <summary>
    /// Reads the raw bytes of the original scan image.
    /// </summary>
    /// <returns>The image data as a byte array.</returns>
    /// <exception cref="InvalidOperationException">Thrown when no original image exists.</exception>
    /// <exception cref="PhotostaxException">Thrown when reading fails.</exception>
    public byte[] ReadOriginalImage()
    {
        if (OriginalPath == null)
            throw new InvalidOperationException("This stack has no original image.");
        return ReadImageInternal(OriginalPath);
    }

    /// <summary>
    /// Reads the raw bytes of the enhanced (color-corrected) scan image.
    /// </summary>
    /// <returns>The image data as a byte array.</returns>
    /// <exception cref="InvalidOperationException">Thrown when no enhanced image exists.</exception>
    /// <exception cref="PhotostaxException">Thrown when reading fails.</exception>
    public byte[] ReadEnhancedImage()
    {
        if (EnhancedPath == null)
            throw new InvalidOperationException("This stack has no enhanced image.");
        return ReadImageInternal(EnhancedPath);
    }

    /// <summary>
    /// Reads the raw bytes of the back-of-photo scan image.
    /// </summary>
    /// <returns>The image data as a byte array.</returns>
    /// <exception cref="InvalidOperationException">Thrown when no back image exists.</exception>
    /// <exception cref="PhotostaxException">Thrown when reading fails.</exception>
    public byte[] ReadBackImage()
    {
        if (BackPath == null)
            throw new InvalidOperationException("This stack has no back image.");
        return ReadImageInternal(BackPath);
    }

    private byte[] ReadImageInternal(string path)
    {
        ThrowIfInvalid();

        var result = NativeMethods.photostax_read_image(
            _managerHandle, path, out var dataPtr, out var len);

        if (!result.Success)
        {
            var errorMessage = result.ErrorMessage != IntPtr.Zero
                ? Marshal.PtrToStringUTF8(result.ErrorMessage)
                : null;
            if (result.ErrorMessage != IntPtr.Zero)
                NativeMethods.photostax_string_free(result.ErrorMessage);
            throw new PhotostaxException(errorMessage ?? $"Failed to read image at '{path}'");
        }

        using var bytesHandle = BytesSafeHandle.FromPointer(dataPtr, len);
        return bytesHandle.ToArray();
    }

    private void ThrowIfInvalid()
    {
        if (_managerHandle == IntPtr.Zero)
            throw new ObjectDisposedException(nameof(PhotoStack),
                "The underlying manager has been disposed.");
    }

    // ── Static conversion helpers ──────────────────────────────────

    internal static PhotoStack ConvertStack(IntPtr managerHandle, FfiPhotoStack ffi)
    {
        var id = Marshal.PtrToStringUTF8(ffi.Id) ?? throw new PhotostaxException("Stack ID is null");
        var name = ffi.Name != IntPtr.Zero ? Marshal.PtrToStringUTF8(ffi.Name) ?? id : id;
        var folder = ffi.Folder != IntPtr.Zero ? Marshal.PtrToStringUTF8(ffi.Folder) : null;
        var original = ffi.Original != IntPtr.Zero ? Marshal.PtrToStringUTF8(ffi.Original) : null;
        var enhanced = ffi.Enhanced != IntPtr.Zero ? Marshal.PtrToStringUTF8(ffi.Enhanced) : null;
        var back = ffi.Back != IntPtr.Zero ? Marshal.PtrToStringUTF8(ffi.Back) : null;
        var metadataJson = Marshal.PtrToStringUTF8(ffi.MetadataJson) ?? "{}";
        var metadata = Metadata.FromJson(metadataJson);

        return new PhotoStack(managerHandle, id, name, folder, original, enhanced, back, metadata);
    }

    internal static IReadOnlyList<PhotoStack> ConvertStackArray(IntPtr managerHandle, FfiPhotoStackArray array)
    {
        if (array.Data == IntPtr.Zero || array.Len == 0)
            return [];

        var stacks = new List<PhotoStack>((int)array.Len);
        var structSize = Marshal.SizeOf<FfiPhotoStack>();

        for (nuint i = 0; i < array.Len; i++)
        {
            var stackPtr = IntPtr.Add(array.Data, (int)i * structSize);
            var ffiStack = Marshal.PtrToStructure<FfiPhotoStack>(stackPtr);
            stacks.Add(ConvertStack(managerHandle, ffiStack));
        }

        return stacks;
    }

    internal static PaginatedResult<PhotoStack> ConvertPaginatedResult(IntPtr managerHandle, FfiPaginatedResult result)
    {
        var items = new List<PhotoStack>();

        if (result.Data != IntPtr.Zero && result.Len > 0)
        {
            var structSize = Marshal.SizeOf<FfiPhotoStack>();
            for (nuint i = 0; i < result.Len; i++)
            {
                var stackPtr = IntPtr.Add(result.Data, (int)i * structSize);
                var ffiStack = Marshal.PtrToStructure<FfiPhotoStack>(stackPtr);
                items.Add(ConvertStack(managerHandle, ffiStack));
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

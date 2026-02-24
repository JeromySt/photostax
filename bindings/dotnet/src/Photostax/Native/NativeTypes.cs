using System.Runtime.InteropServices;

namespace Photostax.Native;

/// <summary>
/// FFI result type for operations that may fail.
/// </summary>
[StructLayout(LayoutKind.Sequential)]
internal struct FfiResult
{
    /// <summary>
    /// True if the operation succeeded.
    /// </summary>
    [MarshalAs(UnmanagedType.I1)]
    public bool Success;

    /// <summary>
    /// Error message (null on success, must be freed on failure).
    /// </summary>
    public IntPtr ErrorMessage;
}

/// <summary>
/// A photo stack returned across FFI.
/// </summary>
[StructLayout(LayoutKind.Sequential)]
internal struct FfiPhotoStack
{
    /// <summary>
    /// Stack identifier (never null).
    /// </summary>
    public IntPtr Id;

    /// <summary>
    /// Path to original image (null if absent).
    /// </summary>
    public IntPtr Original;

    /// <summary>
    /// Path to enhanced image (null if absent).
    /// </summary>
    public IntPtr Enhanced;

    /// <summary>
    /// Path to back image (null if absent).
    /// </summary>
    public IntPtr Back;

    /// <summary>
    /// JSON-serialized metadata (never null, may be "{}").
    /// </summary>
    public IntPtr MetadataJson;
}

/// <summary>
/// Array of photo stacks from FFI.
/// </summary>
[StructLayout(LayoutKind.Sequential)]
internal struct FfiPhotoStackArray
{
    /// <summary>
    /// Pointer to array of stacks (null if len == 0).
    /// </summary>
    public IntPtr Data;

    /// <summary>
    /// Number of stacks in the array.
    /// </summary>
    public nuint Len;
}

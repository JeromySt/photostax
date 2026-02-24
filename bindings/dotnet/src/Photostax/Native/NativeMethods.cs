using System.Runtime.InteropServices;

namespace Photostax.Native;

/// <summary>
/// P/Invoke declarations for the photostax_ffi native library.
/// </summary>
internal static partial class NativeMethods
{
    private const string LibName = "photostax_ffi";

    /// <summary>
    /// Create a new repository from a directory path.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr photostax_repo_open(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string path);

    /// <summary>
    /// Free a repository handle.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void photostax_repo_free(IntPtr repo);

    /// <summary>
    /// Scan the repository and return all photo stacks.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern FfiPhotoStackArray photostax_repo_scan(IntPtr repo);

    /// <summary>
    /// Get a single stack by ID.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr photostax_repo_get_stack(
        IntPtr repo,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string id);

    /// <summary>
    /// Read image bytes.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern FfiResult photostax_read_image(
        IntPtr repo,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string path,
        out IntPtr outData,
        out nuint outLen);

    /// <summary>
    /// Write metadata to a stack.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern FfiResult photostax_write_metadata(
        IntPtr repo,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string stackId,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string metadataJson);

    /// <summary>
    /// Search/filter stacks.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern FfiPhotoStackArray photostax_search(
        IntPtr repo,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string queryJson);

    /// <summary>
    /// Get metadata for a stack as a JSON string.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr photostax_get_metadata(
        IntPtr repo,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string stackId);

    /// <summary>
    /// Get a specific EXIF tag value.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr photostax_get_exif_tag(
        IntPtr repo,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string stackId,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string tagName);

    /// <summary>
    /// Get a specific custom tag value as JSON.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr photostax_get_custom_tag(
        IntPtr repo,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string stackId,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string tagName);

    /// <summary>
    /// Set a custom tag value.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern FfiResult photostax_set_custom_tag(
        IntPtr repo,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string stackId,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string tagName,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string valueJson);

    /// <summary>
    /// Free a photo stack array.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void photostax_stack_array_free(FfiPhotoStackArray array);

    /// <summary>
    /// Free a single photo stack.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void photostax_stack_free(IntPtr stack);

    /// <summary>
    /// Free a string allocated by photostax.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void photostax_string_free(IntPtr s);

    /// <summary>
    /// Free a byte buffer allocated by photostax.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void photostax_bytes_free(IntPtr data, nuint len);
}

using System.Diagnostics.CodeAnalysis;
using System.Reflection;
using System.Runtime.InteropServices;

namespace Photostax.Native;

/// <summary>
/// P/Invoke declarations for the photostax_ffi native library.
/// </summary>
[ExcludeFromCodeCoverage]
internal static partial class NativeMethods
{
    private const string LibName = "photostax_ffi";

    /// <summary>
    /// Registers a custom native library resolver that probes
    /// <c>runtimes/{rid}/native/</c> next to the assembly, matching the
    /// NuGet package layout.  This ensures the library is found both when
    /// consumed via NuGet and when referenced as a project.
    /// </summary>
    static NativeMethods()
    {
        NativeLibrary.SetDllImportResolver(
            typeof(NativeMethods).Assembly,
            ResolveDllImport);
    }

    private static IntPtr ResolveDllImport(
        string libraryName, Assembly assembly, DllImportSearchPath? searchPath)
    {
        if (libraryName != LibName)
            return IntPtr.Zero;

        // 1. Let the default resolver try first (handles PATH, LD_LIBRARY_PATH, etc.)
        if (NativeLibrary.TryLoad(libraryName, assembly, searchPath, out var handle))
            return handle;

        // 2. Probe runtimes/<rid>/native/ next to the managed assembly
        var assemblyDir = Path.GetDirectoryName(assembly.Location) ?? ".";
        var rid = RuntimeInformation.RuntimeIdentifier;

        var candidate = Path.Combine(assemblyDir, "runtimes", rid, "native", MapLibraryName(libraryName));
        if (NativeLibrary.TryLoad(candidate, out handle))
            return handle;

        // 3. Try the base RID (e.g. win-x64 when running as win10-x64)
        var baseRid = SimplifyRid(rid);
        if (baseRid != rid)
        {
            candidate = Path.Combine(assemblyDir, "runtimes", baseRid, "native", MapLibraryName(libraryName));
            if (NativeLibrary.TryLoad(candidate, out handle))
                return handle;
        }

        return IntPtr.Zero;
    }

    /// <summary>
    /// Maps a logical library name to the platform-specific filename.
    /// </summary>
    private static string MapLibraryName(string name)
    {
        if (RuntimeInformation.IsOSPlatform(OSPlatform.Windows))
            return name + ".dll";
        if (RuntimeInformation.IsOSPlatform(OSPlatform.OSX))
            return "lib" + name + ".dylib";
        return "lib" + name + ".so";
    }

    /// <summary>
    /// Strips version qualifiers from a RID (e.g. "win10-x64" → "win-x64").
    /// </summary>
    private static string SimplifyRid(string rid)
    {
        // RIDs like "win10-x64", "ubuntu.22.04-x64" etc.  Strip to base.
        var dash = rid.IndexOf('-');
        if (dash < 0) return rid;

        var os = rid[..dash];
        var arch = rid[(dash + 1)..];

        // Remove trailing digits/dots from the OS part
        var baseOs = new string(os.TakeWhile(c => char.IsLetter(c)).ToArray());
        return baseOs.Length > 0 ? $"{baseOs}-{arch}" : rid;
    }

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

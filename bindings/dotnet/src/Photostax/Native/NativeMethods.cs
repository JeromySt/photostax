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
    /// Create a new repository with recursive subdirectory scanning.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr photostax_repo_open_recursive(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string path,
        [MarshalAs(UnmanagedType.U1)] bool recursive);

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
    /// Scan progress callback delegate matching the native function pointer signature.
    /// </summary>
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    internal delegate void ScanProgressCallback(int phase, nuint current, nuint total, IntPtr userData);

    /// <summary>
    /// Scan with a scanner profile and optional progress callback.
    /// Profile: 0=Auto, 1=EnhancedAndBack, 2=EnhancedOnly, 3=OriginalOnly.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern FfiPhotoStackArray photostax_repo_scan_with_progress(
        IntPtr repo,
        int profile,
        ScanProgressCallback? callback,
        IntPtr userData);

    /// <summary>
    /// Get a single stack by ID.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr photostax_repo_get_stack(
        IntPtr repo,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string id);

    /// <summary>
    /// Load full metadata (EXIF, XMP, sidecar) for a specific stack and return
    /// the result as a JSON string. Returns IntPtr.Zero on error.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr photostax_stack_load_metadata(
        IntPtr repo,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string stackId);

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
    /// Scan the repository and return a paginated result.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern FfiPaginatedResult photostax_repo_scan_paginated(
        IntPtr repo,
        nuint offset,
        nuint limit,
        [MarshalAs(UnmanagedType.U1)] bool loadMetadata);

    /// <summary>
    /// Search/filter stacks with pagination.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern FfiPaginatedResult photostax_search_paginated(
        IntPtr repo,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string queryJson,
        nuint offset,
        nuint limit);

    /// <summary>
    /// Free a paginated result.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void photostax_paginated_result_free(FfiPaginatedResult result);

    /// <summary>
    /// Unified query: search + paginate the cache in a single call.
    /// query_json may be null (match all), limit 0 = return all.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern FfiPaginatedResult photostax_query(
        IntPtr repo,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? queryJson,
        nuint offset,
        nuint limit);

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

    /// <summary>
    /// Rotate images in a photo stack by the given degrees.
    /// Target: 0 = all, 1 = front only, 2 = back only.
    /// Returns a pointer to the updated FfiPhotoStack, or IntPtr.Zero on error.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr photostax_rotate_stack(
        IntPtr repo,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string stackId,
        int degrees,
        int target);

    // ── Snapshot functions ──────────────────────────────────────

    /// <summary>
    /// Create a point-in-time snapshot for consistent pagination.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr photostax_create_snapshot(
        IntPtr repo,
        [MarshalAs(UnmanagedType.U1)] bool loadMetadata);

    /// <summary>
    /// Create a snapshot with scanner profile and progress callback.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr photostax_create_snapshot_with_progress(
        IntPtr repo,
        int profile,
        [MarshalAs(UnmanagedType.U1)] bool loadMetadata,
        ScanProgressCallback? callback,
        IntPtr userData);

    /// <summary>
    /// Get the total number of stacks in the snapshot.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern nuint photostax_snapshot_total_count(IntPtr snapshot);

    /// <summary>
    /// Get a page of stacks from the snapshot.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern FfiPaginatedResult photostax_snapshot_get_page(
        IntPtr snapshot,
        nuint offset,
        nuint limit);

    /// <summary>
    /// Check whether a snapshot is still current.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern FfiSnapshotStatus photostax_snapshot_check_status(
        IntPtr repo,
        IntPtr snapshot);

    /// <summary>
    /// Create a filtered snapshot from an existing one.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr photostax_snapshot_filter(
        IntPtr snapshot,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string queryJson);

    /// <summary>
    /// Free a snapshot handle.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void photostax_snapshot_free(IntPtr snapshot);
}

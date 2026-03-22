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
    /// Create an empty StackManager with no repositories.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr photostax_manager_new();

    /// <summary>
    /// Add a repository directory to a StackManager.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern FfiResult photostax_manager_add_repo(
        IntPtr mgr,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string path,
        [MarshalAs(UnmanagedType.U1)] bool recursive,
        int profile);

    /// <summary>
    /// Return the number of repositories registered with a StackManager.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern nuint photostax_manager_repo_count(IntPtr mgr);

    /// <summary>
    /// Free a repository handle.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void photostax_repo_free(IntPtr repo);

    // ── Scanning & collection functions ──────────────────────────

    /// <summary>
    /// Scan the repository and return all photo stacks as opaque handles.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern FfiStackHandleArray photostax_repo_scan(IntPtr repo);

    /// <summary>
    /// Scan progress callback delegate matching the native function pointer signature.
    /// </summary>
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    internal delegate void ScanProgressCallback(int phase, nuint current, nuint total, IntPtr userData);

    /// <summary>
    /// Scan with a scanner profile and optional progress callback.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern FfiStackHandleArray photostax_repo_scan_with_progress(
        IntPtr repo,
        int profile,
        ScanProgressCallback? callback,
        IntPtr userData);

    /// <summary>
    /// Get a single stack by ID. Returns null if not found.
    /// Caller owns the returned handle and must call photostax_stack_free.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr photostax_repo_get_stack(
        IntPtr repo,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string id);

    /// <summary>
    /// Search/filter stacks, returning opaque handles.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern FfiStackHandleArray photostax_search(
        IntPtr repo,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string queryJson);

    /// <summary>
    /// Scan the repository and return a paginated handle result.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern FfiPaginatedHandleResult photostax_repo_scan_paginated(
        IntPtr repo,
        nuint offset,
        nuint limit,
        [MarshalAs(UnmanagedType.U1)] bool loadMetadata);

    /// <summary>
    /// Search/filter stacks with pagination.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern FfiPaginatedHandleResult photostax_search_paginated(
        IntPtr repo,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string queryJson,
        nuint offset,
        nuint limit);

    /// <summary>
    /// Unified query: search + paginate the cache in a single call.
    /// query_json may be null (match all), limit 0 = return all.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern FfiPaginatedHandleResult photostax_query(
        IntPtr repo,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? queryJson,
        nuint offset,
        nuint limit,
        ScanProgressCallback? callback,
        IntPtr userData);

    // ── Stack accessor functions ──────────────────────────────────

    /// <summary>
    /// Get the stack ID. Caller must free with photostax_string_free.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr photostax_stack_id(IntPtr stack);

    /// <summary>
    /// Get the stack display name. Caller must free with photostax_string_free.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr photostax_stack_name(IntPtr stack);

    /// <summary>
    /// Get the stack subfolder (null if root). Caller must free with photostax_string_free.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr photostax_stack_folder(IntPtr stack);

    // ── Image variant functions ───────────────────────────────────

    /// <summary>
    /// Check whether an image variant is present in the stack.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    [return: MarshalAs(UnmanagedType.U1)]
    internal static extern bool photostax_stack_image_is_present(IntPtr stack, int variant);

    /// <summary>
    /// Check whether an image variant handle is still valid.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    [return: MarshalAs(UnmanagedType.U1)]
    internal static extern bool photostax_stack_image_is_valid(IntPtr stack, int variant);

    /// <summary>
    /// Get the file size of an image variant. Returns -1 if absent.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern long photostax_stack_image_size(IntPtr stack, int variant);

    /// <summary>
    /// Read the full image data for a variant.
    /// Caller must free data with photostax_bytes_free.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern FfiResult photostax_stack_image_read(
        IntPtr stack,
        int variant,
        out IntPtr outData,
        out nuint outLen);

    /// <summary>
    /// Compute/retrieve the cached SHA-256 hash for an image variant.
    /// Returns null on error. Caller must free with photostax_string_free.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr photostax_stack_image_hash(IntPtr stack, int variant);

    /// <summary>
    /// Get image dimensions for a variant.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern FfiDimensions photostax_stack_image_dimensions(IntPtr stack, int variant);

    /// <summary>
    /// Rotate an image variant on disk by the given degrees.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern FfiResult photostax_stack_image_rotate(IntPtr stack, int variant, int degrees);

    /// <summary>
    /// Invalidate cached hash/dimensions for an image variant.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void photostax_stack_image_invalidate(IntPtr stack, int variant);

    // ── Metadata functions ────────────────────────────────────────

    /// <summary>
    /// Check whether metadata has been loaded from disk.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    [return: MarshalAs(UnmanagedType.U1)]
    internal static extern bool photostax_stack_metadata_is_loaded(IntPtr stack);

    /// <summary>
    /// Lazily load and return metadata as JSON. Returns null on error.
    /// Caller must free with photostax_string_free.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr photostax_stack_metadata_read(IntPtr stack);

    /// <summary>
    /// Return cached metadata JSON without loading, or null if not loaded yet.
    /// Caller must free with photostax_string_free.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr photostax_stack_metadata_cached(IntPtr stack);

    /// <summary>
    /// Write metadata to the sidecar file.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern FfiResult photostax_stack_metadata_write(
        IntPtr stack,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string json);

    /// <summary>
    /// Invalidate cached metadata, forcing re-load on next read.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void photostax_stack_metadata_invalidate(IntPtr stack);

    /// <summary>
    /// Swap front and back images (for accidentally backward-scanned photos).
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern FfiResult photostax_stack_swap_front_back(IntPtr stack);

    /// <summary>
    /// Free a stack handle array (container and all handles).
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void photostax_stack_handle_array_free(FfiStackHandleArray array);

    /// <summary>
    /// Free a paginated handle result (container and all handles).
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void photostax_paginated_handle_result_free(FfiPaginatedHandleResult result);

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
    internal static extern FfiPaginatedHandleResult photostax_snapshot_get_page(
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

    /// <summary>
    /// Add a foreign (host-language-provided) repository to a StackManager.
    /// The callbacks struct is passed by value.
    /// </summary>
    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    internal static extern FfiResult photostax_manager_add_foreign_repo(
        IntPtr mgr,
        FfiProviderCallbacks callbacks,
        [MarshalAs(UnmanagedType.I1)] bool recursive,
        int profile);
}

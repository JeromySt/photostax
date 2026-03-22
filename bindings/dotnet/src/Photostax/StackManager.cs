using System.Diagnostics.CodeAnalysis;
using System.Runtime.InteropServices;
using Photostax.Native;

namespace Photostax;

/// <summary>
/// Bridges a managed <see cref="IRepositoryProvider"/> into unmanaged C callbacks
/// expected by the native FFI layer.
/// </summary>
[ExcludeFromCodeCoverage]
internal sealed class ProviderBridge
{
    private readonly IRepositoryProvider _provider;
    private readonly Dictionary<ulong, Stream> _streams = new();
    private ulong _nextHandle = 1;

    // Prevent delegates from being garbage-collected while native code holds pointers.
    internal readonly ListEntriesDelegate ListEntriesDelegate;
    internal readonly FreeEntriesDelegate FreeEntriesDelegate;
    internal readonly OpenReadDelegate OpenReadDelegate;
    internal readonly ReadDelegate ReadDelegate;
    internal readonly SeekDelegate SeekDelegate;
    internal readonly CloseReadDelegate CloseReadDelegate;
    internal readonly OpenWriteDelegate OpenWriteDelegate;
    internal readonly WriteDelegate WriteDelegate;
    internal readonly CloseWriteDelegate CloseWriteDelegate;

    public ProviderBridge(IRepositoryProvider provider)
    {
        _provider = provider;
        ListEntriesDelegate = OnListEntries;
        FreeEntriesDelegate = OnFreeEntries;
        OpenReadDelegate = OnOpenRead;
        ReadDelegate = OnRead;
        SeekDelegate = OnSeek;
        CloseReadDelegate = OnCloseRead;
        OpenWriteDelegate = OnOpenWrite;
        WriteDelegate = OnWrite;
        CloseWriteDelegate = OnCloseWrite;
    }

    private ulong AllocateHandle(Stream stream)
    {
        var h = _nextHandle++;
        _streams[h] = stream;
        return h;
    }

    // ── Callback implementations ───────────────────────────────────

    private FfiFileEntryArray OnListEntries(IntPtr ctx, string prefix, bool recursive)
    {
        try
        {
            var entries = _provider.ListEntries(prefix, recursive);
            if (entries.Count == 0)
                return new FfiFileEntryArray { Data = IntPtr.Zero, Len = 0, Error = 0 };

            int structSize = Marshal.SizeOf<FfiFileEntry>();
            var arrayPtr = Marshal.AllocHGlobal(structSize * entries.Count);

            for (int i = 0; i < entries.Count; i++)
            {
                var entry = entries[i];
                var ffi = new FfiFileEntry
                {
                    Name = Marshal.StringToHGlobalAnsi(entry.Name),
                    Folder = Marshal.StringToHGlobalAnsi(entry.Folder),
                    Path = Marshal.StringToHGlobalAnsi(entry.Path),
                    Size = entry.Size,
                };
                Marshal.StructureToPtr(ffi, arrayPtr + i * structSize, false);
            }

            return new FfiFileEntryArray { Data = arrayPtr, Len = (nuint)entries.Count, Error = 0 };
        }
        catch
        {
            return new FfiFileEntryArray { Data = IntPtr.Zero, Len = 0, Error = 1 };
        }
    }

    private void OnFreeEntries(IntPtr ctx, FfiFileEntryArray entries)
    {
        if (entries.Data == IntPtr.Zero || entries.Len == 0)
            return;

        int structSize = Marshal.SizeOf<FfiFileEntry>();
        for (int i = 0; i < (int)entries.Len; i++)
        {
            var ffi = Marshal.PtrToStructure<FfiFileEntry>(entries.Data + i * structSize);
            if (ffi.Name != IntPtr.Zero) Marshal.FreeHGlobal(ffi.Name);
            if (ffi.Folder != IntPtr.Zero) Marshal.FreeHGlobal(ffi.Folder);
            if (ffi.Path != IntPtr.Zero) Marshal.FreeHGlobal(ffi.Path);
        }

        Marshal.FreeHGlobal(entries.Data);
    }

    private FfiStreamHandle OnOpenRead(IntPtr ctx, string path)
    {
        try
        {
            var stream = _provider.OpenRead(path);
            return new FfiStreamHandle { Handle = AllocateHandle(stream), Error = 0 };
        }
        catch
        {
            return new FfiStreamHandle { Handle = 0, Error = 1 };
        }
    }

    private FfiReadResult OnRead(IntPtr ctx, ulong handle, IntPtr buf, nuint len)
    {
        try
        {
            if (!_streams.TryGetValue(handle, out var stream))
                return new FfiReadResult { BytesRead = 0, Error = 1 };

            var managed = new byte[(int)len];
            int read = stream.Read(managed, 0, (int)len);
            Marshal.Copy(managed, 0, buf, read);
            return new FfiReadResult { BytesRead = (nuint)read, Error = 0 };
        }
        catch
        {
            return new FfiReadResult { BytesRead = 0, Error = 1 };
        }
    }

    private FfiSeekResult OnSeek(IntPtr ctx, ulong handle, long offset, int whence)
    {
        try
        {
            if (!_streams.TryGetValue(handle, out var stream))
                return new FfiSeekResult { Position = 0, Error = 1 };

            var origin = whence switch
            {
                0 => SeekOrigin.Begin,
                1 => SeekOrigin.Current,
                2 => SeekOrigin.End,
                _ => SeekOrigin.Begin,
            };

            var pos = stream.Seek(offset, origin);
            return new FfiSeekResult { Position = (ulong)pos, Error = 0 };
        }
        catch
        {
            return new FfiSeekResult { Position = 0, Error = 1 };
        }
    }

    private void OnCloseRead(IntPtr ctx, ulong handle)
    {
        if (_streams.Remove(handle, out var stream))
            stream.Dispose();
    }

    private FfiStreamHandle OnOpenWrite(IntPtr ctx, string path)
    {
        try
        {
            var stream = _provider.OpenWrite(path);
            return new FfiStreamHandle { Handle = AllocateHandle(stream), Error = 0 };
        }
        catch
        {
            return new FfiStreamHandle { Handle = 0, Error = 1 };
        }
    }

    private FfiWriteResult OnWrite(IntPtr ctx, ulong handle, IntPtr buf, nuint len)
    {
        try
        {
            if (!_streams.TryGetValue(handle, out var stream))
                return new FfiWriteResult { BytesWritten = 0, Error = 1 };

            var managed = new byte[(int)len];
            Marshal.Copy(buf, managed, 0, (int)len);
            stream.Write(managed, 0, (int)len);
            return new FfiWriteResult { BytesWritten = len, Error = 0 };
        }
        catch
        {
            return new FfiWriteResult { BytesWritten = 0, Error = 1 };
        }
    }

    private void OnCloseWrite(IntPtr ctx, ulong handle)
    {
        if (_streams.Remove(handle, out var stream))
            stream.Dispose();
    }
}

/// <summary>
/// A multi-repository stack manager that merges stacks from multiple directories
/// into a single unified cache with O(1) lookups and globally unique IDs.
/// </summary>
/// <remarks>
/// <para>
/// Use <see cref="StackManager"/> when you need to manage multiple photo directories
/// as a single collection. For single-directory convenience, use <see cref="PhotostaxRepository"/>.
/// </para>
/// <para>
/// All stacks from every registered repository are accessible through a single cache.
/// Stack IDs are globally unique (opaque SHA-256 hashes) even when different directories
/// contain files with the same name.
/// </para>
/// </remarks>
[ExcludeFromCodeCoverage]
public sealed class StackManager : IDisposable
{
    private readonly RepoSafeHandle _handle;
    private bool _disposed;
    private readonly List<GCHandle> _providerPins = new();

    /// <summary>
    /// Creates an empty StackManager with no repositories.
    /// Call <see cref="AddRepo(string, bool, ScannerProfile)"/> to register directories before scanning.
    /// </summary>
    /// <exception cref="PhotostaxException">Thrown when the manager cannot be created.</exception>
    public StackManager()
    {
        var ptr = NativeMethods.photostax_manager_new();
        if (ptr == IntPtr.Zero)
        {
            throw new PhotostaxException("Failed to create StackManager");
        }

        _handle = RepoSafeHandle.FromPointer(ptr);
    }

    /// <summary>
    /// Gets the number of registered repositories.
    /// </summary>
    public int RepoCount
    {
        get
        {
            ThrowIfDisposed();
            return (int)NativeMethods.photostax_manager_repo_count(_handle.DangerousGetHandle());
        }
    }

    /// <summary>
    /// Registers a repository directory.
    /// </summary>
    /// <remarks>
    /// Multiple directories can be added — all will be scanned together and their
    /// stacks merged into a single cache with globally unique IDs. Overlapping
    /// directories within the same URI scheme are rejected.
    /// </remarks>
    /// <param name="directoryPath">Path to the directory containing FastFoto files.</param>
    /// <param name="recursive">When true, subdirectories are scanned recursively.</param>
    /// <param name="profile">FastFoto scanner configuration (default: Auto).</param>
    /// <exception cref="ArgumentNullException">Thrown when <paramref name="directoryPath"/> is null.</exception>
    /// <exception cref="PhotostaxException">Thrown when the path overlaps with an existing repo.</exception>
    /// <exception cref="ObjectDisposedException">Thrown when the manager has been disposed.</exception>
    public void AddRepo(
        string directoryPath,
        bool recursive = false,
        ScannerProfile profile = ScannerProfile.Auto)
    {
        ArgumentNullException.ThrowIfNull(directoryPath);
        ThrowIfDisposed();

        var result = NativeMethods.photostax_manager_add_repo(
            _handle.DangerousGetHandle(),
            directoryPath,
            recursive,
            (int)profile);

        if (!result.Success)
        {
            var errorMessage = GetErrorMessage(result);
            throw new PhotostaxException(
                errorMessage ?? $"Failed to add repository at '{directoryPath}'");
        }
    }

    /// <summary>
    /// Registers a foreign (host-language-provided) repository.
    /// </summary>
    /// <remarks>
    /// Use this overload for cloud-backed or virtual repositories (OneDrive, Google Drive, etc.)
    /// where I/O is handled by the host language via <see cref="IRepositoryProvider"/>.
    /// The provider is pinned for the lifetime of this <see cref="StackManager"/> to prevent
    /// garbage collection while native code holds references to the callbacks.
    /// </remarks>
    /// <param name="provider">The repository provider implementation.</param>
    /// <param name="recursive">When true, subdirectories are scanned recursively.</param>
    /// <param name="profile">FastFoto scanner configuration (default: Auto).</param>
    /// <exception cref="ArgumentNullException">Thrown when <paramref name="provider"/> is null.</exception>
    /// <exception cref="PhotostaxException">Thrown when the repository cannot be registered.</exception>
    /// <exception cref="ObjectDisposedException">Thrown when the manager has been disposed.</exception>
    public void AddRepo(
        IRepositoryProvider provider,
        bool recursive = false,
        ScannerProfile profile = ScannerProfile.Auto)
    {
        ArgumentNullException.ThrowIfNull(provider);
        ThrowIfDisposed();

        var bridge = new ProviderBridge(provider);
        var bridgeHandle = GCHandle.Alloc(bridge);

        // Pin the location string for the lifetime of the provider.
        var locationBytes = System.Text.Encoding.UTF8.GetBytes(provider.Location + '\0');
        var locationPin = GCHandle.Alloc(locationBytes, GCHandleType.Pinned);

        try
        {
            var callbacks = new FfiProviderCallbacks
            {
                Ctx = GCHandle.ToIntPtr(bridgeHandle),
                Location = locationPin.AddrOfPinnedObject(),
                ListEntries = Marshal.GetFunctionPointerForDelegate(bridge.ListEntriesDelegate),
                FreeEntries = Marshal.GetFunctionPointerForDelegate(bridge.FreeEntriesDelegate),
                OpenRead = Marshal.GetFunctionPointerForDelegate(bridge.OpenReadDelegate),
                Read = Marshal.GetFunctionPointerForDelegate(bridge.ReadDelegate),
                Seek = Marshal.GetFunctionPointerForDelegate(bridge.SeekDelegate),
                CloseRead = Marshal.GetFunctionPointerForDelegate(bridge.CloseReadDelegate),
                OpenWrite = Marshal.GetFunctionPointerForDelegate(bridge.OpenWriteDelegate),
                Write = Marshal.GetFunctionPointerForDelegate(bridge.WriteDelegate),
                CloseWrite = Marshal.GetFunctionPointerForDelegate(bridge.CloseWriteDelegate),
            };

            var result = NativeMethods.photostax_manager_add_foreign_repo(
                _handle.DangerousGetHandle(),
                callbacks,
                recursive,
                (int)profile);

            if (!result.Success)
            {
                var errorMessage = GetErrorMessage(result);
                throw new PhotostaxException(
                    errorMessage ?? $"Failed to add foreign repository '{provider.Location}'");
            }

            // Keep the bridge and location pinned so the GC cannot collect them
            // while the native side holds function pointers.
            _providerPins.Add(bridgeHandle);
            _providerPins.Add(locationPin);
        }
        catch
        {
            if (bridgeHandle.IsAllocated) bridgeHandle.Free();
            if (locationPin.IsAllocated) locationPin.Free();
            throw;
        }
    }

    /// <summary>
    /// Unified query: search and paginate across all repositories in a single call.
    /// This is the primary way to retrieve stacks — it auto-scans on first call.
    /// </summary>
    /// <param name="query">Search criteria, or null to match all stacks.</param>
    /// <param name="pageSize">Number of stacks per page. Use 0 to put all stacks on a single page.</param>
    /// <param name="onProgress">Optional progress callback invoked during scanning phases.</param>
    /// <returns>A paginated query result with page-based navigation.</returns>
    public QueryResult Query(SearchQuery? query = null, int pageSize = 0, Action<ScanPhase, int, int>? onProgress = null)
    {
        ThrowIfDisposed();

        var queryJson = query?.ToJson();

        NativeMethods.ScanProgressCallback? nativeCallback = null;
        if (onProgress != null)
        {
            nativeCallback = (phase, current, total, _) =>
            {
                onProgress((ScanPhase)phase, (int)current, (int)total);
            };
        }

        var result = NativeMethods.photostax_query(
            _handle.DangerousGetHandle(),
            queryJson,
            (nuint)0,
            (nuint)0,
            nativeCallback,
            IntPtr.Zero);
        try
        {
            var stacks = PhotoStack.ConvertPaginatedHandleResultToList(result);
            return new QueryResult(stacks, pageSize);
        }
        finally
        {
            NativeMethods.photostax_paginated_handle_result_free(result);
            GC.KeepAlive(nativeCallback);
        }
    }

    /// <summary>
    /// Creates a point-in-time snapshot for consistent pagination.
    /// </summary>
    /// <param name="loadMetadata">When true, loads full metadata for all stacks.</param>
    /// <returns>A frozen snapshot of the current cache state.</returns>
    public ScanSnapshot CreateSnapshot(bool loadMetadata = false)
    {
        ThrowIfDisposed();

        var ptr = NativeMethods.photostax_create_snapshot(
            _handle.DangerousGetHandle(), loadMetadata);

        if (ptr == IntPtr.Zero)
            throw new PhotostaxException("Failed to create snapshot");

        return new ScanSnapshot(SnapshotSafeHandle.FromPointer(ptr), _handle.DangerousGetHandle());
    }

    /// <summary>
    /// Checks whether a snapshot is still current relative to the live cache.
    /// </summary>
    /// <param name="snapshot">The snapshot to check.</param>
    /// <returns>The status of the snapshot (current, stale, etc.).</returns>
    public SnapshotStatus CheckSnapshotStatus(ScanSnapshot snapshot)
    {
        ArgumentNullException.ThrowIfNull(snapshot);
        ThrowIfDisposed();

        var status = NativeMethods.photostax_snapshot_check_status(
            _handle.DangerousGetHandle(), snapshot.Handle);

        return new SnapshotStatus(
            status.IsStale,
            (int)status.SnapshotCount,
            (int)status.CurrentCount,
            (int)status.Added,
            (int)status.Removed);
    }

    /// <summary>
    /// Disposes the manager and releases all resources.
    /// </summary>
    public void Dispose()
    {
        if (!_disposed)
        {
            _handle.Dispose();

            foreach (var pin in _providerPins)
            {
                if (pin.IsAllocated)
                    pin.Free();
            }
            _providerPins.Clear();

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
}

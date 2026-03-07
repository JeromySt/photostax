using System.Diagnostics.CodeAnalysis;
using System.Runtime.InteropServices;

namespace Photostax.Native;

/// <summary>
/// Safe handle for repository pointers.
/// </summary>
internal sealed class RepoSafeHandle : SafeHandle
{
    /// <summary>
    /// Creates a new repository safe handle.
    /// </summary>
    public RepoSafeHandle() : base(IntPtr.Zero, true)
    {
    }

    /// <inheritdoc/>
    public override bool IsInvalid => handle == IntPtr.Zero;

    /// <inheritdoc/>
    [ExcludeFromCodeCoverage]
    protected override bool ReleaseHandle()
    {
        NativeMethods.photostax_repo_free(handle);
        return true;
    }

    /// <summary>
    /// Creates a safe handle from a raw pointer.
    /// </summary>
    internal static RepoSafeHandle FromPointer(IntPtr ptr)
    {
        var safeHandle = new RepoSafeHandle();
        safeHandle.SetHandle(ptr);
        return safeHandle;
    }
}

/// <summary>
/// Safe handle for stack pointers.
/// </summary>
internal sealed class StackSafeHandle : SafeHandle
{
    /// <summary>
    /// Creates a new stack safe handle.
    /// </summary>
    public StackSafeHandle() : base(IntPtr.Zero, true)
    {
    }

    /// <inheritdoc/>
    public override bool IsInvalid => handle == IntPtr.Zero;

    /// <inheritdoc/>
    [ExcludeFromCodeCoverage]
    protected override bool ReleaseHandle()
    {
        NativeMethods.photostax_stack_free(handle);
        return true;
    }

    /// <summary>
    /// Creates a safe handle from a raw pointer.
    /// </summary>
    internal static StackSafeHandle FromPointer(IntPtr ptr)
    {
        var safeHandle = new StackSafeHandle();
        safeHandle.SetHandle(ptr);
        return safeHandle;
    }
}

/// <summary>
/// Safe handle for string pointers.
/// </summary>
internal sealed class StringSafeHandle : SafeHandle
{
    /// <summary>
    /// Creates a new string safe handle.
    /// </summary>
    public StringSafeHandle() : base(IntPtr.Zero, true)
    {
    }

    /// <inheritdoc/>
    public override bool IsInvalid => handle == IntPtr.Zero;

    /// <inheritdoc/>
    [ExcludeFromCodeCoverage]
    protected override bool ReleaseHandle()
    {
        NativeMethods.photostax_string_free(handle);
        return true;
    }

    /// <summary>
    /// Creates a safe handle from a raw pointer.
    /// </summary>
    internal static StringSafeHandle FromPointer(IntPtr ptr)
    {
        var safeHandle = new StringSafeHandle();
        safeHandle.SetHandle(ptr);
        return safeHandle;
    }

    /// <summary>
    /// Gets the string value from the handle.
    /// </summary>
    public string? GetString()
    {
        if (IsInvalid)
            return null;
        return Marshal.PtrToStringUTF8(handle);
    }
}

/// <summary>
/// Safe handle for byte buffer pointers.
/// </summary>
internal sealed class BytesSafeHandle : SafeHandle
{
    private nuint _length;

    /// <summary>
    /// Creates a new bytes safe handle.
    /// </summary>
    public BytesSafeHandle() : base(IntPtr.Zero, true)
    {
    }

    /// <inheritdoc/>
    public override bool IsInvalid => handle == IntPtr.Zero;

    /// <summary>
    /// Gets the length of the byte buffer.
    /// </summary>
    public nuint Length => _length;

    /// <inheritdoc/>
    [ExcludeFromCodeCoverage]
    protected override bool ReleaseHandle()
    {
        NativeMethods.photostax_bytes_free(handle, _length);
        return true;
    }

    /// <summary>
    /// Creates a safe handle from a raw pointer and length.
    /// </summary>
    internal static BytesSafeHandle FromPointer(IntPtr ptr, nuint length)
    {
        var safeHandle = new BytesSafeHandle();
        safeHandle.SetHandle(ptr);
        safeHandle._length = length;
        return safeHandle;
    }

    /// <summary>
    /// Gets the byte array from the handle.
    /// </summary>
    public byte[] ToArray()
    {
        if (IsInvalid)
            return [];

        var bytes = new byte[_length];
        Marshal.Copy(handle, bytes, 0, (int)_length);
        return bytes;
    }
}

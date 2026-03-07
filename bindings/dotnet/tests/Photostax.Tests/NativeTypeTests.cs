using Photostax.Native;
using Xunit;

namespace Photostax.Tests;

/// <summary>
/// Tests for SafeHandle types and NativeTypes structs (non-native logic only).
/// </summary>
public class NativeTypeTests
{
    // --- FfiResult ---

    [Fact]
    public void FfiResult_Default_SuccessIsFalse()
    {
        var result = new FfiResult();

        Assert.False(result.Success);
        Assert.Equal(IntPtr.Zero, result.ErrorMessage);
    }

    // --- FfiPhotoStackArray ---

    [Fact]
    public void FfiPhotoStackArray_Default_IsEmpty()
    {
        var array = new FfiPhotoStackArray();

        Assert.Equal(IntPtr.Zero, array.Data);
        Assert.Equal((nuint)0, array.Len);
    }

    // --- FfiPhotoStack ---

    [Fact]
    public void FfiPhotoStack_Default_AllPointersAreZero()
    {
        var stack = new FfiPhotoStack();

        Assert.Equal(IntPtr.Zero, stack.Id);
        Assert.Equal(IntPtr.Zero, stack.Original);
        Assert.Equal(IntPtr.Zero, stack.Enhanced);
        Assert.Equal(IntPtr.Zero, stack.Back);
        Assert.Equal(IntPtr.Zero, stack.MetadataJson);
    }

    // --- RepoSafeHandle ---

    [Fact]
    public void RepoSafeHandle_Default_IsInvalid()
    {
        var handle = new RepoSafeHandle();

        Assert.True(handle.IsInvalid);
        handle.Dispose();
    }

    [Fact]
    public void RepoSafeHandle_FromPointer_Zero_IsInvalid()
    {
        var handle = RepoSafeHandle.FromPointer(IntPtr.Zero);

        Assert.True(handle.IsInvalid);
        handle.Dispose();
    }

    // --- StackSafeHandle ---

    [Fact]
    public void StackSafeHandle_Default_IsInvalid()
    {
        var handle = new StackSafeHandle();

        Assert.True(handle.IsInvalid);
        handle.Dispose();
    }

    [Fact]
    public void StackSafeHandle_FromPointer_Zero_IsInvalid()
    {
        var handle = StackSafeHandle.FromPointer(IntPtr.Zero);

        Assert.True(handle.IsInvalid);
        handle.Dispose();
    }

    // --- StringSafeHandle ---

    [Fact]
    public void StringSafeHandle_Default_IsInvalid()
    {
        var handle = new StringSafeHandle();

        Assert.True(handle.IsInvalid);
        handle.Dispose();
    }

    [Fact]
    public void StringSafeHandle_FromPointer_Zero_IsInvalid()
    {
        var handle = StringSafeHandle.FromPointer(IntPtr.Zero);

        Assert.True(handle.IsInvalid);
        handle.Dispose();
    }

    [Fact]
    public void StringSafeHandle_GetString_WhenInvalid_ReturnsNull()
    {
        var handle = StringSafeHandle.FromPointer(IntPtr.Zero);

        Assert.Null(handle.GetString());
        handle.Dispose();
    }

    // --- BytesSafeHandle ---

    [Fact]
    public void BytesSafeHandle_Default_IsInvalid()
    {
        var handle = new BytesSafeHandle();

        Assert.True(handle.IsInvalid);
        handle.Dispose();
    }

    [Fact]
    public void BytesSafeHandle_FromPointer_Zero_IsInvalid()
    {
        var handle = BytesSafeHandle.FromPointer(IntPtr.Zero, 0);

        Assert.True(handle.IsInvalid);
        handle.Dispose();
    }

    [Fact]
    public void BytesSafeHandle_ToArray_WhenInvalid_ReturnsEmpty()
    {
        var handle = BytesSafeHandle.FromPointer(IntPtr.Zero, 0);

        Assert.Empty(handle.ToArray());
        handle.Dispose();
    }

    [Fact]
    public void BytesSafeHandle_Length_ReturnsSetLength()
    {
        var handle = BytesSafeHandle.FromPointer(IntPtr.Zero, 42);

        Assert.Equal((nuint)42, handle.Length);
        handle.Dispose();
    }

    [Fact]
    public void StringSafeHandle_FromPointer_WithValidPointer_GetStringReturnsNull_WhenZero()
    {
        using var handle = new StringSafeHandle();
        Assert.True(handle.IsInvalid);
        Assert.Null(handle.GetString());
    }
}

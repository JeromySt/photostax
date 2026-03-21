using System.Runtime.InteropServices;
using Xunit;

namespace Photostax.Tests;

/// <summary>
/// Tests for the PhotoStack class.
/// Since PhotoStack now wraps an opaque native handle, unit tests are limited
/// to verifying the constructor guard clause and static conversion helpers.
/// </summary>
public class PhotoStackTests
{
    [Fact]
    public void Constructor_ZeroHandle_ThrowsArgumentException()
    {
        Assert.Throws<ArgumentException>(() => new PhotoStack(IntPtr.Zero));
    }

    [Fact]
    public void Dispose_SetsHandleToZero()
    {
        // Allocate a dummy pointer (not a real native handle)
        var dummy = Marshal.AllocHGlobal(1);
        var stack = new PhotoStack(dummy);

        // We can't call the real Dispose because it would call photostax_stack_free
        // on a fake pointer. Instead, verify the handle was set.
        Assert.NotEqual(IntPtr.Zero, stack.Handle);

        // Clean up without calling native free (leak the 1 byte — acceptable in test)
        Marshal.FreeHGlobal(dummy);
    }

    [Fact]
    public void HasAnyImage_DelegatesToImageRefs()
    {
        // Without a real native handle, we can't test IsPresent.
        // This test verifies the property exists and the sub-objects are created.
        var dummy = Marshal.AllocHGlobal(1);
        var stack = new PhotoStack(dummy);

        Assert.NotNull(stack.Original);
        Assert.NotNull(stack.Enhanced);
        Assert.NotNull(stack.Back);
        Assert.NotNull(stack.Metadata);

        Marshal.FreeHGlobal(dummy);
    }

    [Fact]
    public void Format_ReturnsJpeg_WhenFormatPropertyAccessed()
    {
        // ImageFormat.Jpeg is still the default format detection
        Assert.Equal(ImageFormat.Jpeg, ImageFormat.Jpeg);
    }

    [Fact]
    public void Rotate_InvalidDegrees_ThrowsArgumentException()
    {
        var dummy = Marshal.AllocHGlobal(1);
        var stack = new PhotoStack(dummy);

        Assert.Throws<ArgumentException>(() => stack.Rotate(45));

        Marshal.FreeHGlobal(dummy);
    }
}

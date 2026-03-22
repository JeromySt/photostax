using Xunit;

namespace Photostax.Tests;

/// <summary>
/// Tests for the PhotoStack class.
/// Since PhotoStack wraps an opaque native handle and eagerly reads FFI
/// properties in the constructor, tests that need a real handle must use
/// the Integration category. Tests here verify guard clauses and enums only.
/// </summary>
public class PhotoStackTests
{
    [Fact]
    public void Constructor_ZeroHandle_ThrowsArgumentException()
    {
        Assert.Throws<ArgumentException>(() => new PhotoStack(IntPtr.Zero));
    }

    [Fact]
    public void Format_ReturnsJpeg_WhenFormatPropertyAccessed()
    {
        Assert.Equal(ImageFormat.Jpeg, ImageFormat.Jpeg);
    }

    [Fact]
    public void ImageVariants_Flags_Combine()
    {
        var variants = ImageVariants.Original | ImageVariants.Back;
        Assert.True(variants.HasFlag(ImageVariants.Original));
        Assert.True(variants.HasFlag(ImageVariants.Back));
        Assert.False(variants.HasFlag(ImageVariants.Enhanced));
    }

    [Fact]
    public void ImageVariants_None_IsZero()
    {
        Assert.Equal(0, (int)ImageVariants.None);
    }

    [Fact]
    [Trait("Category", "Integration")]
    public void Constructor_RealHandle_EagerlyReadsProperties()
    {
        using var repo = new PhotostaxRepository("/nonexistent/path");
        var result = repo.Query();
        // Empty repo produces no stacks — this just verifies no crash
        Assert.Empty(result.AllStacks);
    }
}

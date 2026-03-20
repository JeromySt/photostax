using Xunit;

namespace Photostax.Tests;

/// <summary>
/// Tests for the PhotoStack class.
/// </summary>
public class PhotoStackTests
{
    [Fact]
    public void Constructor_NullId_ThrowsArgumentNullException()
    {
        Assert.Throws<ArgumentNullException>(() =>
            new PhotoStack(IntPtr.Zero, null!, "name", null, null, null, null, new Metadata()));
    }

    [Fact]
    public void Constructor_NullMetadata_ThrowsArgumentNullException()
    {
        Assert.Throws<ArgumentNullException>(() =>
            new PhotoStack(IntPtr.Zero, "id", "name", null, null, null, null, null!));
    }

    [Fact]
    public void Constructor_SetsIdCorrectly()
    {
        var stack = CreateStack("test-id");

        Assert.Equal("test-id", stack.Id);
    }

    [Fact]
    public void Constructor_SetsOriginalPathCorrectly()
    {
        var stack = CreateStack("id", originalPath: "/path/to/original.jpg");

        Assert.Equal("/path/to/original.jpg", stack.OriginalPath);
    }

    [Fact]
    public void Constructor_SetsEnhancedPathCorrectly()
    {
        var stack = CreateStack("id", enhancedPath: "/path/to/enhanced.jpg");

        Assert.Equal("/path/to/enhanced.jpg", stack.EnhancedPath);
    }

    [Fact]
    public void Constructor_SetsBackPathCorrectly()
    {
        var stack = CreateStack("id", backPath: "/path/to/back.jpg");

        Assert.Equal("/path/to/back.jpg", stack.BackPath);
    }

    [Fact]
    public void Constructor_SetsMetadataCorrectly()
    {
        var exifTags = new Dictionary<string, string> { ["Make"] = "EPSON" };
        var metadata = new Metadata(exifTags, new Dictionary<string, string>(), new Dictionary<string, object?>());
        var stack = CreateStack("id", metadata: metadata);

        Assert.Equal("EPSON", stack.Metadata.ExifTags["Make"]);
    }

    [Fact]
    public void HasAnyImage_NoImages_ReturnsFalse()
    {
        var stack = CreateStack("id");

        Assert.False(stack.HasAnyImage);
    }

    [Fact]
    public void HasAnyImage_WithOriginal_ReturnsTrue()
    {
        var stack = CreateStack("id", originalPath: "/path/original.jpg");

        Assert.True(stack.HasAnyImage);
    }

    [Fact]
    public void HasAnyImage_WithEnhanced_ReturnsTrue()
    {
        var stack = CreateStack("id", enhancedPath: "/path/enhanced.jpg");

        Assert.True(stack.HasAnyImage);
    }

    [Fact]
    public void HasAnyImage_WithBack_ReturnsTrue()
    {
        var stack = CreateStack("id", backPath: "/path/back.jpg");

        Assert.True(stack.HasAnyImage);
    }

    [Fact]
    public void HasAnyImage_WithAllImages_ReturnsTrue()
    {
        var stack = CreateStack("id",
            originalPath: "/path/original.jpg",
            enhancedPath: "/path/enhanced.jpg",
            backPath: "/path/back.jpg");

        Assert.True(stack.HasAnyImage);
    }

    [Fact]
    public void Format_NoImages_ReturnsNull()
    {
        var stack = CreateStack("id");

        Assert.Null(stack.Format);
    }

    [Fact]
    public void Format_JpgExtension_ReturnsJpeg()
    {
        var stack = CreateStack("id", originalPath: "/path/image.jpg");

        Assert.Equal(ImageFormat.Jpeg, stack.Format);
    }

    [Fact]
    public void Format_JpegExtension_ReturnsJpeg()
    {
        var stack = CreateStack("id", originalPath: "/path/image.jpeg");

        Assert.Equal(ImageFormat.Jpeg, stack.Format);
    }

    [Fact]
    public void Format_JpgUpperCase_ReturnsJpeg()
    {
        var stack = CreateStack("id", originalPath: "/path/image.JPG");

        Assert.Equal(ImageFormat.Jpeg, stack.Format);
    }

    [Fact]
    public void Format_PngExtension_ReturnsPng()
    {
        var stack = CreateStack("id", originalPath: "/path/image.png");

        Assert.Equal(ImageFormat.Png, stack.Format);
    }

    [Fact]
    public void Format_TifExtension_ReturnsTiff()
    {
        var stack = CreateStack("id", originalPath: "/path/image.tif");

        Assert.Equal(ImageFormat.Tiff, stack.Format);
    }

    [Fact]
    public void Format_TiffExtension_ReturnsTiff()
    {
        var stack = CreateStack("id", originalPath: "/path/image.tiff");

        Assert.Equal(ImageFormat.Tiff, stack.Format);
    }

    [Fact]
    public void Format_UnknownExtension_ReturnsUnknown()
    {
        var stack = CreateStack("id", originalPath: "/path/image.bmp");

        Assert.Equal(ImageFormat.Unknown, stack.Format);
    }

    [Fact]
    public void Format_NoOriginal_UsesEnhanced()
    {
        var stack = CreateStack("id", enhancedPath: "/path/enhanced.png");

        Assert.Equal(ImageFormat.Png, stack.Format);
    }

    [Fact]
    public void Format_NoOriginalOrEnhanced_UsesBack()
    {
        var stack = CreateStack("id", backPath: "/path/back.tiff");

        Assert.Equal(ImageFormat.Tiff, stack.Format);
    }

    [Fact]
    public void Format_PrioritizesOriginal()
    {
        var stack = CreateStack("id",
            originalPath: "/path/original.jpg",
            enhancedPath: "/path/enhanced.png",
            backPath: "/path/back.tiff");

        Assert.Equal(ImageFormat.Jpeg, stack.Format);
    }

    private static PhotoStack CreateStack(
        string id,
        string? originalPath = null,
        string? enhancedPath = null,
        string? backPath = null,
        Metadata? metadata = null)
    {
        return new PhotoStack(
            IntPtr.Zero,
            id,
            id,
            null,
            originalPath,
            enhancedPath,
            backPath,
            metadata ?? new Metadata());
    }
}

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
            new PhotoStack(IntPtr.Zero, null!, "name", null, false, false, false, new Metadata()));
    }

    [Fact]
    public void Constructor_NullMetadata_ThrowsArgumentNullException()
    {
        Assert.Throws<ArgumentNullException>(() =>
            new PhotoStack(IntPtr.Zero, "id", "name", null, false, false, false, null!));
    }

    [Fact]
    public void Constructor_SetsIdCorrectly()
    {
        var stack = CreateStack("test-id");

        Assert.Equal("test-id", stack.Id);
    }

    [Fact]
    public void Constructor_SetsHasOriginalCorrectly()
    {
        var stack = CreateStack("id", hasOriginal: true);

        Assert.True(stack.HasOriginal);
    }

    [Fact]
    public void Constructor_SetsHasEnhancedCorrectly()
    {
        var stack = CreateStack("id", hasEnhanced: true);

        Assert.True(stack.HasEnhanced);
    }

    [Fact]
    public void Constructor_SetsHasBackCorrectly()
    {
        var stack = CreateStack("id", hasBack: true);

        Assert.True(stack.HasBack);
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
        var stack = CreateStack("id", hasOriginal: true);

        Assert.True(stack.HasAnyImage);
    }

    [Fact]
    public void HasAnyImage_WithEnhanced_ReturnsTrue()
    {
        var stack = CreateStack("id", hasEnhanced: true);

        Assert.True(stack.HasAnyImage);
    }

    [Fact]
    public void HasAnyImage_WithBack_ReturnsTrue()
    {
        var stack = CreateStack("id", hasBack: true);

        Assert.True(stack.HasAnyImage);
    }

    [Fact]
    public void HasAnyImage_WithAllImages_ReturnsTrue()
    {
        var stack = CreateStack("id", hasOriginal: true, hasEnhanced: true, hasBack: true);

        Assert.True(stack.HasAnyImage);
    }

    [Fact]
    public void Format_NoImages_ReturnsNull()
    {
        var stack = CreateStack("id");

        Assert.Null(stack.Format);
    }

    [Fact]
    public void Format_WithImage_ReturnsJpeg()
    {
        var stack = CreateStack("id", hasOriginal: true);

        Assert.Equal(ImageFormat.Jpeg, stack.Format);
    }

    private static PhotoStack CreateStack(
        string id,
        bool hasOriginal = false,
        bool hasEnhanced = false,
        bool hasBack = false,
        Metadata? metadata = null)
    {
        return new PhotoStack(
            IntPtr.Zero,
            id,
            id,
            null,
            hasOriginal,
            hasEnhanced,
            hasBack,
            metadata ?? new Metadata());
    }
}

using Xunit;

namespace Photostax.Tests;

/// <summary>
/// Tests for the ImageFormat enum.
/// </summary>
public class ImageFormatTests
{
    [Theory]
    [InlineData(ImageFormat.Jpeg, 0)]
    [InlineData(ImageFormat.Png, 1)]
    [InlineData(ImageFormat.Tiff, 2)]
    [InlineData(ImageFormat.Unknown, 3)]
    public void EnumValues_HaveExpectedUnderlyingValues(ImageFormat format, int expected)
    {
        Assert.Equal(expected, (int)format);
    }

    [Fact]
    public void AllValues_AreDefined()
    {
        var values = Enum.GetValues<ImageFormat>();
        Assert.Equal(4, values.Length);
    }
}

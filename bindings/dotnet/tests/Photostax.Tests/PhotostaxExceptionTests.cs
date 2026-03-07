using Xunit;

namespace Photostax.Tests;

/// <summary>
/// Tests for the PhotostaxException class.
/// </summary>
public class PhotostaxExceptionTests
{
    [Fact]
    public void DefaultConstructor_CreatesException()
    {
        var ex = new PhotostaxException();

        Assert.NotNull(ex);
        Assert.Null(ex.InnerException);
    }

    [Fact]
    public void MessageConstructor_SetsMessage()
    {
        var ex = new PhotostaxException("test error");

        Assert.Equal("test error", ex.Message);
        Assert.Null(ex.InnerException);
    }

    [Fact]
    public void MessageAndInnerConstructor_SetsBothProperties()
    {
        var inner = new InvalidOperationException("inner");
        var ex = new PhotostaxException("outer", inner);

        Assert.Equal("outer", ex.Message);
        Assert.Same(inner, ex.InnerException);
    }

    [Fact]
    public void IsException_DerivedFromException()
    {
        var ex = new PhotostaxException("test");

        Assert.IsAssignableFrom<Exception>(ex);
    }
}

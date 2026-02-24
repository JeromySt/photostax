using Xunit;

namespace Photostax.Tests;

/// <summary>
/// Tests for the SearchQuery class.
/// </summary>
public class SearchTests
{
    [Fact]
    public void EmptyQuery_ToJson_ReturnsEmptyObject()
    {
        var query = new SearchQuery();
        var json = query.ToJson();

        Assert.Equal("{}", json);
    }

    [Fact]
    public void WithText_ToJson_IncludesTextQuery()
    {
        var query = new SearchQuery().WithText("birthday");
        var json = query.ToJson();

        Assert.Contains("\"text_query\":\"birthday\"", json);
    }

    [Fact]
    public void WithExifFilter_ToJson_IncludesExifFilters()
    {
        var query = new SearchQuery()
            .WithExifFilter("Make", "EPSON");
        var json = query.ToJson();

        Assert.Contains("\"exif_filters\"", json);
        Assert.Contains("Make", json);
        Assert.Contains("EPSON", json);
    }

    [Fact]
    public void WithCustomFilter_ToJson_IncludesCustomFilters()
    {
        var query = new SearchQuery()
            .WithCustomFilter("album", "Family");
        var json = query.ToJson();

        Assert.Contains("\"custom_filters\"", json);
        Assert.Contains("album", json);
        Assert.Contains("Family", json);
    }

    [Fact]
    public void WithHasBack_True_ToJson_IncludesHasBack()
    {
        var query = new SearchQuery().WithHasBack(true);
        var json = query.ToJson();

        Assert.Contains("\"has_back\":true", json);
    }

    [Fact]
    public void WithHasBack_False_ToJson_IncludesHasBackFalse()
    {
        var query = new SearchQuery().WithHasBack(false);
        var json = query.ToJson();

        Assert.Contains("\"has_back\":false", json);
    }

    [Fact]
    public void WithHasEnhanced_True_ToJson_IncludesHasEnhanced()
    {
        var query = new SearchQuery().WithHasEnhanced(true);
        var json = query.ToJson();

        Assert.Contains("\"has_enhanced\":true", json);
    }

    [Fact]
    public void WithHasEnhanced_False_ToJson_IncludesHasEnhancedFalse()
    {
        var query = new SearchQuery().WithHasEnhanced(false);
        var json = query.ToJson();

        Assert.Contains("\"has_enhanced\":false", json);
    }

    [Fact]
    public void ChainedFilters_ToJson_IncludesAllFilters()
    {
        var query = new SearchQuery()
            .WithText("vacation")
            .WithExifFilter("Make", "Canon")
            .WithCustomFilter("location", "Beach")
            .WithHasBack(true)
            .WithHasEnhanced(false);

        var json = query.ToJson();

        Assert.Contains("\"text_query\":\"vacation\"", json);
        Assert.Contains("\"exif_filters\"", json);
        Assert.Contains("\"custom_filters\"", json);
        Assert.Contains("\"has_back\":true", json);
        Assert.Contains("\"has_enhanced\":false", json);
    }

    [Fact]
    public void MultipleExifFilters_ToJson_IncludesAllFilters()
    {
        var query = new SearchQuery()
            .WithExifFilter("Make", "EPSON")
            .WithExifFilter("Model", "FastFoto");

        var json = query.ToJson();

        Assert.Contains("Make", json);
        Assert.Contains("EPSON", json);
        Assert.Contains("Model", json);
        Assert.Contains("FastFoto", json);
    }

    [Fact]
    public void WithText_Fluent_ReturnsSameInstance()
    {
        var query = new SearchQuery();
        var result = query.WithText("test");

        Assert.Same(query, result);
    }

    [Fact]
    public void WithExifFilter_Fluent_ReturnsSameInstance()
    {
        var query = new SearchQuery();
        var result = query.WithExifFilter("key", "value");

        Assert.Same(query, result);
    }

    [Fact]
    public void WithCustomFilter_Fluent_ReturnsSameInstance()
    {
        var query = new SearchQuery();
        var result = query.WithCustomFilter("key", "value");

        Assert.Same(query, result);
    }

    [Fact]
    public void WithHasBack_Fluent_ReturnsSameInstance()
    {
        var query = new SearchQuery();
        var result = query.WithHasBack(true);

        Assert.Same(query, result);
    }

    [Fact]
    public void WithHasEnhanced_Fluent_ReturnsSameInstance()
    {
        var query = new SearchQuery();
        var result = query.WithHasEnhanced(true);

        Assert.Same(query, result);
    }
}

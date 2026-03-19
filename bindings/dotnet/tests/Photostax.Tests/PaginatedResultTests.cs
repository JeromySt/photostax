using Xunit;

namespace Photostax.Tests;

/// <summary>
/// Tests for the PaginatedResult class.
/// </summary>
public class PaginatedResultTests
{
    [Fact]
    public void Constructor_SetsProperties()
    {
        var items = new[] { "a", "b", "c" };

        var result = new PaginatedResult<string>(items, totalCount: 10, offset: 0, limit: 3, hasMore: true);

        Assert.Equal(3, result.Items.Count);
        Assert.Equal(10, result.TotalCount);
        Assert.Equal(0, result.Offset);
        Assert.Equal(3, result.Limit);
        Assert.True(result.HasMore);
    }

    [Fact]
    public void Items_AreReadOnly()
    {
        var items = new[] { "a", "b" };

        var result = new PaginatedResult<string>(items, totalCount: 2, offset: 0, limit: 10, hasMore: false);

        Assert.IsAssignableFrom<IReadOnlyList<string>>(result.Items);
    }

    [Fact]
    public void HasMore_False_WhenAllItemsReturned()
    {
        var items = new[] { "a", "b" };

        var result = new PaginatedResult<string>(items, totalCount: 2, offset: 0, limit: 10, hasMore: false);

        Assert.False(result.HasMore);
        Assert.Equal(2, result.TotalCount);
    }

    [Fact]
    public void EmptyPage_HasNoItems()
    {
        var result = new PaginatedResult<string>(Array.Empty<string>(), totalCount: 0, offset: 0, limit: 10, hasMore: false);

        Assert.Empty(result.Items);
        Assert.Equal(0, result.TotalCount);
        Assert.False(result.HasMore);
    }

    [Fact]
    public void MiddlePage_ReflectsOffset()
    {
        var items = new[] { "c", "d" };

        var result = new PaginatedResult<string>(items, totalCount: 5, offset: 2, limit: 2, hasMore: true);

        Assert.Equal(2, result.Items.Count);
        Assert.Equal(2, result.Offset);
        Assert.Equal(5, result.TotalCount);
        Assert.True(result.HasMore);
    }

    [Fact]
    public void LastPage_HasMore_IsFalse()
    {
        var items = new[] { "e" };

        var result = new PaginatedResult<string>(items, totalCount: 5, offset: 4, limit: 2, hasMore: false);

        Assert.Single(result.Items);
        Assert.Equal(4, result.Offset);
        Assert.False(result.HasMore);
    }

    [Fact]
    public void WorksWithPhotoStackType()
    {
        var stack = new PhotoStack("test-001", "test-001", null, "/orig.jpg", null, null, new Metadata());
        var items = new[] { stack };

        var result = new PaginatedResult<PhotoStack>(items, totalCount: 1, offset: 0, limit: 10, hasMore: false);

        Assert.Single(result.Items);
        Assert.Equal("test-001", result.Items[0].Id);
    }
}

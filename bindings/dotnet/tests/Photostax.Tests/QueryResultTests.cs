using Xunit;

namespace Photostax.Tests;

/// <summary>
/// Tests for the QueryResult class.
/// Uses string-based stubs since PhotoStack requires a native handle.
/// We test the pagination logic via a thin wrapper that mirrors QueryResult behavior.
/// </summary>
public class QueryResultTests
{
    /// <summary>
    /// Helper that mirrors QueryResult pagination logic for testing without native handles.
    /// </summary>
    private sealed class PagedList<T>
    {
        private readonly IReadOnlyList<T> _all;
        private readonly int _pageSize;
        private int _currentPageIndex;

        public PagedList(IReadOnlyList<T> all, int pageSize)
        {
            _all = all;
            _pageSize = pageSize > 0 ? pageSize : Math.Max(all.Count, 1);
        }

        public int TotalCount => _all.Count;
        public int PageSize => _pageSize;
        public int PageCount => TotalCount == 0 ? 0 : (_all.Count + _pageSize - 1) / _pageSize;
        public int CurrentPageIndex => _currentPageIndex;
        public bool HasMore => _currentPageIndex < PageCount - 1;

        public IReadOnlyList<T> CurrentPage
        {
            get
            {
                if (TotalCount == 0) return Array.Empty<T>();
                int start = _currentPageIndex * _pageSize;
                int count = Math.Min(_pageSize, _all.Count - start);
                return _all.Skip(start).Take(count).ToList().AsReadOnly();
            }
        }

        public IReadOnlyList<T>? GetPage(int pageIndex)
        {
            if (pageIndex < 0 || pageIndex >= PageCount) return null;
            int start = pageIndex * _pageSize;
            int count = Math.Min(_pageSize, _all.Count - start);
            return _all.Skip(start).Take(count).ToList().AsReadOnly();
        }

        public IReadOnlyList<T>? NextPage()
        {
            if (!HasMore) return null;
            _currentPageIndex++;
            return CurrentPage;
        }

        public IReadOnlyList<T>? PreviousPage()
        {
            if (_currentPageIndex <= 0) return null;
            _currentPageIndex--;
            return CurrentPage;
        }

        public IReadOnlyList<T>? SetPage(int pageIndex)
        {
            if (pageIndex < 0 || pageIndex >= PageCount) return null;
            _currentPageIndex = pageIndex;
            return CurrentPage;
        }

        public IReadOnlyList<T> AllItems => _all;
    }

    [Fact]
    public void Empty_HasZeroCounts()
    {
        var paged = new PagedList<string>(Array.Empty<string>(), 10);

        Assert.Equal(0, paged.TotalCount);
        Assert.Equal(0, paged.PageCount);
        Assert.False(paged.HasMore);
        Assert.Empty(paged.CurrentPage);
    }

    [Fact]
    public void SinglePage_AllItemsOnFirstPage()
    {
        var items = new[] { "a", "b", "c" };
        var paged = new PagedList<string>(items, 10);

        Assert.Equal(3, paged.TotalCount);
        Assert.Equal(1, paged.PageCount);
        Assert.Equal(10, paged.PageSize);
        Assert.Equal(0, paged.CurrentPageIndex);
        Assert.False(paged.HasMore);
        Assert.Equal(3, paged.CurrentPage.Count);
    }

    [Fact]
    public void MultiplePages_CorrectPageCount()
    {
        var items = Enumerable.Range(1, 25).Select(i => $"item-{i}").ToList();
        var paged = new PagedList<string>(items, 10);

        Assert.Equal(25, paged.TotalCount);
        Assert.Equal(3, paged.PageCount);
        Assert.Equal(10, paged.PageSize);
        Assert.True(paged.HasMore);
    }

    [Fact]
    public void CurrentPage_ReturnsCorrectSlice()
    {
        var items = Enumerable.Range(1, 5).Select(i => $"item-{i}").ToList();
        var paged = new PagedList<string>(items, 2);

        Assert.Equal(new[] { "item-1", "item-2" }, paged.CurrentPage);
    }

    [Fact]
    public void NextPage_AdvancesAndReturnsPage()
    {
        var items = Enumerable.Range(1, 5).Select(i => $"item-{i}").ToList();
        var paged = new PagedList<string>(items, 2);

        var page = paged.NextPage();
        Assert.NotNull(page);
        Assert.Equal(1, paged.CurrentPageIndex);
        Assert.Equal(new[] { "item-3", "item-4" }, page);
    }

    [Fact]
    public void NextPage_ReturnsNullOnLastPage()
    {
        var items = new[] { "a", "b" };
        var paged = new PagedList<string>(items, 10);

        Assert.Null(paged.NextPage());
        Assert.Equal(0, paged.CurrentPageIndex);
    }

    [Fact]
    public void PreviousPage_GoesBackAndReturnsPage()
    {
        var items = Enumerable.Range(1, 10).Select(i => $"item-{i}").ToList();
        var paged = new PagedList<string>(items, 3);

        paged.NextPage(); // page 1
        var page = paged.PreviousPage();
        Assert.NotNull(page);
        Assert.Equal(0, paged.CurrentPageIndex);
    }

    [Fact]
    public void PreviousPage_ReturnsNullOnFirstPage()
    {
        var items = new[] { "a" };
        var paged = new PagedList<string>(items, 10);

        Assert.Null(paged.PreviousPage());
    }

    [Fact]
    public void SetPage_ValidIndex_ReturnsPage()
    {
        var items = Enumerable.Range(1, 30).Select(i => $"item-{i}").ToList();
        var paged = new PagedList<string>(items, 10);

        var page = paged.SetPage(2);
        Assert.NotNull(page);
        Assert.Equal(2, paged.CurrentPageIndex);
    }

    [Fact]
    public void SetPage_OutOfRange_ReturnsNull()
    {
        var items = Enumerable.Range(1, 10).Select(i => $"item-{i}").ToList();
        var paged = new PagedList<string>(items, 5);

        Assert.Null(paged.SetPage(-1));
        Assert.Null(paged.SetPage(2));
        Assert.Equal(0, paged.CurrentPageIndex);
    }

    [Fact]
    public void GetPage_ReturnsCorrectSlice()
    {
        var items = Enumerable.Range(1, 7).Select(i => $"item-{i}").ToList();
        var paged = new PagedList<string>(items, 3);

        var page1 = paged.GetPage(1);
        Assert.NotNull(page1);
        Assert.Equal(new[] { "item-4", "item-5", "item-6" }, page1);
    }

    [Fact]
    public void GetPage_LastPage_HasPartialItems()
    {
        var items = Enumerable.Range(1, 7).Select(i => $"item-{i}").ToList();
        var paged = new PagedList<string>(items, 3);

        var lastPage = paged.GetPage(2);
        Assert.NotNull(lastPage);
        Assert.Single(lastPage!);
        Assert.Equal("item-7", lastPage[0]);
    }

    [Fact]
    public void GetPage_OutOfRange_ReturnsNull()
    {
        var items = new[] { "a", "b", "c" };
        var paged = new PagedList<string>(items, 2);

        Assert.Null(paged.GetPage(-1));
        Assert.Null(paged.GetPage(2));
    }

    [Fact]
    public void GetPage_DoesNotChangeCurrentPage()
    {
        var items = Enumerable.Range(1, 10).Select(i => $"item-{i}").ToList();
        var paged = new PagedList<string>(items, 5);

        _ = paged.GetPage(1);
        Assert.Equal(0, paged.CurrentPageIndex);
    }

    [Fact]
    public void AllItems_ReturnsFullList()
    {
        var items = new[] { "a", "b", "c" };
        var paged = new PagedList<string>(items, 2);

        Assert.Equal(3, paged.AllItems.Count);
        Assert.Equal(items, paged.AllItems);
    }

    [Fact]
    public void PageSize_ZeroDefaultsToItemCount()
    {
        var items = new[] { "a", "b", "c" };
        var paged = new PagedList<string>(items, 0);

        Assert.Equal(3, paged.PageSize);
        Assert.Equal(1, paged.PageCount);
    }

    [Fact]
    public void PageSize_ZeroWithEmptyList_DefaultsToOne()
    {
        var paged = new PagedList<string>(Array.Empty<string>(), 0);

        Assert.Equal(1, paged.PageSize);
        Assert.Equal(0, paged.PageCount);
    }

    [Fact]
    public void NavigationRoundTrip_WorksCorrectly()
    {
        var items = Enumerable.Range(1, 20).Select(i => $"item-{i}").ToList();
        var paged = new PagedList<string>(items, 5);

        Assert.Equal(4, paged.PageCount);

        // Forward
        Assert.NotNull(paged.NextPage());   // page 1
        Assert.NotNull(paged.NextPage());   // page 2
        Assert.NotNull(paged.NextPage());   // page 3
        Assert.Null(paged.NextPage());      // can't go further

        // Back
        Assert.NotNull(paged.PreviousPage()); // page 2
        Assert.Equal(2, paged.CurrentPageIndex);

        // Jump
        var page = paged.SetPage(0);
        Assert.NotNull(page);
        Assert.Equal(0, paged.CurrentPageIndex);
        Assert.Equal(new[] { "item-1", "item-2", "item-3", "item-4", "item-5" }, page);
    }

    [Fact]
    public void HasMore_CorrectThroughNavigation()
    {
        var items = Enumerable.Range(1, 6).Select(i => $"item-{i}").ToList();
        var paged = new PagedList<string>(items, 3);

        Assert.True(paged.HasMore);    // page 0 of 2
        paged.NextPage();
        Assert.False(paged.HasMore);   // page 1 of 2 (last page)
    }

    [Fact]
    public void ExactlyOnePageOfItems_NoMore()
    {
        var items = new[] { "a", "b", "c" };
        var paged = new PagedList<string>(items, 3);

        Assert.Equal(1, paged.PageCount);
        Assert.False(paged.HasMore);
        Assert.Equal(3, paged.CurrentPage.Count);
    }
}

namespace Photostax;

/// <summary>
/// A paginated query result with page-based navigation and sub-query support.
/// Matches Rust's QueryResult API.
/// </summary>
public sealed class QueryResult
{
    private readonly IReadOnlyList<PhotoStack> _allStacks;
    private readonly int _pageSize;
    private int _currentPageIndex;

    internal QueryResult(IReadOnlyList<PhotoStack> allStacks, int pageSize)
    {
        _allStacks = allStacks;
        _pageSize = pageSize > 0 ? pageSize : Math.Max(allStacks.Count, 1);
        _currentPageIndex = 0;
    }

    /// <summary>Total number of matching stacks.</summary>
    public int TotalCount => _allStacks.Count;

    /// <summary>Number of stacks per page.</summary>
    public int PageSize => _pageSize;

    /// <summary>Total number of pages.</summary>
    public int PageCount => TotalCount == 0 ? 0 : (_allStacks.Count + _pageSize - 1) / _pageSize;

    /// <summary>Zero-based index of the current page.</summary>
    public int CurrentPageIndex => _currentPageIndex;

    /// <summary>Whether there are more pages after the current one.</summary>
    public bool HasMore => _currentPageIndex < PageCount - 1;

    /// <summary>Gets the stacks on the current page.</summary>
    public IReadOnlyList<PhotoStack> CurrentPage
    {
        get
        {
            if (TotalCount == 0) return Array.Empty<PhotoStack>();
            int start = _currentPageIndex * _pageSize;
            int count = Math.Min(_pageSize, _allStacks.Count - start);
            return _allStacks.Skip(start).Take(count).ToList().AsReadOnly();
        }
    }

    /// <summary>Gets stacks at a specific page index without changing the current page.</summary>
    public IReadOnlyList<PhotoStack>? GetPage(int pageIndex)
    {
        if (pageIndex < 0 || pageIndex >= PageCount) return null;
        int start = pageIndex * _pageSize;
        int count = Math.Min(_pageSize, _allStacks.Count - start);
        return _allStacks.Skip(start).Take(count).ToList().AsReadOnly();
    }

    /// <summary>Advances to the next page. Returns the page, or null if already on the last page.</summary>
    public IReadOnlyList<PhotoStack>? NextPage()
    {
        if (!HasMore) return null;
        _currentPageIndex++;
        return CurrentPage;
    }

    /// <summary>Goes back to the previous page. Returns the page, or null if already on the first page.</summary>
    public IReadOnlyList<PhotoStack>? PreviousPage()
    {
        if (_currentPageIndex <= 0) return null;
        _currentPageIndex--;
        return CurrentPage;
    }

    /// <summary>Jumps to a specific page. Returns the page, or null if the index is out of range.</summary>
    public IReadOnlyList<PhotoStack>? SetPage(int pageIndex)
    {
        if (pageIndex < 0 || pageIndex >= PageCount) return null;
        _currentPageIndex = pageIndex;
        return CurrentPage;
    }

    /// <summary>Gets all matching stacks across all pages.</summary>
    public IReadOnlyList<PhotoStack> AllStacks => _allStacks;

    /// <summary>
    /// Creates a sub-query that filters the stacks in this result.
    /// The sub-query operates on the snapshot already held by this QueryResult.
    /// </summary>
    /// <param name="query">Additional filter criteria, or null for no filtering.</param>
    /// <param name="pageSize">Page size for the sub-result, or null to inherit this result's page size.</param>
    public QueryResult Query(SearchQuery? query = null, int? pageSize = null)
    {
        var effectivePageSize = pageSize ?? _pageSize;
        if (query == null)
        {
            return new QueryResult(_allStacks, effectivePageSize);
        }

        var filtered = _allStacks.Where(stack => query.Matches(stack)).ToList().AsReadOnly();
        return new QueryResult(filtered, effectivePageSize);
    }
}

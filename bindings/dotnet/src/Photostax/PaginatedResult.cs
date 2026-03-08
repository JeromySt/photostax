namespace Photostax;

/// <summary>
/// A paginated result containing a page of items and pagination metadata.
/// </summary>
/// <typeparam name="T">The type of items in the result.</typeparam>
public sealed class PaginatedResult<T>
{
    /// <summary>
    /// Gets the items in this page.
    /// </summary>
    public IReadOnlyList<T> Items { get; }

    /// <summary>
    /// Gets the total number of items across all pages.
    /// </summary>
    public int TotalCount { get; }

    /// <summary>
    /// Gets the offset used for this page.
    /// </summary>
    public int Offset { get; }

    /// <summary>
    /// Gets the page size limit used for this page.
    /// </summary>
    public int Limit { get; }

    /// <summary>
    /// Gets a value indicating whether there are more items beyond this page.
    /// </summary>
    public bool HasMore { get; }

    /// <summary>
    /// Initializes a new instance of the <see cref="PaginatedResult{T}"/> class.
    /// </summary>
    /// <param name="items">The items in this page.</param>
    /// <param name="totalCount">Total number of items across all pages.</param>
    /// <param name="offset">The offset used for this page.</param>
    /// <param name="limit">The page size limit used for this page.</param>
    /// <param name="hasMore">Whether there are more items beyond this page.</param>
    internal PaginatedResult(IReadOnlyList<T> items, int totalCount, int offset, int limit, bool hasMore)
    {
        Items = items;
        TotalCount = totalCount;
        Offset = offset;
        Limit = limit;
        HasMore = hasMore;
    }
}

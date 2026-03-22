using System.Text.Json;
using System.Text.Json.Serialization;

namespace Photostax;

/// <summary>
/// Builder for search queries.
/// </summary>
public sealed class SearchQuery
{
    private string? _textQuery;
    private readonly List<(string Key, string Contains)> _exifFilters = [];
    private readonly List<(string Key, string Contains)> _customFilters = [];
    private bool? _hasBack;
    private bool? _hasEnhanced;
    private List<string>? _stackIds;

    /// <summary>
    /// Adds a text search filter.
    /// </summary>
    /// <param name="text">The text to search for.</param>
    /// <returns>This query builder for chaining.</returns>
    public SearchQuery WithText(string text)
    {
        _textQuery = text;
        return this;
    }

    /// <summary>
    /// Adds an EXIF tag filter.
    /// </summary>
    /// <param name="key">The EXIF tag key.</param>
    /// <param name="contains">The value to search for.</param>
    /// <returns>This query builder for chaining.</returns>
    public SearchQuery WithExifFilter(string key, string contains)
    {
        _exifFilters.Add((key, contains));
        return this;
    }

    /// <summary>
    /// Adds a custom tag filter.
    /// </summary>
    /// <param name="key">The custom tag key.</param>
    /// <param name="contains">The value to search for.</param>
    /// <returns>This query builder for chaining.</returns>
    public SearchQuery WithCustomFilter(string key, string contains)
    {
        _customFilters.Add((key, contains));
        return this;
    }

    /// <summary>
    /// Filters by whether the stack has a back image.
    /// </summary>
    /// <param name="hasBack">True to require back image, false to exclude it.</param>
    /// <returns>This query builder for chaining.</returns>
    public SearchQuery WithHasBack(bool hasBack)
    {
        _hasBack = hasBack;
        return this;
    }

    /// <summary>
    /// Filters by whether the stack has an enhanced image.
    /// </summary>
    /// <param name="hasEnhanced">True to require enhanced image, false to exclude it.</param>
    /// <returns>This query builder for chaining.</returns>
    public SearchQuery WithHasEnhanced(bool hasEnhanced)
    {
        _hasEnhanced = hasEnhanced;
        return this;
    }

    /// <summary>
    /// Filters to only include stacks whose ID is in the given list.
    /// </summary>
    /// <param name="ids">The stack IDs to include.</param>
    /// <returns>This query builder for chaining.</returns>
    public SearchQuery WithIds(params string[] ids)
    {
        _stackIds = [..ids];
        return this;
    }

    /// <summary>
    /// Tests whether a stack matches this query's criteria.
    /// Used for client-side sub-query filtering.
    /// </summary>
    internal bool Matches(PhotoStack stack)
    {
        // Text filter — match against name, folder
        if (!string.IsNullOrEmpty(_textQuery))
        {
            bool nameMatch = stack.Name?.Contains(_textQuery, StringComparison.OrdinalIgnoreCase) == true;
            bool folderMatch = stack.Folder?.Contains(_textQuery, StringComparison.OrdinalIgnoreCase) == true;
            if (!nameMatch && !folderMatch) return false;
        }

        // Has back filter
        if (_hasBack.HasValue && stack.Back.IsPresent != _hasBack.Value)
            return false;

        // Has enhanced filter
        if (_hasEnhanced.HasValue && stack.Enhanced.IsPresent != _hasEnhanced.Value)
            return false;

        // Stack IDs filter
        if (_stackIds is { Count: > 0 } && !_stackIds.Contains(stack.Id))
            return false;

        return true;
    }

    /// <summary>
    /// Serializes the query to JSON for FFI.
    /// </summary>
    internal string ToJson()
    {
        var query = new QueryDto
        {
            TextQuery = _textQuery,
            ExifFilters = _exifFilters.Count > 0 ? _exifFilters.Select(f => new[] { f.Key, f.Contains }).ToList() : null,
            CustomFilters = _customFilters.Count > 0 ? _customFilters.Select(f => new[] { f.Key, f.Contains }).ToList() : null,
            HasBack = _hasBack,
            HasEnhanced = _hasEnhanced,
            StackIds = _stackIds is { Count: > 0 } ? _stackIds : null
        };

        var options = new JsonSerializerOptions
        {
            PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower,
            DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull
        };

        return JsonSerializer.Serialize(query, options);
    }

    private sealed class QueryDto
    {
        [JsonPropertyName("text_query")]
        public string? TextQuery { get; set; }

        [JsonPropertyName("exif_filters")]
        public List<string[]>? ExifFilters { get; set; }

        [JsonPropertyName("custom_filters")]
        public List<string[]>? CustomFilters { get; set; }

        [JsonPropertyName("has_back")]
        public bool? HasBack { get; set; }

        [JsonPropertyName("has_enhanced")]
        public bool? HasEnhanced { get; set; }

        [JsonPropertyName("stack_ids")]
        public List<string>? StackIds { get; set; }
    }
}

using System.Text.Json;
using System.Text.Json.Serialization;

namespace Photostax;

/// <summary>
/// Metadata associated with a photo stack.
/// </summary>
public sealed class Metadata
{
    /// <summary>
    /// Gets the EXIF tags.
    /// </summary>
    public IReadOnlyDictionary<string, string> ExifTags { get; }

    /// <summary>
    /// Gets the XMP tags.
    /// </summary>
    public IReadOnlyDictionary<string, string> XmpTags { get; }

    /// <summary>
    /// Gets the custom tags.
    /// </summary>
    public IReadOnlyDictionary<string, object?> CustomTags { get; }

    /// <summary>
    /// Initializes a new instance of the <see cref="Metadata"/> class.
    /// </summary>
    public Metadata()
        : this(new Dictionary<string, string>(), new Dictionary<string, string>(), new Dictionary<string, object?>())
    {
    }

    /// <summary>
    /// Initializes a new instance of the <see cref="Metadata"/> class with the specified tags.
    /// </summary>
    /// <param name="exifTags">EXIF tags dictionary.</param>
    /// <param name="xmpTags">XMP tags dictionary.</param>
    /// <param name="customTags">Custom tags dictionary.</param>
    public Metadata(
        IReadOnlyDictionary<string, string> exifTags,
        IReadOnlyDictionary<string, string> xmpTags,
        IReadOnlyDictionary<string, object?> customTags)
    {
        ExifTags = exifTags ?? throw new ArgumentNullException(nameof(exifTags));
        XmpTags = xmpTags ?? throw new ArgumentNullException(nameof(xmpTags));
        CustomTags = customTags ?? throw new ArgumentNullException(nameof(customTags));
    }

    /// <summary>
    /// Creates a new metadata instance with the specified custom tag added or updated.
    /// </summary>
    /// <param name="key">The tag key.</param>
    /// <param name="value">The tag value.</param>
    /// <returns>A new metadata instance with the updated custom tag.</returns>
    public Metadata WithCustomTag(string key, object? value)
    {
        var newCustomTags = new Dictionary<string, object?>(CustomTags)
        {
            [key] = value
        };
        return new Metadata(ExifTags, XmpTags, newCustomTags);
    }

    /// <summary>
    /// Parses metadata from a JSON string.
    /// </summary>
    /// <param name="json">The JSON string.</param>
    /// <returns>The parsed metadata.</returns>
    internal static Metadata FromJson(string json)
    {
        if (string.IsNullOrEmpty(json) || json == "{}")
        {
            return new Metadata();
        }

        try
        {
            using var doc = JsonDocument.Parse(json);
            var root = doc.RootElement;

            var exifTags = ParseStringDictionary(root, "exif_tags");
            var xmpTags = ParseStringDictionary(root, "xmp_tags");
            var customTags = ParseObjectDictionary(root, "custom_tags");

            return new Metadata(exifTags, xmpTags, customTags);
        }
        catch (JsonException ex)
        {
            throw new PhotostaxException($"Failed to parse metadata JSON: {ex.Message}", ex);
        }
    }

    /// <summary>
    /// Serializes the metadata to a JSON string.
    /// </summary>
    /// <returns>The JSON string.</returns>
    internal string ToJson()
    {
        var options = new JsonSerializerOptions
        {
            PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower,
            DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull
        };

        var obj = new
        {
            exif_tags = ExifTags,
            xmp_tags = XmpTags,
            custom_tags = CustomTags
        };

        return JsonSerializer.Serialize(obj, options);
    }

    private static Dictionary<string, string> ParseStringDictionary(JsonElement root, string propertyName)
    {
        var dict = new Dictionary<string, string>();

        if (root.TryGetProperty(propertyName, out var element) && element.ValueKind == JsonValueKind.Object)
        {
            foreach (var prop in element.EnumerateObject())
            {
                dict[prop.Name] = prop.Value.GetString() ?? string.Empty;
            }
        }

        return dict;
    }

    private static Dictionary<string, object?> ParseObjectDictionary(JsonElement root, string propertyName)
    {
        var dict = new Dictionary<string, object?>();

        if (root.TryGetProperty(propertyName, out var element) && element.ValueKind == JsonValueKind.Object)
        {
            foreach (var prop in element.EnumerateObject())
            {
                dict[prop.Name] = JsonElementToObject(prop.Value);
            }
        }

        return dict;
    }

    private static object? JsonElementToObject(JsonElement element)
    {
        return element.ValueKind switch
        {
            JsonValueKind.String => element.GetString(),
            JsonValueKind.Number when element.TryGetInt64(out var l) && element.GetDouble() == l => l,
            JsonValueKind.Number => element.GetDouble(),
            JsonValueKind.True => true,
            JsonValueKind.False => false,
            JsonValueKind.Null => null,
            JsonValueKind.Array => element.EnumerateArray().Select(JsonElementToObject).ToList(),
            JsonValueKind.Object => element.EnumerateObject()
                .ToDictionary(p => p.Name, p => JsonElementToObject(p.Value)),
            _ => null
        };
    }
}

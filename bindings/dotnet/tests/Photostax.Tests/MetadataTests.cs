using Xunit;

namespace Photostax.Tests;

/// <summary>
/// Tests for the Metadata class.
/// </summary>
public class MetadataTests
{
    [Fact]
    public void DefaultConstructor_CreatesEmptyMetadata()
    {
        var metadata = new Metadata();

        Assert.Empty(metadata.ExifTags);
        Assert.Empty(metadata.XmpTags);
        Assert.Empty(metadata.CustomTags);
    }

    [Fact]
    public void Constructor_WithTags_SetsProperties()
    {
        var exifTags = new Dictionary<string, string> { ["Make"] = "EPSON" };
        var xmpTags = new Dictionary<string, string> { ["Creator"] = "Test" };
        var customTags = new Dictionary<string, object?> { ["album"] = "Family" };

        var metadata = new Metadata(exifTags, xmpTags, customTags);

        Assert.Equal("EPSON", metadata.ExifTags["Make"]);
        Assert.Equal("Test", metadata.XmpTags["Creator"]);
        Assert.Equal("Family", metadata.CustomTags["album"]);
    }

    [Fact]
    public void Constructor_NullExifTags_ThrowsArgumentNullException()
    {
        Assert.Throws<ArgumentNullException>(() =>
            new Metadata(null!, new Dictionary<string, string>(), new Dictionary<string, object?>()));
    }

    [Fact]
    public void Constructor_NullXmpTags_ThrowsArgumentNullException()
    {
        Assert.Throws<ArgumentNullException>(() =>
            new Metadata(new Dictionary<string, string>(), null!, new Dictionary<string, object?>()));
    }

    [Fact]
    public void Constructor_NullCustomTags_ThrowsArgumentNullException()
    {
        Assert.Throws<ArgumentNullException>(() =>
            new Metadata(new Dictionary<string, string>(), new Dictionary<string, string>(), null!));
    }

    [Fact]
    public void WithCustomTag_AddsNewTag()
    {
        var metadata = new Metadata();
        var updated = metadata.WithCustomTag("album", "Vacation");

        Assert.Empty(metadata.CustomTags);
        Assert.Equal("Vacation", updated.CustomTags["album"]);
    }

    [Fact]
    public void WithCustomTag_UpdatesExistingTag()
    {
        var customTags = new Dictionary<string, object?> { ["album"] = "Old" };
        var metadata = new Metadata(
            new Dictionary<string, string>(),
            new Dictionary<string, string>(),
            customTags);

        var updated = metadata.WithCustomTag("album", "New");

        Assert.Equal("Old", metadata.CustomTags["album"]);
        Assert.Equal("New", updated.CustomTags["album"]);
    }

    [Fact]
    public void WithCustomTag_PreservesOtherTags()
    {
        var exifTags = new Dictionary<string, string> { ["Make"] = "EPSON" };
        var xmpTags = new Dictionary<string, string> { ["Creator"] = "Test" };
        var metadata = new Metadata(exifTags, xmpTags, new Dictionary<string, object?>());

        var updated = metadata.WithCustomTag("album", "Family");

        Assert.Equal("EPSON", updated.ExifTags["Make"]);
        Assert.Equal("Test", updated.XmpTags["Creator"]);
        Assert.Equal("Family", updated.CustomTags["album"]);
    }

    [Fact]
    public void FromJson_EmptyObject_ReturnsEmptyMetadata()
    {
        var metadata = Metadata.FromJson("{}");

        Assert.Empty(metadata.ExifTags);
        Assert.Empty(metadata.XmpTags);
        Assert.Empty(metadata.CustomTags);
    }

    [Fact]
    public void FromJson_NullString_ReturnsEmptyMetadata()
    {
        var metadata = Metadata.FromJson(null!);

        Assert.Empty(metadata.ExifTags);
        Assert.Empty(metadata.XmpTags);
        Assert.Empty(metadata.CustomTags);
    }

    [Fact]
    public void FromJson_EmptyString_ReturnsEmptyMetadata()
    {
        var metadata = Metadata.FromJson("");

        Assert.Empty(metadata.ExifTags);
        Assert.Empty(metadata.XmpTags);
        Assert.Empty(metadata.CustomTags);
    }

    [Fact]
    public void FromJson_WithExifTags_ParsesCorrectly()
    {
        var json = """{"exif_tags":{"Make":"EPSON","Model":"FastFoto"}}""";
        var metadata = Metadata.FromJson(json);

        Assert.Equal("EPSON", metadata.ExifTags["Make"]);
        Assert.Equal("FastFoto", metadata.ExifTags["Model"]);
    }

    [Fact]
    public void FromJson_WithXmpTags_ParsesCorrectly()
    {
        var json = """{"xmp_tags":{"Creator":"John","Title":"Vacation"}}""";
        var metadata = Metadata.FromJson(json);

        Assert.Equal("John", metadata.XmpTags["Creator"]);
        Assert.Equal("Vacation", metadata.XmpTags["Title"]);
    }

    [Fact]
    public void FromJson_WithCustomTags_ParsesStringValues()
    {
        var json = """{"custom_tags":{"album":"Family","location":"Beach"}}""";
        var metadata = Metadata.FromJson(json);

        Assert.Equal("Family", metadata.CustomTags["album"]);
        Assert.Equal("Beach", metadata.CustomTags["location"]);
    }

    [Fact]
    public void FromJson_WithCustomTags_ParsesNumericValues()
    {
        var json = """{"custom_tags":{"rating":5,"year":2024}}""";
        var metadata = Metadata.FromJson(json);

        Assert.Equal(5L, (long)metadata.CustomTags["rating"]!);
        Assert.Equal(2024L, (long)metadata.CustomTags["year"]!);
    }

    [Fact]
    public void FromJson_WithCustomTags_ParsesBooleanValues()
    {
        var json = """{"custom_tags":{"favorite":true,"archived":false}}""";
        var metadata = Metadata.FromJson(json);

        Assert.Equal(true, metadata.CustomTags["favorite"]);
        Assert.Equal(false, metadata.CustomTags["archived"]);
    }

    [Fact]
    public void FromJson_WithCustomTags_ParsesNullValues()
    {
        var json = """{"custom_tags":{"notes":null}}""";
        var metadata = Metadata.FromJson(json);

        Assert.True(metadata.CustomTags.ContainsKey("notes"));
        Assert.Null(metadata.CustomTags["notes"]);
    }

    [Fact]
    public void FromJson_WithCustomTags_ParsesDoubleValues()
    {
        var json = """{"custom_tags":{"score":3.14}}""";
        var metadata = Metadata.FromJson(json);

        Assert.Equal(3.14, (double)metadata.CustomTags["score"]!);
    }

    [Fact]
    public void FromJson_WithCustomTags_ParsesArrayValues()
    {
        var json = """{"custom_tags":{"tags":["a","b","c"]}}""";
        var metadata = Metadata.FromJson(json);

        var list = Assert.IsType<List<object?>>(metadata.CustomTags["tags"]);
        Assert.Equal(3, list.Count);
        Assert.Equal("a", list[0]);
        Assert.Equal("b", list[1]);
        Assert.Equal("c", list[2]);
    }

    [Fact]
    public void FromJson_WithCustomTags_ParsesNestedObjectValues()
    {
        var json = """{"custom_tags":{"geo":{"lat":40.7,"lon":-74.0}}}""";
        var metadata = Metadata.FromJson(json);

        var nested = Assert.IsType<Dictionary<string, object?>>(metadata.CustomTags["geo"]);
        Assert.Equal(40.7, (double)nested["lat"]!);
        Assert.Equal(-74.0, (double)nested["lon"]!);
    }

    [Fact]
    public void WithCustomTag_NullValue_AddsNullEntry()
    {
        var metadata = new Metadata();
        var updated = metadata.WithCustomTag("notes", null);

        Assert.True(updated.CustomTags.ContainsKey("notes"));
        Assert.Null(updated.CustomTags["notes"]);
    }

    [Fact]
    public void FromJson_InvalidJson_ThrowsPhotostaxException()
    {
        Assert.Throws<PhotostaxException>(() => Metadata.FromJson("not valid json"));
    }

    [Fact]
    public void ToJson_EmptyMetadata_ReturnsValidJson()
    {
        var metadata = new Metadata();
        var json = metadata.ToJson();

        Assert.Contains("exif_tags", json);
        Assert.Contains("xmp_tags", json);
        Assert.Contains("custom_tags", json);
    }

    [Fact]
    public void ToJson_WithExifTags_IncludesTags()
    {
        var exifTags = new Dictionary<string, string> { ["Make"] = "Canon" };
        var metadata = new Metadata(exifTags, new Dictionary<string, string>(), new Dictionary<string, object?>());

        var json = metadata.ToJson();

        Assert.Contains("\"Make\":\"Canon\"", json);
    }

    [Fact]
    public void ToJson_WithCustomTags_IncludesTags()
    {
        var customTags = new Dictionary<string, object?> { ["album"] = "Travel" };
        var metadata = new Metadata(new Dictionary<string, string>(), new Dictionary<string, string>(), customTags);

        var json = metadata.ToJson();

        Assert.Contains("\"album\":\"Travel\"", json);
    }

    [Fact]
    public void Roundtrip_EmptyMetadata_PreservesData()
    {
        var original = new Metadata();
        var json = original.ToJson();
        var restored = Metadata.FromJson(json);

        Assert.Empty(restored.ExifTags);
        Assert.Empty(restored.XmpTags);
        Assert.Empty(restored.CustomTags);
    }

    [Fact]
    public void Roundtrip_WithTags_PreservesData()
    {
        var exifTags = new Dictionary<string, string> { ["Make"] = "EPSON" };
        var xmpTags = new Dictionary<string, string> { ["Creator"] = "Test" };
        var customTags = new Dictionary<string, object?> { ["album"] = "Family" };
        var original = new Metadata(exifTags, xmpTags, customTags);

        var json = original.ToJson();
        var restored = Metadata.FromJson(json);

        Assert.Equal("EPSON", restored.ExifTags["Make"]);
        Assert.Equal("Test", restored.XmpTags["Creator"]);
        Assert.Equal("Family", restored.CustomTags["album"]);
    }
}

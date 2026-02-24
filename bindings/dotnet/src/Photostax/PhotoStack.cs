namespace Photostax;

/// <summary>
/// Represents a photo stack with its associated images and metadata.
/// </summary>
public sealed class PhotoStack
{
    /// <summary>
    /// Gets the unique identifier for this stack.
    /// </summary>
    public string Id { get; }

    /// <summary>
    /// Gets the path to the original image, or null if not present.
    /// </summary>
    public string? OriginalPath { get; }

    /// <summary>
    /// Gets the path to the enhanced image, or null if not present.
    /// </summary>
    public string? EnhancedPath { get; }

    /// <summary>
    /// Gets the path to the back image, or null if not present.
    /// </summary>
    public string? BackPath { get; }

    /// <summary>
    /// Gets the metadata associated with this stack.
    /// </summary>
    public Metadata Metadata { get; }

    /// <summary>
    /// Gets a value indicating whether this stack has any image.
    /// </summary>
    public bool HasAnyImage => OriginalPath != null || EnhancedPath != null || BackPath != null;

    /// <summary>
    /// Gets the image format, or null if no images are present.
    /// </summary>
    public ImageFormat? Format
    {
        get
        {
            var path = OriginalPath ?? EnhancedPath ?? BackPath;
            if (path == null)
                return null;

            var extension = Path.GetExtension(path).ToLowerInvariant();
            return extension switch
            {
                ".jpg" or ".jpeg" => ImageFormat.Jpeg,
                ".png" => ImageFormat.Png,
                ".tif" or ".tiff" => ImageFormat.Tiff,
                _ => ImageFormat.Unknown
            };
        }
    }

    /// <summary>
    /// Initializes a new instance of the <see cref="PhotoStack"/> class.
    /// </summary>
    /// <param name="id">The stack identifier.</param>
    /// <param name="originalPath">Path to the original image.</param>
    /// <param name="enhancedPath">Path to the enhanced image.</param>
    /// <param name="backPath">Path to the back image.</param>
    /// <param name="metadata">The stack metadata.</param>
    internal PhotoStack(
        string id,
        string? originalPath,
        string? enhancedPath,
        string? backPath,
        Metadata metadata)
    {
        Id = id ?? throw new ArgumentNullException(nameof(id));
        OriginalPath = originalPath;
        EnhancedPath = enhancedPath;
        BackPath = backPath;
        Metadata = metadata ?? throw new ArgumentNullException(nameof(metadata));
    }
}

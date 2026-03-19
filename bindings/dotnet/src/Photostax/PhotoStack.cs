namespace Photostax;

/// <summary>
/// Represents a photo stack with its associated images and metadata.
/// </summary>
public sealed class PhotoStack
{
    /// <summary>
    /// Gets the unique identifier for this stack (opaque SHA-256 hash).
    /// </summary>
    public string Id { get; }

    /// <summary>
    /// Gets the human-readable display name (file stem, e.g. "IMG_001").
    /// </summary>
    public string Name { get; }

    /// <summary>
    /// Gets the subfolder within the repository, or null if root-level.
    /// </summary>
    public string? Folder { get; }

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
    internal PhotoStack(
        string id,
        string name,
        string? folder,
        string? originalPath,
        string? enhancedPath,
        string? backPath,
        Metadata metadata)
    {
        Id = id ?? throw new ArgumentNullException(nameof(id));
        Name = name ?? id;
        Folder = folder;
        OriginalPath = originalPath;
        EnhancedPath = enhancedPath;
        BackPath = backPath;
        Metadata = metadata ?? throw new ArgumentNullException(nameof(metadata));
    }
}

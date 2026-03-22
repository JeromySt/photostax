namespace Photostax;

/// <summary>
/// Flags indicating which image variants are present in a photo stack.
/// </summary>
[Flags]
public enum ImageVariants
{
    /// <summary>No images present.</summary>
    None = 0,

    /// <summary>Original (raw scan) image is present.</summary>
    Original = 1,

    /// <summary>Enhanced (color-corrected) image is present.</summary>
    Enhanced = 2,

    /// <summary>Back-of-photo image is present.</summary>
    Back = 4,
}

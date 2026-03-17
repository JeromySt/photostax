namespace Photostax;

/// <summary>
/// Controls which images in a photo stack are rotated.
/// </summary>
public enum RotationTarget
{
    /// <summary>Rotate all images (original + enhanced + back).</summary>
    All = 0,

    /// <summary>Rotate front-side images only (original + enhanced).</summary>
    Front = 1,

    /// <summary>Rotate back-side image only.</summary>
    Back = 2,
}

namespace Photostax;

/// <summary>
/// FastFoto scanner configuration profile.
/// Tells the scan engine how the scanner was configured so it can avoid
/// unnecessary disk I/O for image classification.
/// </summary>
public enum ScannerProfile
{
    /// <summary>Unknown config — ambiguous _a files are analysed via pixel variance (disk I/O).</summary>
    Auto = 0,

    /// <summary>Enhanced image and back capture both enabled. _a = enhanced, _b = back. No I/O.</summary>
    EnhancedAndBack = 1,

    /// <summary>Enhanced image enabled, back capture disabled. _a = enhanced. No I/O.</summary>
    EnhancedOnly = 2,

    /// <summary>Only original capture — no _a or _b expected. No I/O.</summary>
    OriginalOnly = 3,
}

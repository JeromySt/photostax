namespace Photostax;

/// <summary>
/// Phase of a multi-pass scan operation, reported via progress callbacks.
/// </summary>
public enum ScanPhase
{
    /// <summary>Pass 1: fast directory scan — discovering files and grouping stacks.</summary>
    Scanning = 0,

    /// <summary>Pass 2: classifying ambiguous _a images via pixel analysis (Auto profile only).</summary>
    Classifying = 1,

    /// <summary>All passes complete.</summary>
    Complete = 2,
}

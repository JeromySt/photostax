namespace Photostax;

/// <summary>
/// Result of checking a snapshot's staleness against the current repository state.
/// </summary>
public sealed class SnapshotStatus
{
    internal SnapshotStatus(bool isStale, int snapshotCount, int currentCount, int added, int removed)
    {
        IsStale = isStale;
        SnapshotCount = snapshotCount;
        CurrentCount = currentCount;
        Added = added;
        Removed = removed;
    }

    /// <summary>
    /// True when the filesystem no longer matches the snapshot (stacks were added or removed).
    /// </summary>
    public bool IsStale { get; }

    /// <summary>
    /// Number of stacks captured in the snapshot.
    /// </summary>
    public int SnapshotCount { get; }

    /// <summary>
    /// Number of stacks currently on disk.
    /// </summary>
    public int CurrentCount { get; }

    /// <summary>
    /// Number of new stacks found on disk but absent from the snapshot.
    /// </summary>
    public int Added { get; }

    /// <summary>
    /// Number of snapshot stacks no longer present on disk.
    /// </summary>
    public int Removed { get; }
}

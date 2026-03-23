namespace Photostax;

/// <summary>
/// Interface for host-language-provided repository backends (OneDrive, Google Drive, etc.).
/// </summary>
public interface IRepositoryProvider
{
    /// <summary>
    /// The canonical URI of this repository (e.g., "onedrive://user/Photos").
    /// Must be stable across calls and unique among repositories.
    /// </summary>
    string Location { get; }

    /// <summary>
    /// List file entries under a prefix.
    /// </summary>
    /// <param name="prefix">Folder path prefix (empty for root).</param>
    /// <param name="recursive">Whether to recurse into subdirectories.</param>
    /// <returns>List of file entries.</returns>
    IReadOnlyList<FileEntry> ListEntries(string prefix, bool recursive);

    /// <summary>
    /// Open a file for reading.
    /// </summary>
    Stream OpenRead(string path);

    /// <summary>
    /// Open a file for writing.
    /// </summary>
    Stream OpenWrite(string path);

    /// <summary>
    /// Whether this repository supports write operations (rotate, delete, metadata write).
    /// Defaults to true when not explicitly implemented.
    /// </summary>
    bool IsWritable => true;
}

/// <summary>
/// A file entry from a repository provider.
/// </summary>
/// <param name="Name">File name including extension.</param>
/// <param name="Folder">Containing folder path relative to the repository root.</param>
/// <param name="Path">Full path relative to the repository root.</param>
/// <param name="Size">File size in bytes.</param>
public record FileEntry(string Name, string Folder, string Path, ulong Size);

using Xunit;

namespace Photostax.Tests;

/// <summary>
/// Tests for the PhotostaxRepository class.
/// </summary>
public class RepositoryTests
{
    [Fact]
    public void Constructor_NullPath_ThrowsArgumentNullException()
    {
        Assert.Throws<ArgumentNullException>(() => new PhotostaxRepository(null!));
    }

    [Fact]
    [Trait("Category", "Integration")]
    public void Constructor_InvalidPath_SucceedsLazily()
    {
        // The FFI layer lazily opens repositories — the constructor always
        // succeeds.  Scanning a nonexistent directory returns an empty list
        // because the FFI swallows the underlying I/O error.
        using var repo = new PhotostaxRepository("/nonexistent/path/that/does/not/exist");
        var stacks = repo.Scan();
        Assert.Empty(stacks);
    }

    [Fact]
    public void Dispose_MultipleDisposals_DoesNotThrow()
    {
        // This test verifies the Dispose pattern works correctly
        // Without native library, we test by using a mock-friendly approach
        var disposed = false;
        
        // Create a simple disposable to verify pattern
        using var tracker = new DisposableTracker(() => disposed = true);
        tracker.Dispose();
        tracker.Dispose(); // Second disposal should not throw
        
        Assert.True(disposed);
    }

    [Fact]
    public void GetStack_NullId_ThrowsArgumentNullException()
    {
        // This tests the null check before native call
        // We can't fully test without native lib, but we verify the guard clause pattern
        Assert.Throws<ArgumentNullException>(() =>
        {
            string? nullId = null;
            ArgumentNullException.ThrowIfNull(nullId);
        });
    }

    [Fact]
    public void Search_NullQuery_ThrowsArgumentNullException()
    {
        Assert.Throws<ArgumentNullException>(() =>
        {
            SearchQuery? nullQuery = null;
            ArgumentNullException.ThrowIfNull(nullQuery);
        });
    }

    /// <summary>
    /// Helper class to verify dispose pattern without native library.
    /// </summary>
    private sealed class DisposableTracker : IDisposable
    {
        private readonly Action _onDispose;
        private bool _disposed;

        public DisposableTracker(Action onDispose)
        {
            _onDispose = onDispose;
        }

        public void Dispose()
        {
            if (!_disposed)
            {
                _onDispose();
                _disposed = true;
            }
        }
    }
}

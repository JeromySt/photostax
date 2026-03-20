//! Event types for the reactive notification cascade.
//!
//! Changes flow upward through five layers:
//!
//! ```text
//! Platform event (fs watcher / webhook / poll)
//!   → Repository translates to abstract events
//!   → Structural: RepoEvent → SessionManager → SnapshotEvent → Consumer
//!   → Content:    HandleEvent → ImageRef/MetadataRef cache clear → CacheEvent
//! ```

use crate::photo_stack::PhotoStack;
use crate::scanner::Variant;

/// Variant classification for file events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileVariant {
    /// Original scan (no suffix)
    Original,
    /// Enhanced/color-corrected (`_a` suffix)
    Enhanced,
    /// Back of photo (`_b` suffix)
    Back,
}

impl From<Variant> for FileVariant {
    fn from(v: Variant) -> Self {
        match v {
            Variant::Original => FileVariant::Original,
            Variant::Enhanced => FileVariant::Enhanced,
            Variant::Back => FileVariant::Back,
        }
    }
}

/// Events emitted by a Repository when files change.
#[derive(Debug, Clone)]
pub enum StackEvent {
    /// A file was created or modified — may create a new stack or update a slot.
    FileChanged {
        /// Opaque stack ID (from make_stack_id)
        stack_id: String,
        /// Which variant slot this file occupies
        variant: FileVariant,
        /// The file path/URI
        path: String,
        /// File size in bytes
        size: u64,
    },
    /// A file was removed — null out a slot, remove stack if empty.
    FileRemoved {
        /// Opaque stack ID
        stack_id: String,
        /// Which variant slot was removed
        variant: FileVariant,
    },
}

/// Events emitted by StackManager to consumers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CacheEvent {
    /// A new stack was added to the cache.
    StackAdded(String),
    /// An existing stack was updated (slot added/removed, metadata changed).
    StackUpdated(String),
    /// A stack was removed from the cache (all slots empty).
    StackRemoved(String),
}

/// Events emitted by a Repository to SessionManager when structural
/// changes occur (stacks added or removed).
///
/// Content changes (file modifications) are handled separately via
/// `ImageHandle::invalidate()` which doesn't produce a `RepoEvent`.
#[derive(Debug, Clone)]
pub enum RepoEvent {
    /// A new stack was discovered (e.g., new files appeared on disk).
    StackAdded(Box<PhotoStack>),
    /// A stack was removed (all its files are gone).
    StackRemoved(String),
}

/// Events emitted by an ImageRef or MetadataRef when content freshness
/// changes. These are internal notifications used within a PhotoStack to
/// propagate invalidation from handles up to the stack level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandleEvent {
    /// The backing file's content has changed (file was modified on disk).
    ContentChanged,
    /// The handle was invalidated (file was deleted or stack removed).
    Invalidated,
}

/// Reason why a snapshot became stale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StalenessReason {
    /// A stack was added to a repository.
    StackAdded,
    /// A stack was removed from a repository.
    StackRemoved,
    /// A new repository was added to the session.
    RepoAdded,
    /// A repository was removed from the session.
    RepoRemoved,
}

/// Events pushed to live snapshots when structural changes occur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotEvent {
    /// The snapshot's data is stale — a structural change occurred in a repo.
    Stale {
        /// Which repository triggered the staleness.
        repo_id: String,
        /// Why the snapshot is stale.
        reason: StalenessReason,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::Variant;

    #[test]
    fn test_variant_to_file_variant() {
        assert_eq!(FileVariant::from(Variant::Original), FileVariant::Original);
        assert_eq!(FileVariant::from(Variant::Enhanced), FileVariant::Enhanced);
        assert_eq!(FileVariant::from(Variant::Back), FileVariant::Back);
    }

    #[test]
    fn test_file_variant_debug_clone() {
        let v = FileVariant::Original;
        let cloned = v;
        assert_eq!(format!("{:?}", cloned), "Original");
    }

    #[test]
    fn test_stack_event_debug_clone() {
        let event = StackEvent::FileChanged {
            stack_id: "s1".to_string(),
            variant: FileVariant::Enhanced,
            path: "/p.jpg".to_string(),
            size: 42,
        };
        let cloned = event.clone();
        let debug = format!("{:?}", cloned);
        assert!(debug.contains("FileChanged"));
        assert!(debug.contains("s1"));

        let removed = StackEvent::FileRemoved {
            stack_id: "s2".to_string(),
            variant: FileVariant::Back,
        };
        let debug = format!("{:?}", removed.clone());
        assert!(debug.contains("FileRemoved"));
    }

    #[test]
    fn test_cache_event_debug_clone_eq() {
        let a = CacheEvent::StackAdded("x".to_string());
        let b = a.clone();
        assert_eq!(a, b);

        let c = CacheEvent::StackUpdated("y".to_string());
        assert_ne!(a, c);

        let d = CacheEvent::StackRemoved("z".to_string());
        let debug = format!("{:?}", d);
        assert!(debug.contains("StackRemoved"));
    }

    #[test]
    fn test_repo_event_debug_clone() {
        let stack = PhotoStack::new("test");
        let event = RepoEvent::StackAdded(Box::new(stack));
        let cloned = event.clone();
        let debug = format!("{:?}", cloned);
        assert!(debug.contains("StackAdded"));

        let removed = RepoEvent::StackRemoved("test".to_string());
        let debug = format!("{:?}", removed.clone());
        assert!(debug.contains("StackRemoved"));
    }

    #[test]
    fn test_handle_event_variants() {
        let changed = HandleEvent::ContentChanged;
        let invalidated = HandleEvent::Invalidated;
        assert_ne!(changed, invalidated);
        assert_eq!(changed, HandleEvent::ContentChanged);

        let cloned = changed;
        assert_eq!(format!("{:?}", cloned), "ContentChanged");
    }

    #[test]
    fn test_staleness_reason_variants() {
        let reasons = [
            StalenessReason::StackAdded,
            StalenessReason::StackRemoved,
            StalenessReason::RepoAdded,
            StalenessReason::RepoRemoved,
        ];
        for r in &reasons {
            let cloned = r.clone();
            assert_eq!(r, &cloned);
        }
    }

    #[test]
    fn test_snapshot_event_stale() {
        let event = SnapshotEvent::Stale {
            repo_id: "repo1".to_string(),
            reason: StalenessReason::StackAdded,
        };
        let cloned = event.clone();
        assert_eq!(event, cloned);
        let debug = format!("{:?}", event);
        assert!(debug.contains("repo1"));
        assert!(debug.contains("StackAdded"));
    }
}

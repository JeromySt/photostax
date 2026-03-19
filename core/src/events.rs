//! Event types for the reactive notification cascade.
//!
//! Changes flow upward through four layers:
//! File change → Repository (StackEvent) → StackManager (CacheEvent) → Consumer

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
}

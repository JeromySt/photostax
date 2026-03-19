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

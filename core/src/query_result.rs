//! Paginated query result with cursor navigation.

use std::collections::HashSet;
use std::fmt;

use tokio::sync::broadcast;

use crate::events::CacheEvent;
use crate::photo_stack::PhotoStack;
use crate::search::SearchQuery;
use crate::snapshot::ScanSnapshot;

/// Summary of changes since the QueryResult was created.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QueryDelta {
    /// Number of stacks added since query.
    pub added: usize,
    /// Number of stacks removed since query.
    pub removed: usize,
    /// Number of stacks modified since query.
    pub modified: usize,
}

impl QueryDelta {
    /// True if any changes have occurred.
    pub fn has_changes(&self) -> bool {
        self.added > 0 || self.removed > 0 || self.modified > 0
    }
}

/// Paginated query result wrapping a [`ScanSnapshot`].
///
/// Provides cursor-based navigation over a frozen snapshot of query results.
/// Created by [`StackManager::query()`](crate::stack_manager::StackManager::query).
///
/// # Examples
///
/// ```rust,ignore
/// # use photostax_core::query_result::QueryResult;
/// # use photostax_core::snapshot::ScanSnapshot;
/// // Iterate current page
/// let result = /* from query */;
/// for stack in result.current_page() {
///     println!("{}", stack.name());
/// }
///
/// // Navigate pages
/// while let Some(page) = result.next_page() {
///     for stack in page {
///         println!("{}", stack.name());
///     }
/// }
/// ```
pub struct QueryResult {
    snapshot: ScanSnapshot,
    page_size: usize,
    current_page_index: usize,
    /// Per-stack cursor for next_stack() iteration
    stack_cursor: usize,
    /// Tracks IDs that were added since this result was created.
    pending_added: HashSet<String>,
    /// Tracks IDs that were removed since this result was created.
    pending_removed: HashSet<String>,
    /// Tracks IDs that were modified since this result was created.
    pending_modified: HashSet<String>,
    /// Broadcast receiver for cache events (optional).
    event_rx: Option<broadcast::Receiver<CacheEvent>>,
}

impl Clone for QueryResult {
    fn clone(&self) -> Self {
        Self {
            snapshot: self.snapshot.clone(),
            page_size: self.page_size,
            current_page_index: self.current_page_index,
            stack_cursor: self.stack_cursor,
            pending_added: self.pending_added.clone(),
            pending_removed: self.pending_removed.clone(),
            pending_modified: self.pending_modified.clone(),
            event_rx: None, // broadcast::Receiver is not Clone
        }
    }
}

impl fmt::Debug for QueryResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QueryResult")
            .field("snapshot", &self.snapshot)
            .field("page_size", &self.page_size)
            .field("current_page_index", &self.current_page_index)
            .field("stack_cursor", &self.stack_cursor)
            .field("pending_added", &self.pending_added)
            .field("pending_removed", &self.pending_removed)
            .field("pending_modified", &self.pending_modified)
            .field("event_rx", &self.event_rx.as_ref().map(|_| ".."))
            .finish()
    }
}

impl QueryResult {
    /// Create a new QueryResult from a snapshot with the given page size.
    pub fn new(snapshot: ScanSnapshot, page_size: usize) -> Self {
        let page_size = if page_size == 0 { 1 } else { page_size };
        Self {
            snapshot,
            page_size,
            current_page_index: 0,
            stack_cursor: 0,
            pending_added: HashSet::new(),
            pending_removed: HashSet::new(),
            pending_modified: HashSet::new(),
            event_rx: None,
        }
    }

    /// Create a new QueryResult with a broadcast event receiver for
    /// reactive change notifications via [`pending_changes`](Self::pending_changes).
    pub fn new_with_events(
        snapshot: ScanSnapshot,
        page_size: usize,
        event_rx: broadcast::Receiver<CacheEvent>,
    ) -> Self {
        let page_size = if page_size == 0 { 1 } else { page_size };
        Self {
            snapshot,
            page_size,
            current_page_index: 0,
            stack_cursor: 0,
            pending_added: HashSet::new(),
            pending_removed: HashSet::new(),
            pending_modified: HashSet::new(),
            event_rx: Some(event_rx),
        }
    }

    /// Total number of stacks across all pages.
    pub fn total_count(&self) -> usize {
        self.snapshot.total_count()
    }

    /// Number of pages (ceiling division).
    pub fn page_count(&self) -> usize {
        let total = self.total_count();
        if total == 0 {
            0
        } else {
            total.div_ceil(self.page_size)
        }
    }

    /// The configured page size.
    pub fn page_size(&self) -> usize {
        self.page_size
    }

    /// The current page index (0-based).
    pub fn current_page_index(&self) -> usize {
        self.current_page_index
    }

    /// Get the stacks in the current page as a slice.
    pub fn current_page(&self) -> &[PhotoStack] {
        let start = self.current_page_index * self.page_size;
        let end = (start + self.page_size).min(self.snapshot.total_count());
        if start >= self.snapshot.total_count() {
            &[]
        } else {
            &self.snapshot.stacks()[start..end]
        }
    }

    /// Get a specific page by index (0-based). Returns None if out of bounds.
    pub fn get_page(&self, page_index: usize) -> Option<&[PhotoStack]> {
        let start = page_index * self.page_size;
        if start >= self.snapshot.total_count() {
            return None;
        }
        let end = (start + self.page_size).min(self.snapshot.total_count());
        Some(&self.snapshot.stacks()[start..end])
    }

    /// Advance to the next page. Returns the page, or `None` if already on the last page.
    pub fn next_page(&mut self) -> Option<&[PhotoStack]> {
        if self.current_page_index + 1 < self.page_count() {
            self.current_page_index += 1;
            Some(self.current_page())
        } else {
            None
        }
    }

    /// Go back to the previous page. Returns the page, or `None` if already on the first page.
    pub fn prev_page(&mut self) -> Option<&[PhotoStack]> {
        if self.current_page_index > 0 {
            self.current_page_index -= 1;
            Some(self.current_page())
        } else {
            None
        }
    }

    /// Jump to a specific page. Returns the page, or `None` if the index is out of range.
    pub fn set_page(&mut self, page_index: usize) -> Option<&[PhotoStack]> {
        if page_index < self.page_count() {
            self.current_page_index = page_index;
            Some(self.current_page())
        } else {
            None
        }
    }

    /// Whether there are more pages after the current one.
    pub fn has_more(&self) -> bool {
        self.current_page_index + 1 < self.page_count()
    }

    /// Get the next stack in the auto-paging iterator.
    ///
    /// Automatically advances `current_page_index` as the cursor
    /// crosses page boundaries.
    pub fn next_stack(&mut self) -> Option<&PhotoStack> {
        if self.stack_cursor >= self.snapshot.total_count() {
            return None;
        }
        // Update current page to reflect where the cursor is
        self.current_page_index = self.stack_cursor / self.page_size;
        let stack = &self.snapshot.stacks()[self.stack_cursor];
        self.stack_cursor += 1;
        Some(stack)
    }

    /// Reset the per-stack cursor to the beginning.
    pub fn reset_cursor(&mut self) {
        self.stack_cursor = 0;
        self.current_page_index = 0;
    }

    /// Access the underlying snapshot.
    pub fn snapshot(&self) -> &ScanSnapshot {
        &self.snapshot
    }

    /// Consume this result and return the underlying snapshot.
    pub fn into_snapshot(self) -> ScanSnapshot {
        self.snapshot
    }

    /// All stacks across all pages (convenience accessor).
    pub fn all_stacks(&self) -> &[PhotoStack] {
        self.snapshot.stacks()
    }

    /// Drain pending cache events and return a summary of changes since
    /// this result was created (or since the last call to `pending_changes`).
    ///
    /// Returns a [`QueryDelta`] describing how many stacks were added,
    /// removed, or modified. Only counts events relevant to the snapshot
    /// contained in this result.
    ///
    /// If this `QueryResult` was created without an event receiver
    /// (e.g. via [`new()`](Self::new) or by cloning), the returned delta
    /// is always empty.
    pub fn pending_changes(&mut self) -> QueryDelta {
        if let Some(ref mut rx) = self.event_rx {
            loop {
                match rx.try_recv() {
                    Ok(CacheEvent::StackAdded(id)) => {
                        if !self.snapshot.ids().contains(&id) {
                            self.pending_added.insert(id);
                        }
                    }
                    Ok(CacheEvent::StackRemoved(id)) => {
                        if self.snapshot.ids().contains(&id) {
                            self.pending_removed.insert(id);
                        }
                    }
                    Ok(CacheEvent::StackUpdated(id)) => {
                        if self.snapshot.ids().contains(&id) {
                            self.pending_modified.insert(id);
                        }
                    }
                    Err(broadcast::error::TryRecvError::Empty) => break,
                    Err(broadcast::error::TryRecvError::Lagged(_)) => continue,
                    Err(broadcast::error::TryRecvError::Closed) => break,
                }
            }
        }

        QueryDelta {
            added: self.pending_added.len(),
            removed: self.pending_removed.len(),
            modified: self.pending_modified.len(),
        }
    }

    /// Sub-query this result, producing a new [`QueryResult`] filtered
    /// from the internal snapshot.
    ///
    /// This enables composable queries: start with a broad query from
    /// [`StackManager::query()`](crate::stack_manager::StackManager::query),
    /// then narrow down with additional filters.
    ///
    /// # Arguments
    ///
    /// - `query` — filter criteria (None matches all stacks in this result)
    /// - `page_size` — page size for the new result (None inherits parent's)
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// // Get all stacks, then sub-query for those with back scans
    /// let all = mgr.query(None, Some(20), None, None)?;
    /// let with_backs = all.query(
    ///     Some(&SearchQuery::new().with_has_back(true)),
    ///     None,
    /// );
    /// ```
    pub fn query(&self, query: Option<&SearchQuery>, page_size: Option<usize>) -> QueryResult {
        let default_query = SearchQuery::new();
        let q = query.unwrap_or(&default_query);
        let filtered = self.snapshot.filter(q);
        let ps = page_size.unwrap_or(self.page_size);
        if let Some(ref rx) = self.event_rx {
            QueryResult::new_with_events(filtered, ps, rx.resubscribe())
        } else {
            QueryResult::new(filtered, ps)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_stacks(n: usize) -> Vec<PhotoStack> {
        (0..n)
            .map(|i| PhotoStack::new(format!("stack_{i}")))
            .collect()
    }

    fn make_result(n: usize, page_size: usize) -> QueryResult {
        let snapshot = ScanSnapshot::from_stacks(make_stacks(n));
        QueryResult::new(snapshot, page_size)
    }

    #[test]
    fn test_empty_result() {
        let result = make_result(0, 10);
        assert_eq!(result.total_count(), 0);
        assert_eq!(result.page_count(), 0);
        assert!(result.current_page().is_empty());
        assert!(!result.has_more());
    }

    #[test]
    fn test_empty_result_next_page() {
        let mut result = make_result(0, 10);
        assert!(result.next_page().is_none());
    }

    #[test]
    fn test_single_page() {
        let result = make_result(5, 10);
        assert_eq!(result.total_count(), 5);
        assert_eq!(result.page_count(), 1);
        assert_eq!(result.current_page().len(), 5);
        assert!(!result.has_more());
    }

    #[test]
    fn test_multiple_pages() {
        let result = make_result(25, 10);
        assert_eq!(result.total_count(), 25);
        assert_eq!(result.page_count(), 3);
        assert_eq!(result.page_size(), 10);

        // First page has 10 items
        assert_eq!(result.current_page().len(), 10);
        assert!(result.has_more());

        // Last page has 5 items
        assert_eq!(result.get_page(2).unwrap().len(), 5);
    }

    #[test]
    fn test_next_page_prev_page() {
        let mut result = make_result(25, 10);

        assert_eq!(result.current_page_index(), 0);

        // Advance to page 1
        assert_eq!(result.next_page().unwrap().len(), 10);
        assert_eq!(result.current_page_index(), 1);

        // Advance to page 2 (last, partial)
        assert_eq!(result.next_page().unwrap().len(), 5);
        assert_eq!(result.current_page_index(), 2);

        // Can't go further
        assert!(result.next_page().is_none());
        assert_eq!(result.current_page_index(), 2);

        // Go back
        assert_eq!(result.prev_page().unwrap().len(), 10);
        assert_eq!(result.current_page_index(), 1);

        assert!(result.prev_page().is_some());
        assert_eq!(result.current_page_index(), 0);

        // Can't go before 0
        assert!(result.prev_page().is_none());
        assert_eq!(result.current_page_index(), 0);
    }

    #[test]
    fn test_set_page() {
        let mut result = make_result(25, 10);

        assert_eq!(result.set_page(2).unwrap().len(), 5);
        assert_eq!(result.current_page_index(), 2);

        assert!(result.set_page(0).is_some());
        assert_eq!(result.current_page_index(), 0);

        // Invalid page
        assert!(result.set_page(3).is_none());
        assert_eq!(result.current_page_index(), 0); // unchanged

        assert!(result.set_page(100).is_none());
    }

    #[test]
    fn test_get_page() {
        let result = make_result(25, 10);

        let page0 = result.get_page(0).unwrap();
        assert_eq!(page0.len(), 10);
        assert_eq!(page0[0].name(), "stack_0");

        let page1 = result.get_page(1).unwrap();
        assert_eq!(page1.len(), 10);
        assert_eq!(page1[0].name(), "stack_10");

        let page2 = result.get_page(2).unwrap();
        assert_eq!(page2.len(), 5);
        assert_eq!(page2[0].name(), "stack_20");

        // Out of range
        assert!(result.get_page(3).is_none());
        assert!(result.get_page(100).is_none());
    }

    #[test]
    fn test_next_stack_auto_paging() {
        let mut result = make_result(5, 2);

        // Page 0: stack_0, stack_1
        let s0 = result.next_stack().unwrap();
        assert_eq!(s0.name(), "stack_0");
        assert_eq!(result.current_page_index(), 0);

        let s1 = result.next_stack().unwrap();
        assert_eq!(s1.name(), "stack_1");
        assert_eq!(result.current_page_index(), 0);

        // Page 1: stack_2, stack_3
        let s2 = result.next_stack().unwrap();
        assert_eq!(s2.name(), "stack_2");
        assert_eq!(result.current_page_index(), 1);

        let s3 = result.next_stack().unwrap();
        assert_eq!(s3.name(), "stack_3");
        assert_eq!(result.current_page_index(), 1);

        // Page 2: stack_4
        let s4 = result.next_stack().unwrap();
        assert_eq!(s4.name(), "stack_4");
        assert_eq!(result.current_page_index(), 2);

        // No more
        assert!(result.next_stack().is_none());
    }

    #[test]
    fn test_reset_cursor() {
        let mut result = make_result(5, 2);

        // Iterate through all
        while result.next_stack().is_some() {}
        assert!(result.next_stack().is_none());

        // Reset and iterate again
        result.reset_cursor();
        assert_eq!(result.current_page_index(), 0);

        let s0 = result.next_stack().unwrap();
        assert_eq!(s0.name(), "stack_0");

        // Can iterate all again
        let mut count = 1;
        while result.next_stack().is_some() {
            count += 1;
        }
        assert_eq!(count, 5);
    }

    #[test]
    fn test_page_size_zero_clamped() {
        let result = make_result(5, 0);
        assert_eq!(result.page_size(), 1);
        assert_eq!(result.page_count(), 5);
        assert_eq!(result.current_page().len(), 1);
    }

    #[test]
    fn test_into_snapshot() {
        let result = make_result(3, 10);
        assert_eq!(result.total_count(), 3);
        let snap = result.into_snapshot();
        assert_eq!(snap.total_count(), 3);
    }

    #[test]
    fn test_all_stacks() {
        let result = make_result(5, 2);
        assert_eq!(result.all_stacks().len(), 5);
        assert_eq!(result.all_stacks()[0].name(), "stack_0");
        assert_eq!(result.all_stacks()[4].name(), "stack_4");
    }

    #[test]
    fn test_snapshot_accessor() {
        let result = make_result(3, 10);
        let snap = result.snapshot();
        assert_eq!(snap.total_count(), 3);
        assert_eq!(snap.stacks().len(), 3);
    }

    // ── sub-query tests ─────────────────────────────────────────────────

    #[test]
    fn test_sub_query_no_filter() {
        let result = make_result(5, 10);
        let sub = result.query(None, None);
        assert_eq!(sub.total_count(), 5);
        assert_eq!(sub.page_size(), 10); // inherits parent's page_size
    }

    #[test]
    fn test_sub_query_inherits_page_size() {
        let result = make_result(10, 3);
        let sub = result.query(None, None);
        assert_eq!(sub.page_size(), 3);
    }

    #[test]
    fn test_sub_query_overrides_page_size() {
        let result = make_result(10, 3);
        let sub = result.query(None, Some(5));
        assert_eq!(sub.page_size(), 5);
    }

    #[test]
    fn test_sub_query_filter_by_ids() {
        let result = make_result(5, 10);
        let sub = result.query(
            Some(&SearchQuery::new().with_ids(vec!["stack_1".into(), "stack_3".into()])),
            None,
        );
        assert_eq!(sub.total_count(), 2);
        assert_eq!(sub.all_stacks()[0].name(), "stack_1");
        assert_eq!(sub.all_stacks()[1].name(), "stack_3");
    }

    #[test]
    fn test_sub_query_with_text() {
        let result = make_result(10, 5);
        let sub = result.query(Some(&SearchQuery::new().with_text("stack_7")), None);
        assert_eq!(sub.total_count(), 1);
        assert_eq!(sub.all_stacks()[0].name(), "stack_7");
    }

    #[test]
    fn test_sub_query_empty_result() {
        let result = make_result(5, 10);
        let sub = result.query(Some(&SearchQuery::new().with_text("nonexistent")), None);
        assert_eq!(sub.total_count(), 0);
        assert!(sub.all_stacks().is_empty());
    }

    #[test]
    fn test_chained_sub_queries() {
        // Start with 10 stacks
        let result = make_result(10, 5);
        assert_eq!(result.total_count(), 10);

        // First sub-query: stacks 0-4
        let sub1 = result.query(
            Some(&SearchQuery::new().with_ids((0..5).map(|i| format!("stack_{i}")).collect())),
            None,
        );
        assert_eq!(sub1.total_count(), 5);

        // Second sub-query on sub1: just stack_2
        let sub2 = sub1.query(
            Some(&SearchQuery::new().with_ids(vec!["stack_2".into()])),
            None,
        );
        assert_eq!(sub2.total_count(), 1);
        assert_eq!(sub2.all_stacks()[0].name(), "stack_2");
    }

    #[test]
    fn test_sub_query_with_pagination() {
        let result = make_result(10, 100);
        let sub = result.query(
            Some(&SearchQuery::new().with_ids((0..6).map(|i| format!("stack_{i}")).collect())),
            Some(2),
        );
        assert_eq!(sub.total_count(), 6);
        assert_eq!(sub.page_count(), 3);
        assert_eq!(sub.current_page().len(), 2);
        assert!(sub.has_more());
    }

    #[test]
    fn test_deep_sub_query_chain_inherits_events() {
        let (tx, _) = broadcast::channel::<CacheEvent>(16);

        // Level 0: root result with event receiver
        let stacks = make_stacks(20);
        let snapshot = ScanSnapshot::from_stacks(stacks);
        let mut root = QueryResult::new_with_events(snapshot, 100, tx.subscribe());

        // Build a 10-level deep sub-query chain
        const DEPTH: usize = 10;
        let mut chain: Vec<QueryResult> = Vec::with_capacity(DEPTH);
        chain.push(root.query(None, None));
        for i in 1..DEPTH {
            let sub = chain[i - 1].query(None, None);
            chain.push(sub);
        }

        // Send an event — it should reach every level
        tx.send(CacheEvent::StackAdded("new_stack".to_string()))
            .unwrap();

        // Root should see it
        let delta = root.pending_changes();
        assert_eq!(delta.added, 1, "root must see the added event");

        // Every level in the chain should see it
        for (i, qr) in chain.iter_mut().enumerate() {
            let delta = qr.pending_changes();
            assert_eq!(delta.added, 1, "depth {i} must see the added event");
        }
    }
}

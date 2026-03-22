//! Paginated query result with cursor navigation.

use crate::photo_stack::PhotoStack;
use crate::snapshot::ScanSnapshot;

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
///     println!("{}", stack.name);
/// }
///
/// // Navigate pages
/// while result.next_page() {
///     for stack in result.current_page() {
///         println!("{}", stack.name);
///     }
/// }
/// ```
#[derive(Debug, Clone)]
pub struct QueryResult {
    snapshot: ScanSnapshot,
    page_size: usize,
    current_page_index: usize,
    /// Per-stack cursor for next_stack() iteration
    stack_cursor: usize,
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

    /// Advance to the next page. Returns `true` if a next page existed.
    pub fn next_page(&mut self) -> bool {
        if self.current_page_index + 1 < self.page_count() {
            self.current_page_index += 1;
            true
        } else {
            false
        }
    }

    /// Go back to the previous page. Returns `true` if a previous page existed.
    pub fn prev_page(&mut self) -> bool {
        if self.current_page_index > 0 {
            self.current_page_index -= 1;
            true
        } else {
            false
        }
    }

    /// Jump to a specific page. Returns `true` if the page exists.
    pub fn set_page(&mut self, page_index: usize) -> bool {
        if page_index < self.page_count() {
            self.current_page_index = page_index;
            true
        } else {
            false
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
        assert!(!result.next_page());
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
        assert!(result.next_page());
        assert_eq!(result.current_page_index(), 1);
        assert_eq!(result.current_page().len(), 10);

        assert!(result.next_page());
        assert_eq!(result.current_page_index(), 2);
        assert_eq!(result.current_page().len(), 5);

        // Can't go further
        assert!(!result.next_page());
        assert_eq!(result.current_page_index(), 2);

        // Go back
        assert!(result.prev_page());
        assert_eq!(result.current_page_index(), 1);

        assert!(result.prev_page());
        assert_eq!(result.current_page_index(), 0);

        // Can't go before 0
        assert!(!result.prev_page());
        assert_eq!(result.current_page_index(), 0);
    }

    #[test]
    fn test_set_page() {
        let mut result = make_result(25, 10);

        assert!(result.set_page(2));
        assert_eq!(result.current_page_index(), 2);
        assert_eq!(result.current_page().len(), 5);

        assert!(result.set_page(0));
        assert_eq!(result.current_page_index(), 0);

        // Invalid page
        assert!(!result.set_page(3));
        assert_eq!(result.current_page_index(), 0); // unchanged

        assert!(!result.set_page(100));
    }

    #[test]
    fn test_get_page() {
        let result = make_result(25, 10);

        let page0 = result.get_page(0).unwrap();
        assert_eq!(page0.len(), 10);
        assert_eq!(page0[0].name, "stack_0");

        let page1 = result.get_page(1).unwrap();
        assert_eq!(page1.len(), 10);
        assert_eq!(page1[0].name, "stack_10");

        let page2 = result.get_page(2).unwrap();
        assert_eq!(page2.len(), 5);
        assert_eq!(page2[0].name, "stack_20");

        // Out of range
        assert!(result.get_page(3).is_none());
        assert!(result.get_page(100).is_none());
    }

    #[test]
    fn test_next_stack_auto_paging() {
        let mut result = make_result(5, 2);

        // Page 0: stack_0, stack_1
        let s0 = result.next_stack().unwrap();
        assert_eq!(s0.name, "stack_0");
        assert_eq!(result.current_page_index(), 0);

        let s1 = result.next_stack().unwrap();
        assert_eq!(s1.name, "stack_1");
        assert_eq!(result.current_page_index(), 0);

        // Page 1: stack_2, stack_3
        let s2 = result.next_stack().unwrap();
        assert_eq!(s2.name, "stack_2");
        assert_eq!(result.current_page_index(), 1);

        let s3 = result.next_stack().unwrap();
        assert_eq!(s3.name, "stack_3");
        assert_eq!(result.current_page_index(), 1);

        // Page 2: stack_4
        let s4 = result.next_stack().unwrap();
        assert_eq!(s4.name, "stack_4");
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
        assert_eq!(s0.name, "stack_0");

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
        assert_eq!(result.all_stacks()[0].name, "stack_0");
        assert_eq!(result.all_stacks()[4].name, "stack_4");
    }

    #[test]
    fn test_snapshot_accessor() {
        let result = make_result(3, 10);
        let snap = result.snapshot();
        assert_eq!(snap.total_count(), 3);
        assert_eq!(snap.stacks().len(), 3);
    }
}

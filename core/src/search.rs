//! Search and filtering for photo stacks.
//!
//! This module provides a fluent builder API for constructing search queries
//! and filtering photo stacks based on metadata and structural criteria.
//!
//! ## Query Builder Pattern
//!
//! [`SearchQuery`] uses the builder pattern for ergonomic filter construction:
//!
//! ```
//! use photostax_core::search::SearchQuery;
//!
//! let query = SearchQuery::new()
//!     .with_exif_filter("Make", "EPSON")
//!     .with_has_back(true)
//!     .with_text("birthday");
//! ```
//!
//! ## Filter Composition
//!
//! Multiple filters are combined with AND logic. A stack must match all
//! specified criteria to be included in results.
//!
//! ```
//! use photostax_core::search::{SearchQuery, filter_stacks};
//! use photostax_core::photo_stack::PhotoStack;
//!
//! let stacks: Vec<PhotoStack> = vec![]; // your stacks
//! let query = SearchQuery::new()
//!     .with_exif_filter("Make", "EPSON")   // AND
//!     .with_has_back(true)                  // AND
//!     .with_custom_filter("album", "2024"); // AND
//!
//! let results = filter_stacks(&stacks, &query);
//! ```

use serde::{Deserialize, Serialize};

use crate::photo_stack::PhotoStack;

/// Parameters for paginating a result set.
///
/// Used with [`paginate_stacks`] to fetch a specific page of results.
///
/// # Examples
///
/// ```
/// use photostax_core::search::PaginationParams;
///
/// // First page of 20 items
/// let page1 = PaginationParams { offset: 0, limit: 20 };
///
/// // Second page of 20 items
/// let page2 = PaginationParams { offset: 20, limit: 20 };
/// ```
#[derive(Debug, Clone)]
pub struct PaginationParams {
    /// The number of items to skip (0-based offset into the full result set).
    pub offset: usize,
    /// The maximum number of items to return per page.
    pub limit: usize,
}

/// A paginated result set containing a page of items and pagination metadata.
///
/// Returned by [`paginate_stacks`]. Contains the requested page of photo stacks
/// along with metadata needed for rendering pagination controls in a web UI.
///
/// # Examples
///
/// ```
/// use photostax_core::search::{paginate_stacks, PaginationParams};
/// use photostax_core::photo_stack::PhotoStack;
///
/// let stacks: Vec<PhotoStack> = (0..50)
///     .map(|i| PhotoStack::new(&format!("IMG_{i:03}")))
///     .collect();
///
/// let page = paginate_stacks(&stacks, &PaginationParams { offset: 0, limit: 10 });
/// assert_eq!(page.items.len(), 10);
/// assert_eq!(page.total_count, 50);
/// assert!(page.has_more);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResult<T> {
    /// The items in the current page.
    pub items: Vec<T>,
    /// Total number of items across all pages (before pagination).
    pub total_count: usize,
    /// The offset used for this page.
    pub offset: usize,
    /// The page size limit used for this page.
    pub limit: usize,
    /// Whether there are more items beyond this page.
    pub has_more: bool,
}

/// Paginate a slice of photo stacks.
///
/// Returns a [`PaginatedResult`] containing the requested page of stacks.
/// If `offset` is beyond the end of the slice, an empty page is returned
/// with the correct `total_count`.
///
/// # Arguments
///
/// * `stacks` - The full collection of photo stacks to paginate
/// * `params` - Pagination parameters (offset and limit)
///
/// # Examples
///
/// ```
/// use photostax_core::search::{paginate_stacks, PaginationParams};
/// use photostax_core::photo_stack::PhotoStack;
///
/// let stacks: Vec<PhotoStack> = (0..5)
///     .map(|i| PhotoStack::new(&format!("IMG_{i:03}")))
///     .collect();
///
/// // Get first page of 2
/// let page = paginate_stacks(&stacks, &PaginationParams { offset: 0, limit: 2 });
/// assert_eq!(page.items.len(), 2);
/// assert_eq!(page.items[0].id, "IMG_000");
/// assert_eq!(page.total_count, 5);
/// assert!(page.has_more);
///
/// // Get last page
/// let page = paginate_stacks(&stacks, &PaginationParams { offset: 4, limit: 2 });
/// assert_eq!(page.items.len(), 1);
/// assert!(!page.has_more);
/// ```
pub fn paginate_stacks(
    stacks: &[PhotoStack],
    params: &PaginationParams,
) -> PaginatedResult<PhotoStack> {
    let total_count = stacks.len();
    let items: Vec<PhotoStack> = stacks
        .iter()
        .skip(params.offset)
        .take(params.limit)
        .cloned()
        .collect();
    let has_more = params.offset + params.limit < total_count;

    PaginatedResult {
        items,
        total_count,
        offset: params.offset,
        limit: params.limit,
        has_more,
    }
}

/// Filter criteria for searching photo stacks.
///
/// Build queries using method chaining, then apply with [`filter_stacks`].
/// All filters use AND logic (a stack must match all criteria).
///
/// # Examples
///
/// Find EPSON photos with back scans containing "birthday":
///
/// ```
/// use photostax_core::search::SearchQuery;
///
/// let query = SearchQuery::new()
///     .with_exif_filter("Make", "EPSON")
///     .with_has_back(true)
///     .with_text("birthday");
/// ```
#[derive(Debug, Default, Clone)]
pub struct SearchQuery {
    /// EXIF tag filters (all must match). Key is tag name, value is substring to find.
    pub exif_filters: Vec<(String, String)>,

    /// Custom tag filters (all must match). Key is tag name, value is substring to find.
    pub custom_filters: Vec<(String, String)>,

    /// Free-text search across ID and all metadata values.
    pub text_query: Option<String>,

    /// Filter by presence of back scan (`Some(true)` = must have, `Some(false)` = must not have).
    pub has_back: Option<bool>,

    /// Filter by presence of enhanced scan (`Some(true)` = must have, `Some(false)` = must not have).
    pub has_enhanced: Option<bool>,
}

impl SearchQuery {
    /// Create a new empty search query.
    ///
    /// An empty query matches all stacks. Add filters with the `with_*` methods.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an EXIF tag filter (tag value must contain the given text, case-insensitive).
    ///
    /// # Arguments
    ///
    /// * `key` - EXIF tag name (e.g., `"Make"`, `"Model"`, `"DateTime"`)
    /// * `contains` - Substring to search for in the tag value
    ///
    /// # Examples
    ///
    /// ```
    /// use photostax_core::search::SearchQuery;
    ///
    /// let query = SearchQuery::new()
    ///     .with_exif_filter("Make", "EPSON")
    ///     .with_exif_filter("DateTime", "2024");
    /// ```
    pub fn with_exif_filter(mut self, key: impl Into<String>, contains: impl Into<String>) -> Self {
        self.exif_filters.push((key.into(), contains.into()));
        self
    }

    /// Add a custom tag filter (tag value string must contain the given text, case-insensitive).
    ///
    /// # Arguments
    ///
    /// * `key` - Custom tag name (e.g., `"album"`, `"ocr_text"`, `"people"`)
    /// * `contains` - Substring to search for in the tag value
    ///
    /// # Examples
    ///
    /// ```
    /// use photostax_core::search::SearchQuery;
    ///
    /// let query = SearchQuery::new()
    ///     .with_custom_filter("album", "Family")
    ///     .with_custom_filter("ocr_text", "Happy Birthday");
    /// ```
    pub fn with_custom_filter(
        mut self,
        key: impl Into<String>,
        contains: impl Into<String>,
    ) -> Self {
        self.custom_filters.push((key.into(), contains.into()));
        self
    }

    /// Set a free-text search across all metadata.
    ///
    /// Searches the stack ID, all EXIF tag values, and all custom tag values
    /// for the given text (case-insensitive).
    ///
    /// # Examples
    ///
    /// ```
    /// use photostax_core::search::SearchQuery;
    ///
    /// // Find any stack mentioning "birthday"
    /// let query = SearchQuery::new().with_text("birthday");
    /// ```
    pub fn with_text(mut self, query: impl Into<String>) -> Self {
        self.text_query = Some(query.into());
        self
    }

    /// Filter for stacks that have/don't have a back scan.
    ///
    /// # Arguments
    ///
    /// * `has_back` - `true` to require back scans, `false` to exclude them
    ///
    /// # Examples
    ///
    /// ```
    /// use photostax_core::search::SearchQuery;
    ///
    /// // Only photos with back scans (for OCR processing)
    /// let with_backs = SearchQuery::new().with_has_back(true);
    ///
    /// // Only photos without back scans
    /// let without_backs = SearchQuery::new().with_has_back(false);
    /// ```
    pub fn with_has_back(mut self, has_back: bool) -> Self {
        self.has_back = Some(has_back);
        self
    }

    /// Filter for stacks that have/don't have an enhanced scan.
    ///
    /// # Arguments
    ///
    /// * `has_enhanced` - `true` to require enhanced images, `false` to exclude them
    ///
    /// # Examples
    ///
    /// ```
    /// use photostax_core::search::SearchQuery;
    ///
    /// // Only color-corrected photos
    /// let enhanced = SearchQuery::new().with_has_enhanced(true);
    /// ```
    pub fn with_has_enhanced(mut self, has_enhanced: bool) -> Self {
        self.has_enhanced = Some(has_enhanced);
        self
    }
}

/// Filter a collection of photo stacks based on a search query.
///
/// Returns a new vector containing only stacks that match all query criteria.
/// An empty query returns all stacks.
///
/// # Arguments
///
/// * `stacks` - Slice of photo stacks to filter
/// * `query` - Search criteria (all filters are AND'd together)
///
/// # Examples
///
/// ```
/// use photostax_core::search::{SearchQuery, filter_stacks};
/// use photostax_core::photo_stack::PhotoStack;
///
/// let stacks = vec![
///     PhotoStack::new("IMG_001"),
///     PhotoStack::new("IMG_002"),
/// ];
///
/// // Filter by ID containing "001"
/// let query = SearchQuery::new().with_text("001");
/// let results = filter_stacks(&stacks, &query);
/// assert_eq!(results.len(), 1);
/// assert_eq!(results[0].id, "IMG_001");
/// ```
pub fn filter_stacks(stacks: &[PhotoStack], query: &SearchQuery) -> Vec<PhotoStack> {
    stacks
        .iter()
        .filter(|stack| matches_query(stack, query))
        .cloned()
        .collect()
}

/// Check if a single stack matches all query criteria.
fn matches_query(stack: &PhotoStack, query: &SearchQuery) -> bool {
    // Check structural filters
    if let Some(has_back) = query.has_back {
        if stack.back.is_some() != has_back {
            return false;
        }
    }

    if let Some(has_enhanced) = query.has_enhanced {
        if stack.enhanced.is_some() != has_enhanced {
            return false;
        }
    }

    // Check EXIF tag filters
    for (key, contains) in &query.exif_filters {
        match stack.metadata.exif_tags.get(key) {
            Some(value) if value.to_lowercase().contains(&contains.to_lowercase()) => {}
            _ => return false,
        }
    }

    // Check custom tag filters
    for (key, contains) in &query.custom_filters {
        match stack.metadata.custom_tags.get(key) {
            Some(value) => {
                let value_str = match value {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                if !value_str.to_lowercase().contains(&contains.to_lowercase()) {
                    return false;
                }
            }
            None => return false,
        }
    }

    // Check free-text search
    if let Some(ref text) = query.text_query {
        let text_lower = text.to_lowercase();
        let found_in_id = stack.id.to_lowercase().contains(&text_lower);
        let found_in_exif = stack
            .metadata
            .exif_tags
            .values()
            .any(|v| v.to_lowercase().contains(&text_lower));
        let found_in_custom = stack.metadata.custom_tags.values().any(|v| {
            let s = match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            s.to_lowercase().contains(&text_lower)
        });

        if !found_in_id && !found_in_exif && !found_in_custom {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::photo_stack::{Metadata, PhotoStack};
    use std::path::PathBuf;

    fn make_stack(
        id: &str,
        has_back: bool,
        exif: Vec<(&str, &str)>,
        custom: Vec<(&str, &str)>,
    ) -> PhotoStack {
        let mut metadata = Metadata::default();
        for (k, v) in exif {
            metadata.exif_tags.insert(k.to_string(), v.to_string());
        }
        for (k, v) in custom {
            metadata
                .custom_tags
                .insert(k.to_string(), serde_json::json!(v));
        }
        PhotoStack {
            id: id.to_string(),
            original: Some(PathBuf::from(format!("{id}.jpg"))),
            enhanced: Some(PathBuf::from(format!("{id}_a.jpg"))),
            back: if has_back {
                Some(PathBuf::from(format!("{id}_b.jpg")))
            } else {
                None
            },
            metadata,
        }
    }

    #[test]
    fn test_filter_by_text() {
        let stacks = vec![
            make_stack(
                "IMG_001",
                true,
                vec![],
                vec![("ocr_text", "Happy Birthday")],
            ),
            make_stack(
                "IMG_002",
                true,
                vec![],
                vec![("ocr_text", "Merry Christmas")],
            ),
        ];

        let q = SearchQuery::new().with_text("birthday");
        let results = filter_stacks(&stacks, &q);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "IMG_001");
    }

    #[test]
    fn test_filter_by_has_back() {
        let stacks = vec![
            make_stack("IMG_001", true, vec![], vec![]),
            make_stack("IMG_002", false, vec![], vec![]),
        ];

        let q = SearchQuery::new().with_has_back(true);
        let results = filter_stacks(&stacks, &q);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "IMG_001");
    }

    #[test]
    fn test_filter_by_exif_tag() {
        let stacks = vec![
            make_stack("IMG_001", false, vec![("Make", "EPSON")], vec![]),
            make_stack("IMG_002", false, vec![("Make", "Canon")], vec![]),
        ];

        let q = SearchQuery::new().with_exif_filter("Make", "epson");
        let results = filter_stacks(&stacks, &q);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "IMG_001");
    }

    #[test]
    fn test_combined_filters() {
        let stacks = vec![
            make_stack(
                "IMG_001",
                true,
                vec![("Make", "EPSON")],
                vec![("ocr_text", "Hello")],
            ),
            make_stack(
                "IMG_002",
                true,
                vec![("Make", "EPSON")],
                vec![("ocr_text", "World")],
            ),
            make_stack(
                "IMG_003",
                false,
                vec![("Make", "EPSON")],
                vec![("ocr_text", "Hello")],
            ),
        ];

        let q = SearchQuery::new()
            .with_exif_filter("Make", "EPSON")
            .with_has_back(true)
            .with_text("hello");
        let results = filter_stacks(&stacks, &q);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "IMG_001");
    }

    #[test]
    fn test_filter_by_has_enhanced_true() {
        let stacks = vec![make_stack("IMG_001", false, vec![], vec![]), {
            let mut s = make_stack("IMG_002", false, vec![], vec![]);
            s.enhanced = None;
            s
        }];

        let q = SearchQuery::new().with_has_enhanced(true);
        let results = filter_stacks(&stacks, &q);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "IMG_001");
    }

    #[test]
    fn test_filter_by_has_enhanced_false() {
        let stacks = vec![make_stack("IMG_001", false, vec![], vec![]), {
            let mut s = make_stack("IMG_002", false, vec![], vec![]);
            s.enhanced = None;
            s
        }];

        let q = SearchQuery::new().with_has_enhanced(false);
        let results = filter_stacks(&stacks, &q);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "IMG_002");
    }

    #[test]
    fn test_filter_empty_stacks_list() {
        let stacks: Vec<PhotoStack> = vec![];
        let q = SearchQuery::new().with_text("anything");
        let results = filter_stacks(&stacks, &q);
        assert!(results.is_empty());
    }

    #[test]
    fn test_filter_no_filters_returns_all() {
        let stacks = vec![
            make_stack("IMG_001", true, vec![], vec![]),
            make_stack("IMG_002", false, vec![], vec![]),
            make_stack("IMG_003", true, vec![], vec![]),
        ];

        let q = SearchQuery::new();
        let results = filter_stacks(&stacks, &q);
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_multiple_exif_filters_and_semantics() {
        let stacks = vec![
            make_stack(
                "IMG_001",
                false,
                vec![("Make", "EPSON"), ("Model", "FF-680W")],
                vec![],
            ),
            make_stack(
                "IMG_002",
                false,
                vec![("Make", "EPSON"), ("Model", "Other")],
                vec![],
            ),
            make_stack(
                "IMG_003",
                false,
                vec![("Make", "Canon"), ("Model", "FF-680W")],
                vec![],
            ),
        ];

        let q = SearchQuery::new()
            .with_exif_filter("Make", "EPSON")
            .with_exif_filter("Model", "FF-680W");
        let results = filter_stacks(&stacks, &q);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "IMG_001");
    }

    #[test]
    fn test_multiple_custom_filters_and_semantics() {
        let stacks = vec![
            make_stack(
                "IMG_001",
                false,
                vec![],
                vec![("tag1", "value1"), ("tag2", "value2")],
            ),
            make_stack(
                "IMG_002",
                false,
                vec![],
                vec![("tag1", "value1"), ("tag2", "other")],
            ),
            make_stack(
                "IMG_003",
                false,
                vec![],
                vec![("tag1", "other"), ("tag2", "value2")],
            ),
        ];

        let q = SearchQuery::new()
            .with_custom_filter("tag1", "value1")
            .with_custom_filter("tag2", "value2");
        let results = filter_stacks(&stacks, &q);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "IMG_001");
    }

    #[test]
    fn test_text_query_matches_stack_id() {
        let stacks = vec![
            make_stack("FamilyPhotos_001", false, vec![], vec![]),
            make_stack("VacationPics_002", false, vec![], vec![]),
        ];

        let q = SearchQuery::new().with_text("Family");
        let results = filter_stacks(&stacks, &q);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "FamilyPhotos_001");
    }

    #[test]
    fn test_custom_tag_with_non_string_values() {
        let mut stack = PhotoStack::new("IMG_001");
        stack.original = Some(PathBuf::from("IMG_001.jpg"));
        stack
            .metadata
            .custom_tags
            .insert("count".to_string(), serde_json::json!(42));
        stack
            .metadata
            .custom_tags
            .insert("tags".to_string(), serde_json::json!(["a", "b", "c"]));

        let stacks = vec![stack];

        // Number value search
        let q = SearchQuery::new().with_custom_filter("count", "42");
        let results = filter_stacks(&stacks, &q);
        assert_eq!(results.len(), 1);

        // Array value search (should match stringified form)
        let q2 = SearchQuery::new().with_custom_filter("tags", "b");
        let results2 = filter_stacks(&stacks, &q2);
        assert_eq!(results2.len(), 1);
    }

    #[test]
    fn test_search_query_default() {
        let q = SearchQuery::default();
        assert!(q.exif_filters.is_empty());
        assert!(q.custom_filters.is_empty());
        assert!(q.text_query.is_none());
        assert!(q.has_back.is_none());
        assert!(q.has_enhanced.is_none());
    }

    #[test]
    fn test_filter_missing_exif_tag() {
        let stacks = vec![
            make_stack("IMG_001", false, vec![("Make", "EPSON")], vec![]),
            make_stack("IMG_002", false, vec![], vec![]), // No Make tag
        ];

        let q = SearchQuery::new().with_exif_filter("Make", "EPSON");
        let results = filter_stacks(&stacks, &q);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "IMG_001");
    }

    #[test]
    fn test_filter_missing_custom_tag() {
        let stacks = vec![
            make_stack("IMG_001", false, vec![], vec![("ocr", "text")]),
            make_stack("IMG_002", false, vec![], vec![]), // No ocr tag
        ];

        let q = SearchQuery::new().with_custom_filter("ocr", "text");
        let results = filter_stacks(&stacks, &q);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "IMG_001");
    }

    #[test]
    fn test_filter_has_back_false() {
        let stacks = vec![
            make_stack("IMG_001", true, vec![], vec![]),
            make_stack("IMG_002", false, vec![], vec![]),
        ];

        let q = SearchQuery::new().with_has_back(false);
        let results = filter_stacks(&stacks, &q);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "IMG_002");
    }

    #[test]
    fn test_text_search_in_exif() {
        let stacks = vec![
            make_stack(
                "IMG_001",
                false,
                vec![("Software", "EPSON FastFoto")],
                vec![],
            ),
            make_stack(
                "IMG_002",
                false,
                vec![("Software", "Adobe Photoshop")],
                vec![],
            ),
        ];

        let q = SearchQuery::new().with_text("FastFoto");
        let results = filter_stacks(&stacks, &q);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "IMG_001");
    }

    // ── Pagination tests ──────────────────────────────────────────

    fn make_stacks(n: usize) -> Vec<PhotoStack> {
        (0..n)
            .map(|i| make_stack(&format!("IMG_{i:03}"), false, vec![], vec![]))
            .collect()
    }

    #[test]
    fn test_paginate_first_page() {
        let stacks = make_stacks(10);
        let page = paginate_stacks(
            &stacks,
            &PaginationParams {
                offset: 0,
                limit: 3,
            },
        );

        assert_eq!(page.items.len(), 3);
        assert_eq!(page.items[0].id, "IMG_000");
        assert_eq!(page.items[2].id, "IMG_002");
        assert_eq!(page.total_count, 10);
        assert_eq!(page.offset, 0);
        assert_eq!(page.limit, 3);
        assert!(page.has_more);
    }

    #[test]
    fn test_paginate_middle_page() {
        let stacks = make_stacks(10);
        let page = paginate_stacks(
            &stacks,
            &PaginationParams {
                offset: 3,
                limit: 3,
            },
        );

        assert_eq!(page.items.len(), 3);
        assert_eq!(page.items[0].id, "IMG_003");
        assert_eq!(page.items[2].id, "IMG_005");
        assert!(page.has_more);
    }

    #[test]
    fn test_paginate_last_page_partial() {
        let stacks = make_stacks(10);
        let page = paginate_stacks(
            &stacks,
            &PaginationParams {
                offset: 9,
                limit: 3,
            },
        );

        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].id, "IMG_009");
        assert_eq!(page.total_count, 10);
        assert!(!page.has_more);
    }

    #[test]
    fn test_paginate_exact_boundary() {
        let stacks = make_stacks(6);
        let page = paginate_stacks(
            &stacks,
            &PaginationParams {
                offset: 3,
                limit: 3,
            },
        );

        assert_eq!(page.items.len(), 3);
        assert!(!page.has_more);
    }

    #[test]
    fn test_paginate_offset_beyond_end() {
        let stacks = make_stacks(5);
        let page = paginate_stacks(
            &stacks,
            &PaginationParams {
                offset: 100,
                limit: 10,
            },
        );

        assert!(page.items.is_empty());
        assert_eq!(page.total_count, 5);
        assert!(!page.has_more);
    }

    #[test]
    fn test_paginate_empty_collection() {
        let stacks: Vec<PhotoStack> = vec![];
        let page = paginate_stacks(
            &stacks,
            &PaginationParams {
                offset: 0,
                limit: 10,
            },
        );

        assert!(page.items.is_empty());
        assert_eq!(page.total_count, 0);
        assert!(!page.has_more);
    }

    #[test]
    fn test_paginate_single_item_page() {
        let stacks = make_stacks(5);
        let page = paginate_stacks(
            &stacks,
            &PaginationParams {
                offset: 2,
                limit: 1,
            },
        );

        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].id, "IMG_002");
        assert!(page.has_more);
    }

    #[test]
    fn test_paginate_all_items_in_one_page() {
        let stacks = make_stacks(3);
        let page = paginate_stacks(
            &stacks,
            &PaginationParams {
                offset: 0,
                limit: 100,
            },
        );

        assert_eq!(page.items.len(), 3);
        assert_eq!(page.total_count, 3);
        assert!(!page.has_more);
    }

    #[test]
    fn test_paginate_after_filter() {
        let stacks = vec![
            make_stack("IMG_001", true, vec![], vec![]),
            make_stack("IMG_002", false, vec![], vec![]),
            make_stack("IMG_003", true, vec![], vec![]),
            make_stack("IMG_004", false, vec![], vec![]),
            make_stack("IMG_005", true, vec![], vec![]),
        ];

        let q = SearchQuery::new().with_has_back(true);
        let filtered = filter_stacks(&stacks, &q);
        assert_eq!(filtered.len(), 3);

        let page = paginate_stacks(
            &filtered,
            &PaginationParams {
                offset: 0,
                limit: 2,
            },
        );
        assert_eq!(page.items.len(), 2);
        assert_eq!(page.items[0].id, "IMG_001");
        assert_eq!(page.items[1].id, "IMG_003");
        assert_eq!(page.total_count, 3);
        assert!(page.has_more);
    }
}

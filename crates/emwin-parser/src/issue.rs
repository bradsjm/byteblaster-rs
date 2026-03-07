//! Product parse issue tracking.
//!
//! This module defines the [`ProductParseIssue`] type for tracking non-fatal parsing
//! problems encountered when processing weather products. Issues are collected during
//! enrichment and returned alongside successfully parsed data.

use serde::Serialize;

/// Represents a non-fatal parsing issue encountered during product processing.
///
/// Issues are collected during product enrichment and returned alongside
/// successfully parsed data. Issues are not fatal errors but indicate
/// potential data quality problems that consumers may want to be aware of.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProductParseIssue {
    /// Issue category/type (e.g., "text_product_parse", "header_parse")
    pub kind: &'static str,
    /// Machine-readable error code (e.g., "missing_wmo_line", "invalid_vtec")
    pub code: &'static str,
    /// Human-readable error message
    pub message: String,
    /// Optional line content where the issue occurred
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<String>,
}

impl ProductParseIssue {
    /// Creates a new product parse issue.
    ///
    /// # Arguments
    ///
    /// * `kind` - Issue category/type
    /// * `code` - Machine-readable error code
    /// * `message` - Human-readable error message
    /// * `line` - Optional line content where the issue occurred
    pub(crate) fn new(
        kind: &'static str,
        code: &'static str,
        message: impl Into<String>,
        line: Option<String>,
    ) -> Self {
        Self {
            kind,
            code,
            message: message.into(),
            line,
        }
    }
}

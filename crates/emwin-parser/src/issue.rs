//! Product parse issue tracking.
//!
//! This module defines the [`ProductParseIssue`] type for tracking non-fatal parsing
//! problems encountered when processing weather products. Issues are collected during
//! enrichment and returned alongside successfully parsed data.

use serde::Serialize;

/// Non-fatal parse issue collected during product enrichment.
///
/// The parser uses issues instead of hard failures when it can still return useful structured data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProductParseIssue {
    /// Issue family such as `text_product_parse` or `header_parse`.
    pub kind: &'static str,
    /// Stable machine-readable code.
    pub code: &'static str,
    /// Human-readable detail for logs or diagnostics.
    pub message: String,
    /// Relevant source line when retaining it is useful for debugging.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<String>,
}

impl ProductParseIssue {
    /// Creates a new parse issue from structured identifiers plus a message.
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

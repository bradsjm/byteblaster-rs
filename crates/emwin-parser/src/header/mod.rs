//! WMO/AFOS text product header parsing.
//!
//! This module provides parsing and enrichment for WMO (World Meteorological Organization)
//! headers and AFOS (Automation of Field Operations and Services) Product Identifier Lines (PILs).
//!
//! ## Components
//!
//! - [`parse_text_product`]: Main entry point for parsing text product headers
//! - [`TextProductHeader`]: Parsed header containing WMO fields and AFOS PIL
//! - [`WmoHeader`]: WMO header without AFOS PIL (for bulletins)
//! - [`enrich_header`]: Adds semantic metadata to parsed headers
//! - [`BbbKind`]: Classification of BBB amendment/correction indicators

mod enrich;
mod parser;

pub use enrich::{BbbKind, TextProductEnrichment, enrich_header};
pub(crate) use parser::{
    ParsedTextProductRef, ParsedWmoBulletinRef, condition_text_bytes,
    parse_text_product_conditioned_ref, parse_wmo_bulletin_conditioned_ref,
};
pub use parser::{ParserError, TextProductHeader, WmoHeader, parse_text_product};

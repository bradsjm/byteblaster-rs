//! # byteblaster-parser
//!
//! WMO/AFOS text product parsing library for weather and aviation meteorological products.
//!
//! This library provides parsing, enrichment, and lookup capabilities for WMO (World Meteorological
//! Organization) and AFOS (Automation of Field Operations and Services) formatted text products
//! commonly used in meteorological broadcasting systems.
//!
//! ## Features
//!
//! - **WMO header parsing**: Extracts TTAAII, CCCC, DDHHMM, and optional BBB indicators
//! - **AFOS PIL extraction**: Parses the Product Identifier Line (PIL) with robust error handling
//! - **Text conditioning**: Handles SOH/ETX control characters, null bytes, missing LDM sequences
//! - **PIL lookup**: Built-in product type descriptions for common meteorological products
//! - **Header enrichment**: Classifies BBB indicators (Amendment, Correction, Delayed Repeat)
//! - **Zero-copy parsing**: Efficient byte-based parsing with minimal allocations
//!
//! ## Example
//!
//! ```rust
//! use byteblaster_parser::{parse_text_product, enrich_header};
//!
//! let raw_text = b"000 \nFXUS61 KBOX 022101\nAFDBOX\nAREA FORECAST DISCUSSION\n";
//! let header = parse_text_product(raw_text)?;
//! let enriched = enrich_header(&header);
//!
//! println!("AFOS PIL: {}", header.afos);
//! println!("Station: {}", header.cccc);
//! if let Some(desc) = enriched.pil_description {
//!     println!("Product type: {}", desc);
//! }
//! # Ok::<(), byteblaster_parser::ParserError>(())
//! ```

mod enrich;
mod lookup;
mod parser;

pub use enrich::{BbbKind, TextProductEnrichment, enrich_header};
pub use lookup::{
    PIL_ENTRY_COUNT, PIL_GENERATED_AT_UTC, PIL_SOURCE_COMMIT, PIL_SOURCE_PATH, PIL_SOURCE_REPO,
    pil_description,
};
pub use parser::{ParserError, TextProductHeader, parse_text_product};

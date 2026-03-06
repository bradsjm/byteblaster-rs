//! # emwin-parser
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
//! use emwin_parser::{parse_text_product, enrich_header};
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
//! # Ok::<(), emwin_parser::ParserError>(())
//! ```

mod body;
mod data;
mod header;
mod product;

pub use body::{
    HvtecCause, HvtecCode, HvtecRecord, HvtecSeverity, LatLonPolygon, UgcClass, UgcCode,
    UgcSection, VtecAction, VtecCode, parse_hvtec_codes, parse_latlon_polygons, parse_ugc_sections,
    parse_vtec_codes,
};
pub use data::{
    PIL_ENTRY_COUNT, PIL_GENERATED_AT_UTC, PIL_SOURCE_COMMIT, PIL_SOURCE_PATH, PIL_SOURCE_REPO,
    PilCatalogEntry, pil_catalog_entry, pil_description, wmo_prefix_for_pil,
};
pub use header::{
    BbbKind, ParserError, TextProductEnrichment, TextProductHeader, enrich_header,
    parse_text_product,
};
pub use product::{
    ProductEnrichment, ProductEnrichmentSource, ProductParseWarning, enrich_product,
};

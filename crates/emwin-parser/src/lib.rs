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
//! - **UGC geography lookup**: Built-in county and zone catalogs keyed by canonical UGC codes
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
mod dcp;
mod fd;
mod geo;
mod header;
mod issue;
mod metar;
mod pipeline;
mod pirep;
mod product;
mod sigmet;
mod taf;
mod time;

pub use body::{
    HvtecCause, HvtecCode, HvtecRecord, HvtecSeverity, LatLonPolygon, ProductBody, TimeMotLocEntry,
    UgcArea, UgcClass, UgcCode, UgcSection, VtecCode, WindHailEntry, WindHailKind, enrich_body,
    parse_hvtec_codes, parse_hvtec_codes_with_issues, parse_latlon_polygons,
    parse_latlon_polygons_with_issues, parse_time_mot_loc_entries,
    parse_time_mot_loc_entries_with_issues, parse_ugc_sections, parse_ugc_sections_with_issues,
    parse_vtec_codes, parse_vtec_codes_with_issues, parse_wind_hail_entries,
    parse_wind_hail_entries_with_issues,
};
pub use data::{
    NWSLID_ENTRY_COUNT, NWSLID_GENERATED_AT_UTC, NwslidEntry, PIL_ENTRY_COUNT,
    PIL_GENERATED_AT_UTC, PilCatalogEntry, UGC_COUNTY_ENTRY_COUNT, UGC_COUNTY_SOURCE_PATH,
    UGC_GENERATED_AT_UTC, UGC_ZONE_ENTRY_COUNT, UGC_ZONE_SOURCE_PATH, UgcLocationEntry,
    WMO_OFFICE_ENTRY_COUNT, WMO_OFFICE_GENERATED_AT_UTC, WMO_OFFICE_SOURCE_PATH, WmoOfficeEntry,
    nwslid_entry, pil_catalog_entry, pil_description, ugc_county_entry, ugc_zone_entry,
    wmo_office_entry, wmo_prefix_for_pil,
};
pub use dcp::DcpBulletin;
pub use fd::{FdBulletin, FdForecast, FdLevelForecast};
pub use geo::{
    GeoBounds, GeoPoint, bounds_contains, distance_miles, point_in_polygon, polygon_bounds,
};
pub use header::{
    BbbKind, ParserError, TextProductEnrichment, TextProductHeader, WmoHeader, enrich_header,
    parse_text_product,
};
pub use issue::ProductParseIssue;
pub use metar::{MetarBulletin, MetarReport, MetarReportKind};
pub use pirep::{PirepBulletin, PirepKind, PirepReport};
pub use product::{ProductEnrichment, ProductEnrichmentSource, enrich_product};
pub use sigmet::{SigmetBulletin, SigmetSection};
pub use taf::TafBulletin;

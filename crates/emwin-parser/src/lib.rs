//! Parse and enrich EMWIN text products.
//!
//! `emwin-parser` turns raw bulletin bytes into structured headers, body features, and catalog
//! lookups used elsewhere in the workspace. The crate prefers explicit parsing steps and borrowed
//! views internally so higher-level code can opt into owned data only at stable API boundaries.

mod body;
mod cf6;
mod cwa;
mod data;
mod dcp;
mod dsm;
mod fd;
mod geo;
mod header;
mod hml;
mod issue;
mod lsr;
mod metar;
mod mos;
mod pipeline;
mod pirep;
mod product;
mod sigmet;
mod taf;
mod time;
mod wwp;

pub use body::{
    BodyExtractorId, HvtecCause, HvtecCode, HvtecRecord, HvtecSeverity, LatLonPolygon, ProductBody,
    TimeMotLocEntry, UgcArea, UgcClass, UgcCode, UgcSection, VtecCode, WindHailEntry, WindHailKind,
    enrich_body, parse_hvtec_codes, parse_hvtec_codes_with_issues, parse_latlon_polygons,
    parse_latlon_polygons_with_issues, parse_time_mot_loc_entries,
    parse_time_mot_loc_entries_with_issues, parse_ugc_sections, parse_ugc_sections_with_issues,
    parse_vtec_codes, parse_vtec_codes_with_issues, parse_wind_hail_entries,
    parse_wind_hail_entries_with_issues,
};
pub use cf6::{Cf6Bulletin, Cf6DayRow};
pub use cwa::{CwaBulletin, CwaGeometry, CwaGeometryKind};
pub use data::{
    NWSLID_ENTRY_COUNT, NWSLID_GENERATED_AT_UTC, NwslidEntry, TEXT_PRODUCT_ENTRY_COUNT,
    TEXT_PRODUCT_GENERATED_AT_UTC, TextProductBodyBehavior, TextProductCatalogEntry,
    TextProductRouting, UGC_COUNTY_ENTRY_COUNT, UGC_COUNTY_SOURCE_PATH, UGC_GENERATED_AT_UTC,
    UGC_ZONE_ENTRY_COUNT, UGC_ZONE_SOURCE_PATH, UgcLocationEntry, WMO_OFFICE_ENTRY_COUNT,
    WMO_OFFICE_GENERATED_AT_UTC, WMO_OFFICE_SOURCE_PATH, WmoOfficeEntry, nwslid_entry,
    pil_description, text_product_catalog_entry, ugc_county_entry, ugc_zone_entry,
    wmo_office_entry, wmo_prefix_for_pil,
};
pub use dcp::DcpBulletin;
pub use dsm::{DsmBulletin, DsmSummary};
pub use fd::{FdBulletin, FdForecast, FdLevelForecast};
pub use geo::{
    GeoBounds, GeoPoint, bounds_contains, distance_miles, point_in_polygon, polygon_bounds,
};
pub use header::{
    BbbKind, ParserError, TextProductEnrichment, TextProductHeader, WmoHeader, enrich_header,
    parse_text_product,
};
pub use hml::{HmlBulletin, HmlDatum, HmlDocument, HmlSeries};
pub use issue::ProductParseIssue;
pub use lsr::{LsrBulletin, LsrReport};
pub use metar::{MetarBulletin, MetarReport, MetarReportKind};
pub use mos::{MosBulletin, MosForecastRow, MosSection};
pub use pirep::{PirepBulletin, PirepKind, PirepReport};
pub use product::{ProductEnrichment, ProductEnrichmentSource, enrich_product};
pub use sigmet::{SigmetBulletin, SigmetSection};
pub use taf::TafBulletin;
pub use wwp::{WwpBulletin, WwpWatchType};

//! Parse and enrich EMWIN text products.
//!
//! `emwin-parser` turns raw bulletin bytes into structured headers, body features, and catalog
//! lookups used elsewhere in the workspace. The crate prefers explicit parsing steps and borrowed
//! views internally so higher-level code can opt into owned data only at stable API boundaries.

mod body;
mod data;
mod enrichment;
mod geo;
mod header;
mod issue;
mod pipeline;
mod specialized;
mod time;

pub use body::{
    BodyExtractorId, GenericBody, HvtecCause, HvtecCode, HvtecRecord, HvtecSeverity, LatLonPolygon,
    ProductBody, TimeMotLocEntry, UgcArea, UgcClass, UgcCode, UgcSection, VtecCode, VtecEventBody,
    VtecEventSegment, WindHailEntry, WindHailKind, enrich_body, parse_hvtec_codes,
    parse_hvtec_codes_with_issues, parse_latlon_polygons, parse_latlon_polygons_with_issues,
    parse_time_mot_loc_entries, parse_time_mot_loc_entries_with_issues, parse_ugc_sections,
    parse_ugc_sections_with_issues, parse_vtec_codes, parse_vtec_codes_with_issues,
    parse_wind_hail_entries, parse_wind_hail_entries_with_issues,
};
pub use data::{
    NWSLID_ENTRY_COUNT, NWSLID_GENERATED_AT_UTC, NwslidEntry, TEXT_PRODUCT_ENTRY_COUNT,
    TEXT_PRODUCT_GENERATED_AT_UTC, TextProductBodyBehavior, TextProductCatalogEntry,
    TextProductRouting, UGC_COUNTY_ENTRY_COUNT, UGC_COUNTY_SOURCE_PATH, UGC_GENERATED_AT_UTC,
    UGC_ZONE_ENTRY_COUNT, UGC_ZONE_SOURCE_PATH, UgcLocationEntry, WMO_OFFICE_ENTRY_COUNT,
    WMO_OFFICE_GENERATED_AT_UTC, WMO_OFFICE_SOURCE_PATH, WmoOfficeEntry, nwslid_entry,
    pil_description, text_product_catalog_entry, ugc_county_entry, ugc_zone_entry,
    wmo_office_entry, wmo_prefix_for_pil,
};
pub use enrichment::{ProductEnrichment, ProductEnrichmentSource, enrich_product};
pub use geo::{
    GeoBounds, GeoPoint, bounds_contains, distance_miles, point_in_polygon, polygon_bounds,
};
pub use header::{
    BbbKind, ParserError, TextProductEnrichment, TextProductHeader, WmoHeader, enrich_header,
    parse_text_product,
};
pub use issue::ProductParseIssue;
pub use specialized::{
    Cf6Bulletin, Cf6DayRow, CwaBulletin, CwaGeometry, CwaGeometryKind, DcpBulletin, DsmBulletin,
    DsmSummary, FdBulletin, FdForecast, FdLevelForecast, HmlBulletin, HmlDatum, HmlDocument,
    HmlSeries, LsrBulletin, LsrReport, MetarBulletin, MetarReport, MetarReportKind, MosBulletin,
    MosForecastRow, MosSection, PirepBulletin, PirepKind, PirepReport, SigmetBulletin,
    SigmetSection, TafBulletin, WwpBulletin, WwpWatchType,
};

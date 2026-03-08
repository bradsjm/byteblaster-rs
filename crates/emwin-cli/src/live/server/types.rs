use crate::cmd::event_output::{frame_event_name, frame_event_to_json};
use crate::live::file_pipeline::CompletedFileMetadata;
use crate::live::server_support::{RetainedFiles, file_download_url, wildcard_match};
use emwin_parser::{
    BbbKind, GeoPoint, HvtecCause, HvtecCode, HvtecRecord, HvtecSeverity, ProductBody,
    ProductEnrichment, ProductEnrichmentSource, ProductParseIssue, UgcSection, VtecCode,
    WindHailEntry, WindHailKind, bounds_contains, point_in_polygon,
};
use emwin_protocol::qbt_receiver::{QbtFrameEvent, QbtReceiverTelemetrySnapshot};
use emwin_protocol::wxwire_receiver::{WxWireReceiverFrameEvent, WxWireReceiverTelemetrySnapshot};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Instant;
use tokio::sync::{broadcast, watch};

#[derive(Debug, Clone)]
pub(crate) struct BroadcastEvent {
    pub(crate) id: u64,
    pub(crate) kind: EventKind,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CompletedFilePayload {
    #[serde(flatten)]
    pub(crate) metadata: CompletedFileMetadata,
    pub(crate) download_url: String,
}

impl CompletedFilePayload {
    pub(crate) fn from_metadata(metadata: CompletedFileMetadata) -> Self {
        let download_url = file_download_url(&metadata.filename);
        Self {
            metadata,
            download_url,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "receiver", rename_all = "snake_case")]
pub(crate) enum TelemetryPayload {
    Unavailable,
    Qbt(QbtReceiverTelemetrySnapshot),
    WxWire(WxWireReceiverTelemetrySnapshot),
}

#[derive(Debug, Clone)]
pub(crate) enum EventKind {
    Connected { endpoint: String },
    Disconnected,
    QbtFrame(QbtFrameEvent),
    WxWireFrame(WxWireReceiverFrameEvent),
    FileComplete(Box<CompletedFilePayload>),
    Telemetry(TelemetryPayload),
    Error { message: String },
}

impl EventKind {
    pub(crate) fn event_name(&self) -> &'static str {
        match self {
            Self::Connected { .. } => "connected",
            Self::Disconnected => "disconnected",
            Self::QbtFrame(frame) => frame_event_name(frame),
            Self::WxWireFrame(frame) => match frame {
                WxWireReceiverFrameEvent::File(_) => "file",
                WxWireReceiverFrameEvent::Warning(_) => "warning",
                _ => "unknown",
            },
            Self::FileComplete(_) => "file_complete",
            Self::Telemetry(_) => "telemetry",
            Self::Error { .. } => "error",
        }
    }

    pub(crate) fn to_json(&self) -> serde_json::Value {
        match self {
            Self::Connected { endpoint } => serde_json::json!({ "endpoint": endpoint }),
            Self::Disconnected => serde_json::json!({}),
            Self::QbtFrame(frame) => frame_event_to_json(frame, 0),
            Self::WxWireFrame(frame) => match frame {
                WxWireReceiverFrameEvent::File(file) => serde_json::json!({
                    "type": "file",
                    "filename": file.filename,
                    "length": file.data.len(),
                    "subject": file.subject,
                    "id": file.id,
                    "issue_utc": crate::live::shared::unix_seconds(file.issue_utc),
                    "ttaaii": file.ttaaii,
                    "cccc": file.cccc,
                    "awipsid": file.awipsid,
                }),
                WxWireReceiverFrameEvent::Warning(warning) => serde_json::json!({
                    "type": "warning",
                    "warning": format!("{warning:?}"),
                }),
                _ => serde_json::json!({
                    "type": "unknown",
                }),
            },
            Self::FileComplete(file) => {
                serde_json::to_value(file).unwrap_or_else(|_| serde_json::json!({}))
            }
            Self::Telemetry(snapshot) => {
                serde_json::to_value(snapshot).unwrap_or_else(|_| serde_json::json!({}))
            }
            Self::Error { message } => serde_json::json!({ "message": message }),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct EventFilter {
    pub(crate) event_names: Option<BTreeSet<String>>,
    pub(crate) file: FileEventFilter,
}

impl Eq for EventFilter {}

impl EventFilter {
    #[cfg(test)]
    pub(crate) fn from_query(query: EventsQuery) -> Self {
        Self::try_from_query(query).expect("query should compile")
    }

    pub(crate) fn try_from_query(query: EventsQuery) -> Result<Self, EventFilterQueryError> {
        let location = LocationFilter::try_from_query(&query)?;
        Ok(Self {
            event_names: csv_values(query.event.as_deref(), normalize_lower),
            file: FileEventFilter {
                filename_pattern: query.filename,
                size: SizeRange {
                    min: query.min_size,
                    max: query.max_size,
                },
                product: ProductFilter {
                    source: csv_values(query.source.as_deref(), normalize_lower),
                    pil: csv_values(query.pil.as_deref(), normalize_upper),
                    family: csv_values(query.family.as_deref(), normalize_lower),
                    container: csv_values(query.container.as_deref(), normalize_lower),
                    wmo_prefix: csv_values(query.wmo_prefix.as_deref(), normalize_upper),
                    office: csv_values(query.office.as_deref(), normalize_upper),
                    office_city: csv_values(query.office_city.as_deref(), normalize_lower),
                    office_state: csv_values(query.office_state.as_deref(), normalize_upper),
                    bbb_kind: csv_values(query.bbb_kind.as_deref(), normalize_lower),
                },
                header: HeaderFilter {
                    cccc: csv_values(query.cccc.as_deref(), normalize_upper),
                    ttaaii: csv_values(query.ttaaii.as_deref(), normalize_upper),
                    afos: csv_values(query.afos.as_deref(), normalize_upper),
                    bbb: csv_values(query.bbb.as_deref(), normalize_upper),
                },
                issues: IssueFilter {
                    has_issues: parse_optional_bool(query.has_issues.as_deref()),
                    kinds: csv_values(query.issue_kind.as_deref(), normalize_lower),
                    codes: csv_values(query.issue_code.as_deref(), normalize_lower),
                },
                geo: GeoFilter {
                    states: csv_values(query.state.as_deref(), normalize_upper),
                    counties: csv_values(query.county.as_deref(), normalize_upper),
                    zones: csv_values(query.zone.as_deref(), normalize_upper),
                    fire_zones: csv_values(query.fire_zone.as_deref(), normalize_upper),
                    marine_zones: csv_values(query.marine_zone.as_deref(), normalize_upper),
                },
                vtec: VtecFilter {
                    phenomena: csv_values(query.vtec_phenomena.as_deref(), normalize_upper),
                    significance: csv_values(query.vtec_significance.as_deref(), normalize_upper),
                    action: csv_values(query.vtec_action.as_deref(), normalize_upper),
                    office: csv_values(query.vtec_office.as_deref(), normalize_upper),
                    etn: csv_numbers(query.etn.as_deref()),
                },
                hvtec: HvtecFilter {
                    present: parse_optional_bool(query.has_hvtec.as_deref()),
                    nwslid: csv_values(query.hvtec_nwslid.as_deref(), normalize_upper),
                    severity: csv_values(query.hvtec_severity.as_deref(), normalize_lower),
                    cause: csv_values(query.hvtec_cause.as_deref(), normalize_lower),
                    record: csv_values(query.hvtec_record.as_deref(), normalize_lower),
                },
                wind_hail: WindHailFilter {
                    present: parse_optional_bool(query.has_wind_hail.as_deref()),
                    kinds: csv_values(query.wind_hail_kind.as_deref(), normalize_lower),
                    min_wind_mph: query.min_wind_mph,
                    min_hail_inches: query.min_hail_inches,
                },
                location,
                presence: BodyPresenceFilter {
                    has_vtec: parse_optional_bool(query.has_vtec.as_deref()),
                    has_ugc: parse_optional_bool(query.has_ugc.as_deref()),
                    has_latlon: parse_optional_bool(query.has_latlon.as_deref()),
                    has_time_mot_loc: parse_optional_bool(query.has_time_mot_loc.as_deref()),
                },
            },
        })
    }

    pub(crate) fn matches(&self, event: &EventKind) -> bool {
        if let Some(event_names) = &self.event_names {
            let event_name = normalize_lower(event.event_name());
            if !event_names.contains(&event_name) {
                return false;
            }
        }

        if !self.file.has_constraints() {
            return true;
        }

        match event {
            EventKind::FileComplete(file) => self.file.matches_metadata(&file.metadata),
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct FileEventFilter {
    pub(crate) filename_pattern: Option<String>,
    pub(crate) size: SizeRange,
    pub(crate) product: ProductFilter,
    pub(crate) header: HeaderFilter,
    pub(crate) issues: IssueFilter,
    pub(crate) geo: GeoFilter,
    pub(crate) vtec: VtecFilter,
    pub(crate) hvtec: HvtecFilter,
    pub(crate) wind_hail: WindHailFilter,
    pub(crate) location: LocationFilter,
    pub(crate) presence: BodyPresenceFilter,
}

impl Eq for FileEventFilter {}

impl FileEventFilter {
    fn has_constraints(&self) -> bool {
        self.filename_pattern.is_some()
            || self.size.has_constraints()
            || self.product.has_constraints()
            || self.header.has_constraints()
            || self.issues.has_constraints()
            || self.geo.has_constraints()
            || self.vtec.has_constraints()
            || self.hvtec.has_constraints()
            || self.wind_hail.has_constraints()
            || self.location.has_constraints()
            || self.presence.has_constraints()
    }

    fn matches_metadata(&self, metadata: &CompletedFileMetadata) -> bool {
        if let Some(pattern) = self.filename_pattern.as_deref()
            && !wildcard_match(pattern, &metadata.filename)
        {
            return false;
        }

        if !self.size.matches(metadata.size) {
            return false;
        }

        if !self.product.matches(&metadata.product) {
            return false;
        }

        if !self.header.matches(&metadata.product) {
            return false;
        }

        if !self.issues.matches(&metadata.product.issues) {
            return false;
        }

        if !self.location.matches(metadata.product.body.as_ref()) {
            return false;
        }

        if !self.presence.matches(metadata.product.body.as_ref()) {
            return false;
        }

        if !self.geo.matches(metadata.product.body.as_ref()) {
            return false;
        }

        if !self.vtec.matches(metadata.product.body.as_ref()) {
            return false;
        }

        if !self.hvtec.matches(metadata.product.body.as_ref()) {
            return false;
        }

        self.wind_hail.matches(metadata.product.body.as_ref())
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct LocationFilter {
    pub(crate) center: Option<GeoPoint>,
    pub(crate) distance_miles: Option<f64>,
}

impl Eq for LocationFilter {}

impl LocationFilter {
    const DEFAULT_DISTANCE_MILES: f64 = 5.0;

    fn try_from_query(query: &EventsQuery) -> Result<Self, EventFilterQueryError> {
        let lat = query.lat;
        let lon = query.lon;

        if lat.is_some() != lon.is_some() {
            return Err(EventFilterQueryError::new(
                "lat and lon must be provided together",
            ));
        }

        let center = match (lat, lon) {
            (Some(lat), Some(lon)) => {
                if !lat.is_finite() || !(-90.0..=90.0).contains(&lat) {
                    return Err(EventFilterQueryError::new(
                        "lat must be a finite value between -90 and 90",
                    ));
                }
                if !lon.is_finite() || !(-180.0..=180.0).contains(&lon) {
                    return Err(EventFilterQueryError::new(
                        "lon must be a finite value between -180 and 180",
                    ));
                }
                Some(GeoPoint { lat, lon })
            }
            _ => None,
        };

        let distance_miles = match query.distance_miles {
            Some(distance_miles) => {
                if center.is_none() {
                    return Err(EventFilterQueryError::new(
                        "distance_miles requires both lat and lon",
                    ));
                }
                if !distance_miles.is_finite() || distance_miles <= 0.0 {
                    return Err(EventFilterQueryError::new(
                        "distance_miles must be a finite value greater than 0",
                    ));
                }
                Some(distance_miles)
            }
            None if center.is_some() => Some(Self::DEFAULT_DISTANCE_MILES),
            None => None,
        };

        Ok(Self {
            center,
            distance_miles,
        })
    }

    fn has_constraints(&self) -> bool {
        self.center.is_some()
    }

    fn matches(&self, body: Option<&ProductBody>) -> bool {
        let Some(center) = self.center else {
            return true;
        };
        let Some(body) = body else {
            return false;
        };

        if body.iter_polygons().any(|polygon| {
            polygon
                .bounds
                .is_some_and(|bounds| bounds_contains(bounds, center))
                && point_in_polygon(center, polygon.points)
        }) {
            return true;
        }

        let Some(distance_miles) = self.distance_miles else {
            return false;
        };

        body.iter_location_points()
            .any(|point| emwin_parser::distance_miles(center, point) <= distance_miles)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EventFilterQueryError {
    pub(crate) message: String,
}

impl EventFilterQueryError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SizeRange {
    pub(crate) min: Option<usize>,
    pub(crate) max: Option<usize>,
}

impl SizeRange {
    fn has_constraints(&self) -> bool {
        self.min.is_some() || self.max.is_some()
    }

    fn matches(&self, size: usize) -> bool {
        if let Some(min) = self.min
            && size < min
        {
            return false;
        }

        if let Some(max) = self.max
            && size > max
        {
            return false;
        }

        true
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ProductFilter {
    pub(crate) source: Option<BTreeSet<String>>,
    pub(crate) pil: Option<BTreeSet<String>>,
    pub(crate) family: Option<BTreeSet<String>>,
    pub(crate) container: Option<BTreeSet<String>>,
    pub(crate) wmo_prefix: Option<BTreeSet<String>>,
    pub(crate) office: Option<BTreeSet<String>>,
    pub(crate) office_city: Option<BTreeSet<String>>,
    pub(crate) office_state: Option<BTreeSet<String>>,
    pub(crate) bbb_kind: Option<BTreeSet<String>>,
}

impl ProductFilter {
    fn has_constraints(&self) -> bool {
        self.source.is_some()
            || self.pil.is_some()
            || self.family.is_some()
            || self.container.is_some()
            || self.wmo_prefix.is_some()
            || self.office.is_some()
            || self.office_city.is_some()
            || self.office_state.is_some()
            || self.bbb_kind.is_some()
    }

    fn matches(&self, product: &ProductEnrichment) -> bool {
        matches_serialized_option(&self.source, Some(product.source), product_source_name)
            && matches_option_set(&self.pil, product.pil.as_deref(), normalize_upper)
            && matches_option_set(&self.family, product.family, normalize_lower)
            && matches_option_set(&self.container, Some(product.container), normalize_lower)
            && matches_option_set(&self.wmo_prefix, product.wmo_prefix, normalize_upper)
            && matches_option_set(
                &self.office,
                product.office.as_ref().map(|office| office.code),
                normalize_upper,
            )
            && matches_option_set(
                &self.office_city,
                product.office.as_ref().map(|office| office.city),
                normalize_lower,
            )
            && matches_option_set(
                &self.office_state,
                product.office.as_ref().map(|office| office.state),
                normalize_upper,
            )
            && matches_serialized_option(&self.bbb_kind, product.bbb_kind, bbb_kind_name)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct HeaderFilter {
    pub(crate) cccc: Option<BTreeSet<String>>,
    pub(crate) ttaaii: Option<BTreeSet<String>>,
    pub(crate) afos: Option<BTreeSet<String>>,
    pub(crate) bbb: Option<BTreeSet<String>>,
}

impl HeaderFilter {
    fn has_constraints(&self) -> bool {
        self.cccc.is_some() || self.ttaaii.is_some() || self.afos.is_some() || self.bbb.is_some()
    }

    fn matches(&self, product: &ProductEnrichment) -> bool {
        if !self.has_constraints() {
            return true;
        }

        let header_matches = product.header.as_ref().is_some_and(|header| {
            matches_option_set(&self.cccc, Some(header.cccc.as_str()), normalize_upper)
                && matches_option_set(&self.ttaaii, Some(header.ttaaii.as_str()), normalize_upper)
                && matches_option_set(&self.afos, Some(header.afos.as_str()), normalize_upper)
                && matches_option_set(&self.bbb, header.bbb.as_deref(), normalize_upper)
        });
        let wmo_header_matches = product.wmo_header.as_ref().is_some_and(|header| {
            matches_option_set(&self.cccc, Some(header.cccc.as_str()), normalize_upper)
                && matches_option_set(&self.ttaaii, Some(header.ttaaii.as_str()), normalize_upper)
                && self.afos.is_none()
                && matches_option_set(&self.bbb, header.bbb.as_deref(), normalize_upper)
        });

        header_matches || wmo_header_matches
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct IssueFilter {
    pub(crate) has_issues: Option<bool>,
    pub(crate) kinds: Option<BTreeSet<String>>,
    pub(crate) codes: Option<BTreeSet<String>>,
}

impl IssueFilter {
    fn has_constraints(&self) -> bool {
        self.has_issues.is_some() || self.kinds.is_some() || self.codes.is_some()
    }

    fn matches(&self, issues: &[ProductParseIssue]) -> bool {
        if let Some(has_issues) = self.has_issues
            && has_issues == issues.is_empty()
        {
            return false;
        }

        if let Some(kinds) = &self.kinds
            && !issues
                .iter()
                .any(|issue| kinds.contains(&normalize_lower(issue.kind)))
        {
            return false;
        }

        if let Some(codes) = &self.codes
            && !issues
                .iter()
                .any(|issue| codes.contains(&normalize_lower(issue.code)))
        {
            return false;
        }

        true
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct GeoFilter {
    pub(crate) states: Option<BTreeSet<String>>,
    pub(crate) counties: Option<BTreeSet<String>>,
    pub(crate) zones: Option<BTreeSet<String>>,
    pub(crate) fire_zones: Option<BTreeSet<String>>,
    pub(crate) marine_zones: Option<BTreeSet<String>>,
}

impl GeoFilter {
    fn has_constraints(&self) -> bool {
        self.states.is_some()
            || self.counties.is_some()
            || self.zones.is_some()
            || self.fire_zones.is_some()
            || self.marine_zones.is_some()
    }

    fn matches(&self, body: Option<&ProductBody>) -> bool {
        if !self.has_constraints() {
            return true;
        }

        let Some(body) = body else {
            return false;
        };
        let Some(sections) = body.ugc.as_deref() else {
            return false;
        };

        matches_geo_states(&self.states, sections)
            && matches_enriched_ugc_codes(
                &self.counties,
                sections,
                |section| &section.counties,
                'C',
            )
            && matches_enriched_ugc_codes(&self.zones, sections, |section| &section.zones, 'Z')
            && matches_enriched_ugc_codes(
                &self.fire_zones,
                sections,
                |section| &section.fire_zones,
                'F',
            )
            && matches_enriched_ugc_codes(
                &self.marine_zones,
                sections,
                |section| &section.marine_zones,
                'Z',
            )
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct VtecFilter {
    pub(crate) phenomena: Option<BTreeSet<String>>,
    pub(crate) significance: Option<BTreeSet<String>>,
    pub(crate) action: Option<BTreeSet<String>>,
    pub(crate) office: Option<BTreeSet<String>>,
    pub(crate) etn: Option<BTreeSet<u32>>,
}

impl VtecFilter {
    fn has_constraints(&self) -> bool {
        self.phenomena.is_some()
            || self.significance.is_some()
            || self.action.is_some()
            || self.office.is_some()
            || self.etn.is_some()
    }

    fn matches(&self, body: Option<&ProductBody>) -> bool {
        if !self.has_constraints() {
            return true;
        }

        let Some(body) = body else {
            return false;
        };
        let Some(vtec_codes) = body.vtec.as_deref() else {
            return false;
        };

        vtec_codes.iter().any(|code| self.matches_code(code))
    }

    fn matches_code(&self, code: &VtecCode) -> bool {
        matches_option_set(
            &self.phenomena,
            Some(code.phenomena.as_str()),
            normalize_upper,
        ) && matches_char_set(&self.significance, code.significance)
            && matches_option_set(&self.action, Some(code.action.as_str()), normalize_upper)
            && matches_option_set(&self.office, Some(code.office.as_str()), normalize_upper)
            && matches_number_set(&self.etn, code.etn)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct HvtecFilter {
    pub(crate) present: Option<bool>,
    pub(crate) nwslid: Option<BTreeSet<String>>,
    pub(crate) severity: Option<BTreeSet<String>>,
    pub(crate) cause: Option<BTreeSet<String>>,
    pub(crate) record: Option<BTreeSet<String>>,
}

impl HvtecFilter {
    fn has_constraints(&self) -> bool {
        self.present.is_some()
            || self.nwslid.is_some()
            || self.severity.is_some()
            || self.cause.is_some()
            || self.record.is_some()
    }

    fn matches(&self, body: Option<&ProductBody>) -> bool {
        if !self.has_constraints() {
            return true;
        }

        let codes = body.and_then(|body| body.hvtec.as_deref());
        if let Some(present) = self.present
            && present != codes.is_some_and(|codes| !codes.is_empty())
        {
            return false;
        }

        if self.nwslid.is_none()
            && self.severity.is_none()
            && self.cause.is_none()
            && self.record.is_none()
        {
            return true;
        }

        let Some(codes) = codes else {
            return false;
        };

        codes.iter().any(|code| self.matches_code(code))
    }

    fn matches_code(&self, code: &HvtecCode) -> bool {
        matches_option_set(&self.nwslid, Some(code.nwslid.as_str()), normalize_upper)
            && matches_serialized_option(&self.severity, Some(code.severity), hvtec_severity_name)
            && matches_serialized_option(&self.cause, Some(code.cause), hvtec_cause_name)
            && matches_serialized_option(&self.record, Some(code.record), hvtec_record_name)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct WindHailFilter {
    pub(crate) present: Option<bool>,
    pub(crate) kinds: Option<BTreeSet<String>>,
    pub(crate) min_wind_mph: Option<f64>,
    pub(crate) min_hail_inches: Option<f64>,
}

impl Eq for WindHailFilter {}

impl WindHailFilter {
    fn has_constraints(&self) -> bool {
        self.present.is_some()
            || self.kinds.is_some()
            || self.min_wind_mph.is_some()
            || self.min_hail_inches.is_some()
    }

    fn matches(&self, body: Option<&ProductBody>) -> bool {
        if !self.has_constraints() {
            return true;
        }

        let entries = body.and_then(|body| body.wind_hail.as_deref());
        if let Some(present) = self.present
            && present != entries.is_some_and(|entries| !entries.is_empty())
        {
            return false;
        }

        let Some(entries) = entries else {
            return self.kinds.is_none()
                && self.min_wind_mph.is_none()
                && self.min_hail_inches.is_none();
        };

        if let Some(kinds) = &self.kinds
            && !entries
                .iter()
                .any(|entry| kinds.contains(wind_hail_kind_name(entry.kind)))
        {
            return false;
        }

        if let Some(min_wind_mph) = self.min_wind_mph
            && !entries.iter().any(|entry| {
                is_wind_entry(entry)
                    && entry
                        .numeric_value
                        .zip(entry.units.as_deref())
                        .is_some_and(|(value, units)| wind_speed_mph(value, units) >= min_wind_mph)
            })
        {
            return false;
        }

        if let Some(min_hail_inches) = self.min_hail_inches
            && !entries.iter().any(|entry| {
                is_hail_entry(entry)
                    && entry
                        .numeric_value
                        .is_some_and(|value| value >= min_hail_inches)
            })
        {
            return false;
        }

        true
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct BodyPresenceFilter {
    pub(crate) has_vtec: Option<bool>,
    pub(crate) has_ugc: Option<bool>,
    pub(crate) has_latlon: Option<bool>,
    pub(crate) has_time_mot_loc: Option<bool>,
}

impl BodyPresenceFilter {
    fn has_constraints(&self) -> bool {
        self.has_vtec.is_some()
            || self.has_ugc.is_some()
            || self.has_latlon.is_some()
            || self.has_time_mot_loc.is_some()
    }

    fn matches(&self, body: Option<&ProductBody>) -> bool {
        matches_optional_presence(self.has_vtec, body.and_then(|body| body.vtec.as_deref()))
            && matches_optional_presence(self.has_ugc, body.and_then(|body| body.ugc.as_deref()))
            && matches_optional_presence(
                self.has_latlon,
                body.and_then(|body| body.latlon.as_deref()),
            )
            && matches_optional_presence(
                self.has_time_mot_loc,
                body.and_then(|body| body.time_mot_loc.as_deref()),
            )
    }
}

fn matches_option_set(
    allowed: &Option<BTreeSet<String>>,
    value: Option<&str>,
    normalize: fn(&str) -> String,
) -> bool {
    match allowed {
        Some(allowed) => value
            .map(normalize)
            .map(|normalized| allowed.contains(&normalized))
            .unwrap_or(false),
        None => true,
    }
}

fn matches_char_set(allowed: &Option<BTreeSet<String>>, value: char) -> bool {
    match allowed {
        Some(allowed) => allowed.contains(&value.to_ascii_uppercase().to_string()),
        None => true,
    }
}

fn matches_number_set(allowed: &Option<BTreeSet<u32>>, value: u32) -> bool {
    match allowed {
        Some(allowed) => allowed.contains(&value),
        None => true,
    }
}

fn matches_serialized_option<T: Copy>(
    allowed: &Option<BTreeSet<String>>,
    value: Option<T>,
    serialize: fn(T) -> &'static str,
) -> bool {
    match allowed {
        Some(allowed) => value
            .map(serialize)
            .map(|serialized| allowed.contains(serialized))
            .unwrap_or(false),
        None => true,
    }
}

fn matches_optional_presence<T>(expected: Option<bool>, values: Option<&[T]>) -> bool {
    match expected {
        Some(expected) => expected == values.is_some_and(|values| !values.is_empty()),
        None => true,
    }
}

fn matches_geo_states(allowed: &Option<BTreeSet<String>>, sections: &[UgcSection]) -> bool {
    match allowed {
        Some(allowed) => sections.iter().any(|section| {
            section.counties.keys().any(|state| allowed.contains(state))
                || section.zones.keys().any(|state| allowed.contains(state))
                || section
                    .fire_zones
                    .keys()
                    .any(|state| allowed.contains(state))
                || section
                    .marine_zones
                    .keys()
                    .any(|state| allowed.contains(state))
        }),
        None => true,
    }
}

fn matches_enriched_ugc_codes(
    allowed: &Option<BTreeSet<String>>,
    sections: &[UgcSection],
    select: fn(&UgcSection) -> &std::collections::BTreeMap<String, Vec<emwin_parser::UgcArea>>,
    class_code: char,
) -> bool {
    match allowed {
        Some(allowed) => sections.iter().any(|section| {
            select(section).iter().any(|(state, areas)| {
                areas
                    .iter()
                    .any(|area| allowed.contains(&format!("{state}{class_code}{:03}", area.id)))
            })
        }),
        None => true,
    }
}

fn csv_values(raw: Option<&str>, normalize: fn(&str) -> String) -> Option<BTreeSet<String>> {
    let values = raw
        .into_iter()
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(normalize)
        .collect::<BTreeSet<_>>();

    (!values.is_empty()).then_some(values)
}

fn csv_numbers(raw: Option<&str>) -> Option<BTreeSet<u32>> {
    let values = raw
        .into_iter()
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter_map(|value| value.parse::<u32>().ok())
        .collect::<BTreeSet<_>>();

    (!values.is_empty()).then_some(values)
}

fn parse_optional_bool(raw: Option<&str>) -> Option<bool> {
    match raw.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) if value.eq_ignore_ascii_case("true") || value == "1" => Some(true),
        Some(value) if value.eq_ignore_ascii_case("false") || value == "0" => Some(false),
        _ => None,
    }
}

fn product_source_name(value: ProductEnrichmentSource) -> &'static str {
    match value {
        ProductEnrichmentSource::TextHeader => "text_header",
        ProductEnrichmentSource::WmoFdBulletin => "wmo_fd_bulletin",
        ProductEnrichmentSource::TextPirepBulletin => "text_pirep_bulletin",
        ProductEnrichmentSource::TextSigmetBulletin => "text_sigmet_bulletin",
        ProductEnrichmentSource::WmoMetarBulletin => "wmo_metar_bulletin",
        ProductEnrichmentSource::WmoTafBulletin => "wmo_taf_bulletin",
        ProductEnrichmentSource::WmoDcpBulletin => "wmo_dcp_bulletin",
        ProductEnrichmentSource::FilenameNonText => "filename_non_text",
        ProductEnrichmentSource::Unknown => "unknown",
    }
}

fn bbb_kind_name(value: BbbKind) -> &'static str {
    match value {
        BbbKind::Amendment => "amendment",
        BbbKind::Correction => "correction",
        BbbKind::DelayedRepeat => "delayed_repeat",
        BbbKind::Other => "other",
    }
}

fn hvtec_severity_name(value: HvtecSeverity) -> &'static str {
    match value {
        HvtecSeverity::None => "none",
        HvtecSeverity::Minor => "minor",
        HvtecSeverity::Moderate => "moderate",
        HvtecSeverity::Major => "major",
        HvtecSeverity::Unknown => "unknown",
    }
}

fn hvtec_cause_name(value: HvtecCause) -> &'static str {
    match value {
        HvtecCause::ExcessiveRainfall => "excessive_rainfall",
        HvtecCause::Snowmelt => "snowmelt",
        HvtecCause::RainAndSnowmelt => "rain_and_snowmelt",
        HvtecCause::DamFailure => "dam_failure",
        HvtecCause::GlacierOutburst => "glacier_outburst",
        HvtecCause::IceJam => "ice_jam",
        HvtecCause::RainSnowmeltIceJam => "rain_snowmelt_ice_jam",
        HvtecCause::UpstreamFloodingStormSurge => "upstream_flooding_storm_surge",
        HvtecCause::UpstreamFloodingTidalEffects => "upstream_flooding_tidal_effects",
        HvtecCause::ElevatedUpstreamFlowTidalEffects => "elevated_upstream_flow_tidal_effects",
        HvtecCause::WindTidalEffects => "wind_tidal_effects",
        HvtecCause::UpstreamDamRelease => "upstream_dam_release",
        HvtecCause::MultipleCauses => "multiple_causes",
        HvtecCause::OtherEffects => "other_effects",
        HvtecCause::Unknown => "unknown",
        HvtecCause::Other => "other",
    }
}

fn hvtec_record_name(value: HvtecRecord) -> &'static str {
    match value {
        HvtecRecord::NoRecord => "no_record",
        HvtecRecord::NearRecord => "near_record",
        HvtecRecord::NotApplicable => "not_applicable",
        HvtecRecord::Unavailable => "unavailable",
        HvtecRecord::Unknown => "unknown",
    }
}

fn wind_hail_kind_name(value: WindHailKind) -> &'static str {
    match value {
        WindHailKind::LegacyWind => "legacy_wind",
        WindHailKind::LegacyHail => "legacy_hail",
        WindHailKind::WindThreat => "wind_threat",
        WindHailKind::MaxWindGust => "max_wind_gust",
        WindHailKind::HailThreat => "hail_threat",
        WindHailKind::MaxHailSize => "max_hail_size",
    }
}

fn is_wind_entry(entry: &WindHailEntry) -> bool {
    matches!(
        entry.kind,
        WindHailKind::LegacyWind | WindHailKind::MaxWindGust
    )
}

fn is_hail_entry(entry: &WindHailEntry) -> bool {
    matches!(
        entry.kind,
        WindHailKind::LegacyHail | WindHailKind::MaxHailSize
    )
}

fn wind_speed_mph(value: f64, units: &str) -> f64 {
    match normalize_upper(units).as_str() {
        "KTS" | "KT" => value * 1.150_78,
        _ => value,
    }
}

fn normalize_upper(value: &str) -> String {
    value.trim().to_ascii_uppercase()
}

fn normalize_lower(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

#[derive(Debug)]
pub(crate) struct AppState {
    pub(crate) event_tx: broadcast::Sender<BroadcastEvent>,
    pub(crate) shutdown_rx: watch::Receiver<bool>,
    pub(crate) retained_files: Mutex<RetainedFiles>,
    pub(crate) telemetry: Mutex<TelemetryPayload>,
    pub(crate) connected_clients: AtomicUsize,
    pub(crate) max_clients: usize,
    pub(crate) next_event_id: AtomicU64,
    pub(crate) data_blocks_total: AtomicU64,
    pub(crate) received_servers: AtomicUsize,
    pub(crate) received_sat_servers: AtomicUsize,
    pub(crate) started_at: Instant,
    pub(crate) upstream_endpoint: Mutex<Option<String>>,
    pub(crate) quiet: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct EventsQuery {
    pub(crate) event: Option<String>,
    pub(crate) filename: Option<String>,
    pub(crate) source: Option<String>,
    pub(crate) pil: Option<String>,
    pub(crate) family: Option<String>,
    pub(crate) container: Option<String>,
    pub(crate) wmo_prefix: Option<String>,
    pub(crate) office: Option<String>,
    pub(crate) office_city: Option<String>,
    pub(crate) office_state: Option<String>,
    pub(crate) bbb_kind: Option<String>,
    pub(crate) cccc: Option<String>,
    pub(crate) ttaaii: Option<String>,
    pub(crate) afos: Option<String>,
    pub(crate) bbb: Option<String>,
    pub(crate) has_issues: Option<String>,
    pub(crate) issue_kind: Option<String>,
    pub(crate) issue_code: Option<String>,
    pub(crate) has_vtec: Option<String>,
    pub(crate) has_ugc: Option<String>,
    pub(crate) has_hvtec: Option<String>,
    pub(crate) has_latlon: Option<String>,
    pub(crate) has_time_mot_loc: Option<String>,
    pub(crate) has_wind_hail: Option<String>,
    pub(crate) state: Option<String>,
    pub(crate) county: Option<String>,
    pub(crate) zone: Option<String>,
    pub(crate) fire_zone: Option<String>,
    pub(crate) marine_zone: Option<String>,
    pub(crate) vtec_phenomena: Option<String>,
    pub(crate) vtec_significance: Option<String>,
    pub(crate) vtec_action: Option<String>,
    pub(crate) vtec_office: Option<String>,
    pub(crate) etn: Option<String>,
    pub(crate) hvtec_nwslid: Option<String>,
    pub(crate) hvtec_severity: Option<String>,
    pub(crate) hvtec_cause: Option<String>,
    pub(crate) hvtec_record: Option<String>,
    pub(crate) wind_hail_kind: Option<String>,
    pub(crate) lat: Option<f64>,
    pub(crate) lon: Option<f64>,
    pub(crate) distance_miles: Option<f64>,
    pub(crate) min_wind_mph: Option<f64>,
    pub(crate) min_hail_inches: Option<f64>,
    pub(crate) min_size: Option<usize>,
    pub(crate) max_size: Option<usize>,
}

#[derive(Debug, Serialize)]
pub(crate) struct FilesResponse {
    pub(crate) files: Vec<CompletedFilePayload>,
}

#[derive(Debug, Serialize)]
pub(crate) struct HealthResponse {
    pub(crate) status: &'static str,
    pub(crate) connected_clients: usize,
    pub(crate) retained_files: usize,
    pub(crate) uptime_secs: u64,
    pub(crate) upstream_endpoint: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct EndpointDoc {
    pub(crate) method: &'static str,
    pub(crate) path: &'static str,
    pub(crate) description: &'static str,
}

#[derive(Debug, Serialize)]
pub(crate) struct RootResponse {
    pub(crate) service: &'static str,
    pub(crate) endpoints: Vec<EndpointDoc>,
}

#[derive(Debug, Clone)]
pub struct ServerOptions {
    pub receiver: crate::ReceiverKind,
    pub username: String,
    pub password: Option<String>,
    pub raw_servers: Vec<String>,
    pub server_list_path: Option<String>,
    pub bind: String,
    pub cors_origin: Option<String>,
    pub max_clients: usize,
    pub stats_interval_secs: u64,
    pub file_retention_secs: u64,
    pub max_retained_files: usize,
    pub quiet: bool,
}

pub(crate) struct ClientGuard {
    pub(crate) state: Arc<AppState>,
    pub(crate) peer: SocketAddr,
}

impl Drop for ClientGuard {
    fn drop(&mut self) {
        self.state.connected_clients.fetch_sub(1, Ordering::Relaxed);
        super::log_info(
            self.state.quiet,
            &format!("sse client disconnected peer={}", self.peer),
        );
    }
}

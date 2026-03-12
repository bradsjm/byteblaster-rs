//! Filter completed-file events for stream and server consumers.
//!
//! These filters operate on already-enriched parser output, which keeps query evaluation out of
//! the hot path that assembles files from incoming segments.

use crate::live::file_pipeline::CompletedFileMetadata;
use crate::live::server_support::wildcard_match;
use emwin_parser::{
    BbbKind, GeoPoint, HvtecCause, HvtecCode, HvtecRecord, HvtecSeverity, ProductBody,
    ProductEnrichment, ProductEnrichmentSource, ProductParseIssue, UgcSection, VtecCode,
    WindHailEntry, WindHailKind, bounds_contains, point_in_polygon,
};
use std::collections::{BTreeMap, BTreeSet};

/// Raw filter parameters collected from CLI flags or HTTP query strings.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct FileFilterInput {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileFilterInputError {
    pub(crate) message: String,
}

impl FileFilterInputError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
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
    pub(crate) fn try_from_input(input: &FileFilterInput) -> Result<Self, FileFilterInputError> {
        let location = LocationFilter::try_from_input(input)?;

        if input
            .min_size
            .zip(input.max_size)
            .is_some_and(|(min, max)| min > max)
        {
            return Err(FileFilterInputError::new(
                "min_size must be less than or equal to max_size",
            ));
        }

        Ok(Self {
            filename_pattern: input.filename.clone(),
            size: SizeRange {
                min: input.min_size,
                max: input.max_size,
            },
            product: ProductFilter {
                source: csv_values(input.source.as_deref(), normalize_lower),
                pil: csv_values(input.pil.as_deref(), normalize_upper),
                family: csv_values(input.family.as_deref(), normalize_lower),
                container: csv_values(input.container.as_deref(), normalize_lower),
                wmo_prefix: csv_values(input.wmo_prefix.as_deref(), normalize_upper),
                office: csv_values(input.office.as_deref(), normalize_upper),
                office_city: csv_values(input.office_city.as_deref(), normalize_lower),
                office_state: csv_values(input.office_state.as_deref(), normalize_upper),
                bbb_kind: csv_values(input.bbb_kind.as_deref(), normalize_lower),
            },
            header: HeaderFilter {
                cccc: csv_values(input.cccc.as_deref(), normalize_upper),
                ttaaii: csv_values(input.ttaaii.as_deref(), normalize_upper),
                afos: csv_values(input.afos.as_deref(), normalize_upper),
                bbb: csv_values(input.bbb.as_deref(), normalize_upper),
            },
            issues: IssueFilter {
                has_issues: parse_optional_bool(input.has_issues.as_deref()),
                kinds: csv_values(input.issue_kind.as_deref(), normalize_lower),
                codes: csv_values(input.issue_code.as_deref(), normalize_lower),
            },
            geo: GeoFilter {
                states: csv_values(input.state.as_deref(), normalize_upper),
                counties: csv_values(input.county.as_deref(), normalize_upper),
                zones: csv_values(input.zone.as_deref(), normalize_upper),
                fire_zones: csv_values(input.fire_zone.as_deref(), normalize_upper),
                marine_zones: csv_values(input.marine_zone.as_deref(), normalize_upper),
            },
            vtec: VtecFilter {
                phenomena: csv_values(input.vtec_phenomena.as_deref(), normalize_upper),
                significance: csv_values(input.vtec_significance.as_deref(), normalize_upper),
                action: csv_values(input.vtec_action.as_deref(), normalize_upper),
                office: csv_values(input.vtec_office.as_deref(), normalize_upper),
                etn: csv_numbers(input.etn.as_deref()),
            },
            hvtec: HvtecFilter {
                present: parse_optional_bool(input.has_hvtec.as_deref()),
                nwslid: csv_values(input.hvtec_nwslid.as_deref(), normalize_upper),
                severity: csv_values(input.hvtec_severity.as_deref(), normalize_lower),
                cause: csv_values(input.hvtec_cause.as_deref(), normalize_lower),
                record: csv_values(input.hvtec_record.as_deref(), normalize_lower),
            },
            wind_hail: WindHailFilter {
                present: parse_optional_bool(input.has_wind_hail.as_deref()),
                kinds: csv_values(input.wind_hail_kind.as_deref(), normalize_lower),
                min_wind_mph: input.min_wind_mph,
                min_hail_inches: input.min_hail_inches,
            },
            location,
            presence: BodyPresenceFilter {
                has_vtec: parse_optional_bool(input.has_vtec.as_deref()),
                has_ugc: parse_optional_bool(input.has_ugc.as_deref()),
                has_hvtec: parse_optional_bool(input.has_hvtec.as_deref()),
                has_latlon: parse_optional_bool(input.has_latlon.as_deref()),
                has_time_mot_loc: parse_optional_bool(input.has_time_mot_loc.as_deref()),
                has_wind_hail: parse_optional_bool(input.has_wind_hail.as_deref()),
            },
        })
    }

    pub(crate) fn from_cli_filters(
        raw_filters: &[String],
    ) -> crate::error::CliResult<Option<Self>> {
        if raw_filters.is_empty() {
            return Ok(None);
        }

        let mut input = FileFilterInput::default();
        for raw in raw_filters {
            let (key, value) = raw.split_once('=').ok_or_else(|| {
                crate::error::CliError::invalid_argument(format!(
                    "invalid --filter entry {raw:?}; expected key=value"
                ))
            })?;
            let key = key.trim();
            let value = value.trim();
            if key.is_empty() || value.is_empty() {
                return Err(crate::error::CliError::invalid_argument(format!(
                    "invalid --filter entry {raw:?}; expected non-empty key=value"
                )));
            }
            if key.eq_ignore_ascii_case("event") {
                return Err(crate::error::CliError::invalid_argument(
                    "--filter event=... is only supported by server /events",
                ));
            }
            input.insert_cli_value(key, value).map_err(|message| {
                crate::error::CliError::invalid_argument(format!(
                    "invalid --filter entry {raw:?}: {message}"
                ))
            })?;
        }

        Self::try_from_input(&input)
            .map(Some)
            .map_err(|err| crate::error::CliError::invalid_argument(err.message))
    }

    pub(crate) fn has_constraints(&self) -> bool {
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

    pub(crate) fn matches_metadata(&self, metadata: &CompletedFileMetadata) -> bool {
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

impl FileFilterInput {
    fn insert_cli_value(&mut self, key: &str, value: &str) -> Result<(), String> {
        match normalize_lower(key).as_str() {
            "filename" => append_string(&mut self.filename, value),
            "source" => append_string(&mut self.source, value),
            "pil" => append_string(&mut self.pil, value),
            "family" => append_string(&mut self.family, value),
            "container" => append_string(&mut self.container, value),
            "wmo_prefix" => append_string(&mut self.wmo_prefix, value),
            "office" => append_string(&mut self.office, value),
            "office_city" => append_string(&mut self.office_city, value),
            "office_state" => append_string(&mut self.office_state, value),
            "bbb_kind" => append_string(&mut self.bbb_kind, value),
            "cccc" => append_string(&mut self.cccc, value),
            "ttaaii" => append_string(&mut self.ttaaii, value),
            "afos" => append_string(&mut self.afos, value),
            "bbb" => append_string(&mut self.bbb, value),
            "has_issues" => append_string(&mut self.has_issues, value),
            "issue_kind" => append_string(&mut self.issue_kind, value),
            "issue_code" => append_string(&mut self.issue_code, value),
            "has_vtec" => append_string(&mut self.has_vtec, value),
            "has_ugc" => append_string(&mut self.has_ugc, value),
            "has_hvtec" => append_string(&mut self.has_hvtec, value),
            "has_latlon" => append_string(&mut self.has_latlon, value),
            "has_time_mot_loc" => append_string(&mut self.has_time_mot_loc, value),
            "has_wind_hail" => append_string(&mut self.has_wind_hail, value),
            "state" => append_string(&mut self.state, value),
            "county" => append_string(&mut self.county, value),
            "zone" => append_string(&mut self.zone, value),
            "fire_zone" => append_string(&mut self.fire_zone, value),
            "marine_zone" => append_string(&mut self.marine_zone, value),
            "vtec_phenomena" => append_string(&mut self.vtec_phenomena, value),
            "vtec_significance" => append_string(&mut self.vtec_significance, value),
            "vtec_action" => append_string(&mut self.vtec_action, value),
            "vtec_office" => append_string(&mut self.vtec_office, value),
            "etn" => append_string(&mut self.etn, value),
            "hvtec_nwslid" => append_string(&mut self.hvtec_nwslid, value),
            "hvtec_severity" => append_string(&mut self.hvtec_severity, value),
            "hvtec_cause" => append_string(&mut self.hvtec_cause, value),
            "hvtec_record" => append_string(&mut self.hvtec_record, value),
            "wind_hail_kind" => append_string(&mut self.wind_hail_kind, value),
            "lat" => self.lat = Some(parse_scalar(key, value)?),
            "lon" => self.lon = Some(parse_scalar(key, value)?),
            "distance_miles" => self.distance_miles = Some(parse_scalar(key, value)?),
            "min_wind_mph" => self.min_wind_mph = Some(parse_scalar(key, value)?),
            "min_hail_inches" => self.min_hail_inches = Some(parse_scalar(key, value)?),
            "min_size" => self.min_size = Some(parse_scalar(key, value)?),
            "max_size" => self.max_size = Some(parse_scalar(key, value)?),
            other => return Err(format!("unknown filter key {other}")),
        }

        Ok(())
    }
}

fn append_string(slot: &mut Option<String>, value: &str) {
    match slot {
        Some(existing) => {
            existing.push(',');
            existing.push_str(value);
        }
        None => *slot = Some(value.to_string()),
    }
}

fn parse_scalar<T>(key: &str, value: &str) -> Result<T, String>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    value
        .parse::<T>()
        .map_err(|err| format!("{key} must be a valid value: {err}"))
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
        let sections = body_ugc_sections(body);
        if sections.is_empty() {
            return false;
        }

        matches_geo_states(&self.states, &sections)
            && matches_enriched_ugc_codes(
                &self.counties,
                &sections,
                |section| &section.counties,
                'C',
            )
            && matches_enriched_ugc_codes(&self.zones, &sections, |section| &section.zones, 'Z')
            && matches_enriched_ugc_codes(
                &self.fire_zones,
                &sections,
                |section| &section.fire_zones,
                'F',
            )
            && matches_enriched_ugc_codes(
                &self.marine_zones,
                &sections,
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
        let vtec_codes = body_vtec_codes(body);
        if vtec_codes.is_empty() {
            return false;
        }

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

        let codes = body.map(body_hvtec_codes).unwrap_or_default();
        if let Some(present) = self.present
            && present == codes.is_empty()
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

        if codes.is_empty() {
            return false;
        }

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

        let entries = body.map(body_wind_hail_entries).unwrap_or_default();
        if let Some(present) = self.present
            && present == entries.is_empty()
        {
            return false;
        }

        if entries.is_empty() {
            return self.kinds.is_none()
                && self.min_wind_mph.is_none()
                && self.min_hail_inches.is_none();
        }

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

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct LocationFilter {
    pub(crate) center: Option<GeoPoint>,
    pub(crate) distance_miles: Option<f64>,
}

impl Eq for LocationFilter {}

impl LocationFilter {
    const DEFAULT_DISTANCE_MILES: f64 = 5.0;

    fn try_from_input(input: &FileFilterInput) -> Result<Self, FileFilterInputError> {
        let lat = input.lat;
        let lon = input.lon;

        if lat.is_some() != lon.is_some() {
            return Err(FileFilterInputError::new(
                "lat and lon must be provided together",
            ));
        }

        let center = match (lat, lon) {
            (Some(lat), Some(lon)) => {
                if !lat.is_finite() || !(-90.0..=90.0).contains(&lat) {
                    return Err(FileFilterInputError::new(
                        "lat must be a finite value between -90 and 90",
                    ));
                }
                if !lon.is_finite() || !(-180.0..=180.0).contains(&lon) {
                    return Err(FileFilterInputError::new(
                        "lon must be a finite value between -180 and 180",
                    ));
                }
                Some(GeoPoint { lat, lon })
            }
            _ => None,
        };

        let distance_miles = match input.distance_miles {
            Some(distance_miles) => {
                if center.is_none() {
                    return Err(FileFilterInputError::new(
                        "distance_miles requires both lat and lon",
                    ));
                }
                if !distance_miles.is_finite() || distance_miles <= 0.0 {
                    return Err(FileFilterInputError::new(
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct BodyPresenceFilter {
    pub(crate) has_vtec: Option<bool>,
    pub(crate) has_ugc: Option<bool>,
    pub(crate) has_hvtec: Option<bool>,
    pub(crate) has_latlon: Option<bool>,
    pub(crate) has_time_mot_loc: Option<bool>,
    pub(crate) has_wind_hail: Option<bool>,
}

impl BodyPresenceFilter {
    fn has_constraints(&self) -> bool {
        self.has_vtec.is_some()
            || self.has_ugc.is_some()
            || self.has_hvtec.is_some()
            || self.has_latlon.is_some()
            || self.has_time_mot_loc.is_some()
            || self.has_wind_hail.is_some()
    }

    fn matches(&self, body: Option<&ProductBody>) -> bool {
        matches_optional_presence(self.has_vtec, body.map_or(0, body_vtec_codes_len))
            && matches_optional_presence(self.has_ugc, body.map_or(0, body_ugc_sections_len))
            && matches_optional_presence(self.has_hvtec, body.map_or(0, body_hvtec_codes_len))
            && matches_optional_presence(self.has_latlon, body.map_or(0, body_latlon_len))
            && matches_optional_presence(
                self.has_time_mot_loc,
                body.map_or(0, body_time_mot_loc_len),
            )
            && matches_optional_presence(self.has_wind_hail, body.map_or(0, body_wind_hail_len))
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

fn matches_optional_presence(expected: Option<bool>, value_count: usize) -> bool {
    match expected {
        Some(expected) => expected == (value_count > 0),
        None => true,
    }
}

fn matches_geo_states(allowed: &Option<BTreeSet<String>>, sections: &[&UgcSection]) -> bool {
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
    sections: &[&UgcSection],
    select: fn(&UgcSection) -> &BTreeMap<String, Vec<emwin_parser::UgcArea>>,
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

fn body_ugc_sections(body: &ProductBody) -> Vec<&UgcSection> {
    match body {
        ProductBody::VtecEvent(body) => body
            .segments
            .iter()
            .flat_map(|segment| segment.ugc_sections.iter())
            .collect(),
        ProductBody::Generic(body) => body
            .ugc
            .iter()
            .flat_map(|sections| sections.iter())
            .collect(),
    }
}

fn body_vtec_codes(body: &ProductBody) -> Vec<&VtecCode> {
    match body {
        ProductBody::VtecEvent(body) => body
            .segments
            .iter()
            .flat_map(|segment| segment.vtec.iter())
            .collect(),
        ProductBody::Generic(_) => Vec::new(),
    }
}

fn body_hvtec_codes(body: &ProductBody) -> Vec<&HvtecCode> {
    match body {
        ProductBody::VtecEvent(body) => body
            .segments
            .iter()
            .flat_map(|segment| segment.hvtec.iter())
            .collect(),
        ProductBody::Generic(_) => Vec::new(),
    }
}

fn body_wind_hail_entries(body: &ProductBody) -> Vec<&WindHailEntry> {
    match body {
        ProductBody::VtecEvent(body) => body
            .segments
            .iter()
            .flat_map(|segment| segment.wind_hail.iter())
            .collect(),
        ProductBody::Generic(body) => body
            .wind_hail
            .iter()
            .flat_map(|entries| entries.iter())
            .collect(),
    }
}

fn body_vtec_codes_len(body: &ProductBody) -> usize {
    match body {
        ProductBody::VtecEvent(body) => {
            body.segments.iter().map(|segment| segment.vtec.len()).sum()
        }
        ProductBody::Generic(_) => 0,
    }
}

fn body_ugc_sections_len(body: &ProductBody) -> usize {
    match body {
        ProductBody::VtecEvent(body) => body
            .segments
            .iter()
            .map(|segment| segment.ugc_sections.len())
            .sum(),
        ProductBody::Generic(body) => body.ugc.as_ref().map_or(0, Vec::len),
    }
}

fn body_hvtec_codes_len(body: &ProductBody) -> usize {
    match body {
        ProductBody::VtecEvent(body) => body
            .segments
            .iter()
            .map(|segment| segment.hvtec.len())
            .sum(),
        ProductBody::Generic(_) => 0,
    }
}

fn body_latlon_len(body: &ProductBody) -> usize {
    match body {
        ProductBody::VtecEvent(body) => body
            .segments
            .iter()
            .map(|segment| segment.polygons.len())
            .sum(),
        ProductBody::Generic(body) => body.latlon.as_ref().map_or(0, Vec::len),
    }
}

fn body_time_mot_loc_len(body: &ProductBody) -> usize {
    match body {
        ProductBody::VtecEvent(body) => body
            .segments
            .iter()
            .map(|segment| segment.time_mot_loc.len())
            .sum(),
        ProductBody::Generic(body) => body.time_mot_loc.as_ref().map_or(0, Vec::len),
    }
}

fn body_wind_hail_len(body: &ProductBody) -> usize {
    match body {
        ProductBody::VtecEvent(body) => body
            .segments
            .iter()
            .map(|segment| segment.wind_hail.len())
            .sum(),
        ProductBody::Generic(body) => body.wind_hail.as_ref().map_or(0, Vec::len),
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
        ProductEnrichmentSource::TextLsrBulletin => "text_lsr_bulletin",
        ProductEnrichmentSource::TextCwaBulletin => "text_cwa_bulletin",
        ProductEnrichmentSource::TextWwpBulletin => "text_wwp_bulletin",
        ProductEnrichmentSource::TextSawBulletin => "text_saw_bulletin",
        ProductEnrichmentSource::TextSelBulletin => "text_sel_bulletin",
        ProductEnrichmentSource::TextCf6Bulletin => "text_cf6_bulletin",
        ProductEnrichmentSource::TextDsmBulletin => "text_dsm_bulletin",
        ProductEnrichmentSource::TextHmlBulletin => "text_hml_bulletin",
        ProductEnrichmentSource::TextMosBulletin => "text_mos_bulletin",
        ProductEnrichmentSource::TextMcdBulletin => "text_mcd_bulletin",
        ProductEnrichmentSource::TextEroBulletin => "text_ero_bulletin",
        ProductEnrichmentSource::TextSpcOutlookBulletin => "text_spc_outlook_bulletin",
        ProductEnrichmentSource::WmoSigmetBulletin => "wmo_sigmet_bulletin",
        ProductEnrichmentSource::WmoMetarBulletin => "wmo_metar_bulletin",
        ProductEnrichmentSource::WmoTafBulletin => "wmo_taf_bulletin",
        ProductEnrichmentSource::WmoDcpBulletin => "wmo_dcp_bulletin",
        ProductEnrichmentSource::WmoUnsupportedBulletin => "wmo_unsupported_bulletin",
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

#[cfg(test)]
mod tests {
    use super::{FileEventFilter, FileFilterInput};
    use crate::live::file_pipeline::build_completed_file_metadata;

    #[test]
    fn cli_filter_matches_issue_code() {
        let filter = FileEventFilter::from_cli_filters(&[
            "has_issues=true".to_string(),
            "issue_code=invalid_wmo_header".to_string(),
        ])
        .expect("filter should parse")
        .expect("filter should exist");
        let metadata = build_completed_file_metadata(
            "AFDBOX.TXT",
            1704070800,
            b"000 \nINVALID HEADER\nAFDBOX\nBody\n",
        );

        assert!(filter.matches_metadata(&metadata));
    }

    #[test]
    fn cli_filter_rejects_unknown_key() {
        let err = FileEventFilter::from_cli_filters(&["bogus=value".to_string()])
            .expect_err("unknown key should fail");

        assert!(err.to_string().contains("unknown filter key"));
    }

    #[test]
    fn cli_filter_rejects_event_key() {
        let err = FileEventFilter::from_cli_filters(&["event=file_complete".to_string()])
            .expect_err("event key should fail");

        assert!(err.to_string().contains("only supported by server /events"));
    }

    #[test]
    fn input_validation_rejects_distance_without_coords() {
        let err = FileEventFilter::try_from_input(&FileFilterInput {
            distance_miles: Some(5.0),
            ..FileFilterInput::default()
        })
        .expect_err("distance without coords should fail");

        assert_eq!(err.message, "distance_miles requires both lat and lon");
    }
}

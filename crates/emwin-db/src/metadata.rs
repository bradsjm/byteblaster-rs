use chrono::{DateTime, Utc};
use emwin_parser::{
    ProductBody, ProductDetailV2, ProductEnrichment, ProductSummaryV2, VtecCode, detail_product_v2,
    enrich_product, summarize_product_v2,
};
use emwin_protocol::ingest::ProductOrigin;
use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IncidentStatus {
    Active,
    Cancelled,
    Expired,
    Upgraded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IncidentMetadata {
    pub office: String,
    pub phenomena: String,
    pub significance: String,
    pub etn: u32,
    pub latest_vtec_action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_status: Option<IncidentStatus>,
    pub issued_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_utc: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_utc: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct IncidentKey {
    office: String,
    phenomena: String,
    significance: String,
    etn: u32,
}

/// Shared completed-file metadata used by the CLI and persistence sinks.
///
/// This keeps runtime access to the enriched product while replacing persisted metadata with a
/// focused incident projection derived from operational VTEC updates.
#[derive(Debug, Clone, PartialEq)]
pub struct CompletedFileMetadata {
    pub filename: String,
    pub size: usize,
    pub timestamp_utc: u64,
    pub origin: ProductOrigin,
    pub product: ProductEnrichment,
    pub incidents: Vec<IncidentMetadata>,
}

impl CompletedFileMetadata {
    /// Builds the canonical metadata bundle from one delivered product payload.
    pub fn build(filename: &str, timestamp_utc: u64, origin: ProductOrigin, data: &[u8]) -> Self {
        let product = enrich_product(filename, data);
        Self::from_product(filename, data.len(), timestamp_utc, origin, product)
    }

    /// Builds metadata from an already enriched product.
    pub fn from_product(
        filename: &str,
        size: usize,
        timestamp_utc: u64,
        origin: ProductOrigin,
        product: ProductEnrichment,
    ) -> Self {
        Self {
            filename: filename.to_string(),
            size,
            timestamp_utc,
            origin,
            incidents: collect_incidents(timestamp_utc, &product),
            product,
        }
    }

    pub fn product_summary(&self) -> ProductSummaryV2 {
        summarize_product_v2(&self.product)
    }

    pub fn product_detail(&self) -> ProductDetailV2 {
        detail_product_v2(&self.product)
    }
}

impl Serialize for CompletedFileMetadata {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("CompletedFileMetadata", 4)?;
        state.serialize_field("filename", &self.filename)?;
        state.serialize_field("size", &self.size)?;
        state.serialize_field("timestamp_utc", &self.timestamp_utc)?;
        state.serialize_field("incidents", &self.incidents)?;
        state.end()
    }
}

pub(crate) fn incident_status_for_action(action: &str) -> Option<IncidentStatus> {
    match action {
        "NEW" | "CON" | "EXT" | "EXA" | "EXB" => Some(IncidentStatus::Active),
        "CAN" => Some(IncidentStatus::Cancelled),
        "EXP" => Some(IncidentStatus::Expired),
        "UPG" => Some(IncidentStatus::Upgraded),
        "COR" | "ROU" => None,
        _ => Some(IncidentStatus::Active),
    }
}

fn collect_incidents(timestamp_utc: u64, product: &ProductEnrichment) -> Vec<IncidentMetadata> {
    let issued_at = DateTime::<Utc>::from_timestamp(timestamp_utc as i64, 0)
        .unwrap_or(DateTime::<Utc>::UNIX_EPOCH);
    let Some(body) = product.body.as_ref() else {
        return Vec::new();
    };

    let mut incidents = BTreeMap::<IncidentKey, IncidentMetadata>::new();
    for code in body_vtec_codes(body) {
        if code.status != 'O' {
            continue;
        }

        let key = IncidentKey {
            office: code.office.clone(),
            phenomena: code.phenomena.clone(),
            significance: code.significance.to_string(),
            etn: code.etn,
        };
        let entry = incidents
            .entry(key.clone())
            .or_insert_with(|| IncidentMetadata {
                office: key.office.clone(),
                phenomena: key.phenomena.clone(),
                significance: key.significance.clone(),
                etn: key.etn,
                latest_vtec_action: code.action.clone(),
                current_status: incident_status_for_action(&code.action),
                issued_at,
                start_utc: code.begin,
                end_utc: code.end,
            });

        entry.latest_vtec_action = code.action.clone();
        entry.current_status = incident_status_for_action(&code.action);
        entry.start_utc = min_datetime(entry.start_utc, code.begin);
        entry.end_utc = max_datetime(entry.end_utc, code.end);
    }

    incidents.into_values().collect()
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

fn min_datetime(
    current: Option<DateTime<Utc>>,
    candidate: Option<DateTime<Utc>>,
) -> Option<DateTime<Utc>> {
    match (current, candidate) {
        (Some(current), Some(candidate)) => Some(current.min(candidate)),
        (Some(current), None) => Some(current),
        (None, Some(candidate)) => Some(candidate),
        (None, None) => None,
    }
}

fn max_datetime(
    current: Option<DateTime<Utc>>,
    candidate: Option<DateTime<Utc>>,
) -> Option<DateTime<Utc>> {
    match (current, candidate) {
        (Some(current), Some(candidate)) => Some(current.max(candidate)),
        (Some(current), None) => Some(current),
        (None, Some(candidate)) => Some(candidate),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{CompletedFileMetadata, IncidentStatus};
    use chrono::DateTime;
    use emwin_protocol::ingest::ProductOrigin;

    #[test]
    fn serialization_replaces_sidecar_with_incident_metadata() {
        let metadata = CompletedFileMetadata::build(
            "AFDBOX.TXT",
            1,
            ProductOrigin::Qbt,
            b"000 \nFXUS61 KBOX 022101\nAFDBOX\nBody\n",
        );

        let value = serde_json::to_value(&metadata).expect("metadata should serialize");
        assert_eq!(value["filename"], "AFDBOX.TXT");
        assert_eq!(value["size"], metadata.size);
        assert_eq!(value["timestamp_utc"], 1);
        assert!(value["incidents"].is_array());
        assert!(value.get("origin").is_none());
        assert!(value.get("product").is_none());
    }

    #[test]
    fn build_collects_operational_vtec_incidents() {
        let metadata = CompletedFileMetadata::build(
            "FFWOAXNE.TXT",
            1_740_960_000,
            ProductOrigin::Qbt,
            br#"000
WUUS53 KOAX 051200
FFWOAX

/O.NEW.KOAX.FF.W.0001.250305T1200Z-250305T1800Z/
/O.CON.KOAX.FF.W.0001.250305T1215Z-250305T1900Z/
"#,
        );

        assert_eq!(metadata.incidents.len(), 1);
        let incident = &metadata.incidents[0];
        assert_eq!(incident.office, "KOAX");
        assert_eq!(incident.phenomena, "FF");
        assert_eq!(incident.significance, "W");
        assert_eq!(incident.etn, 1);
        assert_eq!(incident.latest_vtec_action, "CON");
        assert_eq!(incident.current_status, Some(IncidentStatus::Active));
        assert_eq!(
            incident.start_utc,
            DateTime::from_timestamp(1_741_176_000, 0)
        );
        assert_eq!(incident.end_utc, DateTime::from_timestamp(1_741_201_200, 0));
    }
}

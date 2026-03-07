//! Product metadata lookup, hydrologic location lookup, and non-text filename classification.

mod generated_nwslid;
mod generated_pil;
mod generated_ugc;
mod generated_wmo_office;
mod graphics;

use std::sync::OnceLock;

pub use generated_nwslid::{NWSLID_ENTRY_COUNT, NWSLID_GENERATED_AT_UTC};
pub use generated_pil::{PIL_ENTRY_COUNT, PIL_GENERATED_AT_UTC};
pub use generated_ugc::{
    UGC_COUNTY_ENTRY_COUNT, UGC_COUNTY_SOURCE_PATH, UGC_GENERATED_AT_UTC, UGC_ZONE_ENTRY_COUNT,
    UGC_ZONE_SOURCE_PATH,
};
pub use generated_wmo_office::{
    WMO_OFFICE_ENTRY_COUNT, WMO_OFFICE_GENERATED_AT_UTC, WMO_OFFICE_SOURCE_PATH,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PilCatalogEntry {
    pub pil: &'static str,
    pub wmo_prefix: &'static str,
    pub title: &'static str,
    pub ugc: bool,
    pub vtec: bool,
    pub cz: bool,
    pub latlong: bool,
    pub time_mot_loc: bool,
    pub wind_hail: bool,
    pub hvtec: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct NwslidEntry {
    pub nwslid: &'static str,
    pub state_code: &'static str,
    pub stream_name: &'static str,
    pub proximity: &'static str,
    pub place_name: &'static str,
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct UgcLocationEntry {
    pub code: &'static str,
    pub name: &'static str,
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct WmoOfficeEntry {
    pub code: &'static str,
    pub office_name: &'static str,
    pub city: &'static str,
    pub state: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct ProductMetadataFlags {
    pub ugc: bool,
    pub vtec: bool,
    pub cz: bool,
    pub latlong: bool,
    pub time_mot_loc: bool,
    pub wind_hail: bool,
    pub hvtec: bool,
}

impl From<&PilCatalogEntry> for ProductMetadataFlags {
    fn from(entry: &PilCatalogEntry) -> Self {
        Self {
            ugc: entry.ugc,
            vtec: entry.vtec,
            cz: entry.cz,
            latlong: entry.latlong,
            time_mot_loc: entry.time_mot_loc,
            wind_hail: entry.wind_hail,
            hvtec: entry.hvtec,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NonTextProductMeta {
    pub family: &'static str,
    pub title: &'static str,
    pub container: &'static str,
    pub pil: Option<&'static str>,
    pub wmo_prefix: Option<&'static str>,
}

pub fn pil_description(nnn: &str) -> Option<&'static str> {
    pil_catalog_entry(nnn).map(|entry| entry.title)
}

pub fn nwslid_entry(code: &str) -> Option<&'static NwslidEntry> {
    let key = normalize_nwslid(code)?;
    generated_nwslid::NWSLID_CATALOG
        .binary_search_by_key(&key.as_str(), |entry| entry.nwslid)
        .ok()
        .map(|index| &generated_nwslid::NWSLID_CATALOG[index])
}

pub fn ugc_county_entry(code: &str) -> Option<&'static UgcLocationEntry> {
    let key = normalize_ugc(code, 'C')?;
    generated_ugc::UGC_COUNTY_CATALOG
        .binary_search_by_key(&key.as_str(), |entry| entry.code)
        .ok()
        .map(|index| &generated_ugc::UGC_COUNTY_CATALOG[index])
}

pub fn ugc_zone_entry(code: &str) -> Option<&'static UgcLocationEntry> {
    let key = normalize_ugc(code, 'Z')?;
    generated_ugc::UGC_ZONE_CATALOG
        .binary_search_by_key(&key.as_str(), |entry| entry.code)
        .ok()
        .map(|index| &generated_ugc::UGC_ZONE_CATALOG[index])
}

pub fn wmo_office_entry(code: &str) -> Option<&'static WmoOfficeEntry> {
    let key = normalize_wmo_office(code)?;
    generated_wmo_office::WMO_OFFICE_CATALOG
        .binary_search_by_key(&key.as_str(), |entry| entry.code)
        .ok()
        .map(|index| &generated_wmo_office::WMO_OFFICE_CATALOG[index])
}

pub fn pil_catalog_entry(nnn: &str) -> Option<&'static PilCatalogEntry> {
    let key = normalize_pil(nnn)?;
    generated_pil::PIL_CATALOG
        .binary_search_by_key(&key.as_str(), |entry| entry.pil)
        .ok()
        .map(|index| &generated_pil::PIL_CATALOG[index])
}

pub fn wmo_prefix_for_pil(nnn: &str) -> Option<&'static str> {
    pil_catalog_entry(nnn).map(|entry| entry.wmo_prefix)
}

pub(crate) fn classify_non_text_product(filename: &str) -> Option<NonTextProductMeta> {
    let canon = canonical_name(filename);
    let upper = canon.to_ascii_uppercase();

    graphics::detect_graphics(&upper)
}

fn normalize_pil(nnn: &str) -> Option<String> {
    let key = nnn.trim().to_ascii_uppercase();
    if key.len() == 3 { Some(key) } else { None }
}

fn normalize_nwslid(code: &str) -> Option<String> {
    let key = code.trim().to_ascii_uppercase();
    if key.len() == 5 && key.chars().all(|ch| ch.is_ascii_alphanumeric()) {
        Some(key)
    } else {
        None
    }
}

fn normalize_ugc(code: &str, expected_class: char) -> Option<String> {
    let key = code.trim().to_ascii_uppercase();
    let bytes = key.as_bytes();
    if key.len() == 6
        && bytes[0].is_ascii_uppercase()
        && bytes[1].is_ascii_uppercase()
        && bytes[2] == expected_class as u8
        && bytes[3..].iter().all(u8::is_ascii_digit)
    {
        Some(key)
    } else {
        None
    }
}

fn normalize_wmo_office(code: &str) -> Option<String> {
    let key = code.trim().to_ascii_uppercase();
    match key.len() {
        3 if key.chars().all(|ch| ch.is_ascii_alphanumeric()) => Some(key),
        4 if key.chars().all(|ch| ch.is_ascii_alphanumeric()) => Some(key[1..].to_string()),
        _ => None,
    }
}

pub(crate) fn canonical_name(filename: &str) -> String {
    let base = filename
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(filename)
        .trim();
    if let Some((_, tail)) = base.rsplit_once('-') {
        let candidate = tail.trim();
        if candidate.contains('.')
            && !candidate.is_empty()
            && candidate
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        {
            return candidate.to_string();
        }
    }
    base.to_string()
}

pub(crate) fn container_from_ext(ext: &str) -> &'static str {
    if ext == "ZIP" { "zip" } else { "raw" }
}

pub(crate) fn container_from_filename(filename: &str) -> &'static str {
    let canon = canonical_name(filename);
    let ext = canon
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_uppercase())
        .unwrap_or_default();
    container_from_ext(&ext)
}

pub(super) fn radar_re() -> &'static regex::Regex {
    static RADAR_RE: OnceLock<regex::Regex> = OnceLock::new();
    RADAR_RE.get_or_init(|| regex::Regex::new(r"^(RAD[A-Z0-9]{5})\.(GIF)$").expect("valid regex"))
}

pub(super) fn goes_re() -> &'static regex::Regex {
    static GOES_RE: OnceLock<regex::Regex> = OnceLock::new();
    GOES_RE.get_or_init(|| {
        regex::Regex::new(r"^(G\d{2}[A-Z0-9]{6})\.(ZIP|JPG)$").expect("valid regex")
    })
}

pub(super) fn imgmod_re() -> &'static regex::Regex {
    static IMGMOD_RE: OnceLock<regex::Regex> = OnceLock::new();
    IMGMOD_RE.get_or_init(|| {
        regex::Regex::new(r"^((?:IMG|MOD)[A-Z0-9]{5})\.(ZIP|GIF|PNG|JPG)$").expect("valid regex")
    })
}

#[cfg(test)]
mod tests {
    use super::{
        classify_non_text_product, nwslid_entry, pil_catalog_entry, pil_description,
        ugc_county_entry, ugc_zone_entry, wmo_office_entry, wmo_prefix_for_pil,
    };

    #[test]
    fn description_lookup_is_case_insensitive() {
        assert_eq!(pil_description("afd"), Some("Area Forecast Discussion"));
        assert_eq!(wmo_prefix_for_pil("AFD"), Some("FX"));
    }

    #[test]
    fn lookup_rejects_invalid_pil_length() {
        assert_eq!(pil_catalog_entry("TO"), None);
        assert_eq!(pil_description("TOOO"), None);
    }

    #[test]
    fn detects_radar_graphic_family() {
        let meta = classify_non_text_product("RADUMSVY.GIF").expect("expected metadata");
        assert_eq!(meta.family, "radar_graphic");
        assert_eq!(meta.title, "Radar graphic");
        assert_eq!(meta.container, "raw");
    }

    #[test]
    fn detects_prefixed_filename() {
        let meta = classify_non_text_product("20260305-RADUMSVY.GIF").expect("expected metadata");
        assert_eq!(meta.family, "radar_graphic");
    }

    #[test]
    fn generated_catalog_contains_known_entry() {
        let entry = pil_catalog_entry("ZFP").expect("expected generated catalog entry");
        assert_eq!(entry.title, "Zone Forecast Product");
        assert_eq!(entry.wmo_prefix, "FP");
        assert!(entry.ugc);
        assert!(!entry.vtec);
        assert!(!entry.hvtec);
    }

    #[test]
    fn generated_catalog_exposes_product_flags() {
        let entry = pil_catalog_entry("FFW").expect("expected generated catalog entry");
        assert!(entry.ugc);
        assert!(entry.vtec);
        assert!(!entry.cz);
        assert!(entry.latlong);
        assert!(entry.time_mot_loc);
        assert!(!entry.wind_hail);
        assert!(entry.hvtec);
    }

    #[test]
    fn nwslid_lookup_is_case_insensitive() {
        let entry = nwslid_entry("chfa2").expect("expected generated nwslid entry");
        assert_eq!(entry.nwslid, "CHFA2");
        assert_eq!(entry.state_code, "AK");
        assert_eq!(entry.stream_name, "Chena River");
        assert_eq!(entry.proximity, "at");
        assert_eq!(entry.place_name, "Fairbanks");
        assert_eq!(entry.latitude, 64.8458);
        assert_eq!(entry.longitude, -147.7011);
    }

    #[test]
    fn nwslid_lookup_rejects_invalid_codes() {
        assert!(nwslid_entry("TOO-LONG").is_none());
        assert!(nwslid_entry("AB!12").is_none());
        assert!(nwslid_entry("ZZZZZ").is_none());
    }

    #[test]
    fn ugc_county_lookup_is_case_insensitive() {
        let entry = ugc_county_entry("alc001").expect("expected generated county entry");
        assert_eq!(entry.code, "ALC001");
        assert_eq!(entry.name, "Autauga");
        assert_eq!(entry.latitude, 32.5349);
        assert_eq!(entry.longitude, -86.6428);
    }

    #[test]
    fn ugc_zone_lookup_is_case_insensitive() {
        let entry = ugc_zone_entry("akz317").expect("expected generated zone entry");
        assert_eq!(entry.code, "AKZ317");
        assert_eq!(entry.name, "City and Borough of Yakutat");
        assert_eq!(entry.latitude, 59.8909);
        assert_eq!(entry.longitude, -140.3727);
    }

    #[test]
    fn ugc_lookup_rejects_wrong_class() {
        assert!(ugc_county_entry("AKZ317").is_none());
        assert!(ugc_zone_entry("ALC001").is_none());
        assert!(ugc_zone_entry("BAD").is_none());
    }

    #[test]
    fn wmo_office_lookup_supports_three_and_four_letter_codes() {
        let entry = wmo_office_entry("LWX").expect("expected generated office entry");
        assert_eq!(entry.code, "LWX");
        assert_eq!(entry.office_name, "WFO Baltimore/Washington");
        assert_eq!(entry.city, "Baltimore/Washington");
        assert_eq!(entry.state, "DC");

        let entry = wmo_office_entry("KLWX").expect("expected generated office entry");
        assert_eq!(entry.code, "LWX");
    }

    #[test]
    fn wmo_office_lookup_rejects_invalid_codes() {
        assert!(wmo_office_entry("XX").is_none());
        assert!(wmo_office_entry("ABCDE").is_none());
    }
}

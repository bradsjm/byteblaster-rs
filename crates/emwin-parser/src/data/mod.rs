//! Product metadata lookup, hydrologic location lookup, and non-text filename classification.

mod generated_nwslid;
mod generated_pil;
mod graphics;

use std::sync::OnceLock;

pub use generated_nwslid::{NWSLID_ENTRY_COUNT, NWSLID_GENERATED_AT_UTC};
pub use generated_pil::{
    PIL_ENTRY_COUNT, PIL_GENERATED_AT_UTC, PIL_SOURCE_COMMIT, PIL_SOURCE_PATH, PIL_SOURCE_REPO,
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
        wmo_prefix_for_pil,
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
}

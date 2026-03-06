use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

const PRODUCT_CATALOG_CSV: &str = include_str!("../../data/product_catalog.csv");
const PIL_ORIGIN_SAFE_LIST: &str = include_str!("../../data/pil_origin_safe.txt");

#[derive(Debug, Clone)]
struct PilCatalogEntry {
    wmo_prefix: String,
    title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProductMeta {
    pub source: &'static str,
    pub family: &'static str,
    pub title: String,
    pub code: String,
    pub container: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pil: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wmo_prefix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
}

pub fn detect_product_meta(filename: &str) -> Option<ProductMeta> {
    let canon = canonical_name(filename);
    let upper = canon.to_ascii_uppercase();

    detect_graphics(&upper).or_else(|| detect_awips_text(&upper))
}

fn detect_graphics(filename_upper: &str) -> Option<ProductMeta> {
    static RADAR_RE: OnceLock<Regex> = OnceLock::new();
    static GOES_RE: OnceLock<Regex> = OnceLock::new();
    static IMGMOD_RE: OnceLock<Regex> = OnceLock::new();

    let radar_re =
        RADAR_RE.get_or_init(|| Regex::new(r"^(RAD[A-Z0-9]{5})\.(GIF)$").expect("valid regex"));
    if let Some(caps) = radar_re.captures(filename_upper) {
        let code = caps.get(1).expect("code group exists").as_str().to_string();
        return Some(ProductMeta {
            source: "regex_graphics",
            family: "radar_graphic",
            title: "Radar graphic".to_string(),
            code,
            container: "raw",
            pil: None,
            wmo_prefix: None,
            origin: None,
            region: None,
        });
    }

    let goes_re = GOES_RE
        .get_or_init(|| Regex::new(r"^(G\d{2}[A-Z0-9]{6})\.(ZIP|JPG)$").expect("valid regex"));
    if let Some(caps) = goes_re.captures(filename_upper) {
        let code = caps.get(1).expect("code group exists").as_str().to_string();
        let ext = caps.get(2).expect("ext group exists").as_str();
        return Some(ProductMeta {
            source: "regex_graphics",
            family: "goes_graphic",
            title: "GOES satellite graphic".to_string(),
            code,
            container: container_from_ext(ext),
            pil: None,
            wmo_prefix: None,
            origin: None,
            region: None,
        });
    }

    let imgmod_re = IMGMOD_RE.get_or_init(|| {
        Regex::new(r"^((?:IMG|MOD)[A-Z0-9]{5})\.(ZIP|GIF|PNG|JPG)$").expect("valid regex")
    });
    if let Some(caps) = imgmod_re.captures(filename_upper) {
        let code = caps.get(1).expect("code group exists").as_str().to_string();
        let ext = caps.get(2).expect("ext group exists").as_str();
        return Some(ProductMeta {
            source: "regex_graphics",
            family: "nws_graphic",
            title: "NWS graphic product".to_string(),
            code,
            container: container_from_ext(ext),
            pil: None,
            wmo_prefix: None,
            origin: None,
            region: None,
        });
    }

    None
}

fn detect_awips_text(filename_upper: &str) -> Option<ProductMeta> {
    static STEM_RE: OnceLock<Regex> = OnceLock::new();
    static ORIGIN_RE: OnceLock<Regex> = OnceLock::new();

    let stem_re = STEM_RE.get_or_init(|| {
        Regex::new(r"^(?P<stem>[A-Z0-9]{3,})\.(?P<ext>TXT|ZIP)$").expect("valid regex")
    });
    let caps = stem_re.captures(filename_upper)?;
    let stem = caps.name("stem")?.as_str();
    let ext = caps.name("ext")?.as_str();
    let pil = stem.get(0..3)?.to_string();

    let entry = pil_catalog().get(&pil)?;
    let mut origin = None;
    let mut region = None;

    if pil_origin_safe().contains(&pil) {
        let origin_re = ORIGIN_RE.get_or_init(|| {
            Regex::new(r"^[A-Z0-9]{3}(?P<origin>[A-Z0-9]{3})(?P<region>[A-Z]{2})$")
                .expect("valid regex")
        });
        if let Some(caps) = origin_re.captures(stem) {
            origin = caps.name("origin").map(|m| m.as_str().to_string());
            region = caps.name("region").map(|m| m.as_str().to_string());
        }
    }

    Some(ProductMeta {
        source: "regex_awips_table",
        family: "nws_text_product",
        title: entry.title.clone(),
        code: stem.to_string(),
        container: container_from_ext(ext),
        pil: Some(pil),
        wmo_prefix: Some(entry.wmo_prefix.clone()),
        origin,
        region,
    })
}

fn pil_catalog() -> &'static HashMap<String, PilCatalogEntry> {
    static PIL_CATALOG: OnceLock<HashMap<String, PilCatalogEntry>> = OnceLock::new();
    PIL_CATALOG.get_or_init(|| {
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(true)
            .from_reader(PRODUCT_CATALOG_CSV.as_bytes());
        let mut table = HashMap::new();
        for row in reader.records().flatten() {
            if row.len() != 3 {
                continue;
            }
            let pil = row.get(0).unwrap_or_default().trim().to_string();
            let wmo_prefix = row.get(1).unwrap_or_default().trim().to_string();
            let title = row.get(2).unwrap_or_default().trim().to_string();
            if pil.is_empty() || wmo_prefix.is_empty() || title.is_empty() {
                continue;
            }
            table.insert(pil, PilCatalogEntry { wmo_prefix, title });
        }
        table
    })
}

fn pil_origin_safe() -> &'static HashSet<String> {
    static PIL_ORIGIN_SAFE: OnceLock<HashSet<String>> = OnceLock::new();
    PIL_ORIGIN_SAFE.get_or_init(|| {
        PIL_ORIGIN_SAFE_LIST
            .lines()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
            .collect()
    })
}

fn canonical_name(filename: &str) -> String {
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

fn container_from_ext(ext: &str) -> &'static str {
    if ext == "ZIP" { "zip" } else { "raw" }
}

#[cfg(test)]
mod tests {
    use super::detect_product_meta;

    #[test]
    fn detects_awips_text_metadata_shape() {
        let meta = detect_product_meta("TAFPDKGA.TXT").expect("expected metadata");
        assert_eq!(meta.pil.as_deref(), Some("TAF"));
        assert_eq!(meta.origin.as_deref(), Some("PDK"));
        assert_eq!(meta.region.as_deref(), Some("GA"));
        assert!(!meta.title.is_empty());
        assert!(meta.wmo_prefix.as_deref().is_some());
        assert_eq!(meta.container, "raw");
    }

    #[test]
    fn detects_radar_graphic_family() {
        let meta = detect_product_meta("RADUMSVY.GIF").expect("expected metadata");
        assert_eq!(meta.family, "radar_graphic");
        assert_eq!(meta.title, "Radar graphic");
        assert_eq!(meta.container, "raw");
    }

    #[test]
    fn unknown_code_returns_none() {
        assert!(detect_product_meta("COMP1117.ZIP").is_none());
    }
}

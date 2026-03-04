use regex::Regex;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

#[derive(Debug, Clone, Serialize)]
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
    let candidate = canonical_name(filename);
    let upper = candidate.to_ascii_uppercase();

    detect_graphics(&upper).or_else(|| detect_awips_text(&upper))
}

fn canonical_name(filename: &str) -> String {
    let base = filename.rsplit(['/', '\\']).next().unwrap_or(filename);
    let trimmed = base.trim();
    if let Some(last) = trimmed.rsplit('-').next()
        && last.contains('.')
        && last
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
    {
        return last.to_string();
    }
    trimmed.to_string()
}

fn container_from_ext(ext: &str) -> &'static str {
    if ext == "ZIP" { "zip" } else { "raw" }
}

fn detect_graphics(filename_upper: &str) -> Option<ProductMeta> {
    static RADAR_RE: OnceLock<Regex> = OnceLock::new();
    static GOES_RE: OnceLock<Regex> = OnceLock::new();
    static IMG_MOD_RE: OnceLock<Regex> = OnceLock::new();

    let radar_re =
        RADAR_RE.get_or_init(|| Regex::new(r"^(RAD[A-Z0-9]{5})\.(GIF)$").expect("valid regex"));
    if let Some(caps) = radar_re.captures(filename_upper) {
        let code = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
        let ext = caps.get(2).map(|m| m.as_str()).unwrap_or("GIF");
        return Some(ProductMeta {
            source: "regex_graphics",
            family: "radar_graphic",
            title: "Radar graphic".to_string(),
            code: code.to_string(),
            container: container_from_ext(ext),
            pil: None,
            wmo_prefix: None,
            origin: None,
            region: None,
        });
    }

    let goes_re = GOES_RE
        .get_or_init(|| Regex::new(r"^(G\d{2}[A-Z0-9]{6})\.(ZIP|JPG)$").expect("valid regex"));
    if let Some(caps) = goes_re.captures(filename_upper) {
        let code = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
        let ext = caps.get(2).map(|m| m.as_str()).unwrap_or("JPG");
        return Some(ProductMeta {
            source: "regex_graphics",
            family: "goes_graphic",
            title: "GOES satellite graphic".to_string(),
            code: code.to_string(),
            container: container_from_ext(ext),
            pil: None,
            wmo_prefix: None,
            origin: None,
            region: None,
        });
    }

    let img_mod_re = IMG_MOD_RE.get_or_init(|| {
        Regex::new(r"^((?:IMG|MOD)[A-Z0-9]{5})\.(ZIP|GIF|PNG|JPG)$").expect("valid regex")
    });
    if let Some(caps) = img_mod_re.captures(filename_upper) {
        let code = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
        let ext = caps.get(2).map(|m| m.as_str()).unwrap_or("GIF");
        return Some(ProductMeta {
            source: "regex_graphics",
            family: "nws_graphic",
            title: "NWS graphic product".to_string(),
            code: code.to_string(),
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

    let pil = &stem[0..3];
    let entry = pil_catalog().get(pil)?;

    let mut meta = ProductMeta {
        source: "regex_awips_table",
        family: "nws_text_product",
        title: entry.title.to_string(),
        code: stem.to_string(),
        container: container_from_ext(ext),
        pil: Some(pil.to_string()),
        wmo_prefix: Some(entry.wmo_prefix.to_string()),
        origin: None,
        region: None,
    };

    if pil_origin_safe().contains(pil) {
        let origin_re = ORIGIN_RE.get_or_init(|| {
            Regex::new(r"^[A-Z0-9]{3}(?P<origin>[A-Z0-9]{3})(?P<region>[A-Z]{2})$")
                .expect("valid regex")
        });
        if let Some(origin_caps) = origin_re.captures(stem)
            && let (Some(origin), Some(region)) =
                (origin_caps.name("origin"), origin_caps.name("region"))
        {
            meta.origin = Some(origin.as_str().to_string());
            meta.region = Some(region.as_str().to_string());
        }
    }

    Some(meta)
}

fn pil_origin_safe() -> &'static HashSet<&'static str> {
    static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| {
        [
            "AFD", "CWF", "ESP", "FFA", "FFW", "FLW", "FTM", "HWR", "LSR", "MIS", "OMR", "OPU",
            "OSO", "PNS", "PRB", "RFW", "RVS", "RVF", "RWR", "SFT", "SPS", "STQ", "TAF", "TID",
            "WRK", "WWA",
        ]
        .into_iter()
        .collect()
    })
}

#[derive(Debug, Clone, Copy)]
struct PilCatalogEntry {
    wmo_prefix: &'static str,
    title: &'static str,
}

fn pil_catalog() -> &'static HashMap<&'static str, PilCatalogEntry> {
    static MAP: OnceLock<HashMap<&'static str, PilCatalogEntry>> = OnceLock::new();
    MAP.get_or_init(|| {
        PRODUCT_TABLE_ENTRIES
            .iter()
            .map(|(pil, wmo_prefix, title)| (*pil, PilCatalogEntry { wmo_prefix, title }))
            .collect()
    })
}

const PRODUCT_TABLE_ENTRIES: &[(&str, &str, &str)] = &[
    ("18A", "FB", "18 Hour Report"),
    ("24A", "FB", "24 Hour Report"),
    ("30L", "FE", "Monthly Weather Outlook"),
    (
        "3HR",
        "FO",
        "3-hourly Space Weather Conditions and Forecast",
    ),
    ("5TC", "AU", "500 Millibar Type Correlation"),
    ("ABV", "UF", "Rawinsonde Data Above 100 Millibars"),
    ("ADA", "NO", "Alarm/Alert Administrative Message"),
    ("ADM", "NO", "Alert Administrative Message"),
    ("ADR", "NO", "Administrative Message"),
    ("ADV", "NW", "Official Space Weather Advisory"),
    ("ADW", "NO", "Administrative Message for NWWS"),
    ("ADX", "NT", "Administrative Alert Non-Receipt"),
    ("AFD", "FX", "Area Forecast Discussion"),
    ("AFM", "FO", "Area Forecast Matrices"),
    ("AFP", "FL", "Area Forecast Product"),
    ("AGF", "FN", "Agricultural Forecast"),
    ("AGO", "SH", "Agricultural Observations"),
    ("ALM", "NW", "Space Environment Alarm"),
    ("ALT", "WO", "Space Weather Message Alert"),
    ("APG", "AE", "Air Stagnation Guidance Narrative"),
    ("AQA", "AE", "Air Quality Message"),
    ("AQI", "AE", "Air Quality Index Statement"),
    ("ARP", "UA", "AIREPS"),
    ("ASA", "AE", "Air Stagnation Advisory"),
    ("AVA", "WO", "Avalanche Watch"),
    ("AVM", "NO", "Aviation Verification Matrix"),
    ("AVW", "WO", "Avalanche Warning"),
    ("AWG", "WO", "National Attack Warning"),
    ("AWO", "NO", "Area Weather Outlook"),
    ("AWS", "AW", "Area Weather Summary"),
    ("AWU", "FL", "Area Weather Update"),
    ("AWW", "WW", "Aviation Weather Warning"),
    ("BOY", "SS", "Buoy Reports"),
    ("BRT", "AX", "Area Synopsis and Forecast"),
    ("CAE", "WO", "Child Abduction Emergency"),
    ("CAP", "XO", "Alert Messages in CAP XML"),
    ("CCF", "FP", "Coded City Forecast"),
    ("CDW", "WO", "Civil Danger Warning"),
    ("CEM", "WO", "Civil Emergency Message"),
    ("CF6", "CX", "Climate F-6 products"),
    ("CFP", "FA", "Convective Forecast Product"),
    ("CFW", "WH", "Coastal/Lakeshore Hazard Messages"),
    ("CGR", "SX", "Coast Guard Surface Report"),
    ("CHG", "WT", "Computer Hurricane Guidance"),
    ("CLA", "CX", "Climate Report Annual"),
    ("CLI", "CD", "Climate Report Daily"),
    ("CLM", "CX", "Climate Report Monthly"),
    ("CLS", "CX", "Climate Report Seasonal"),
    ("CMM", "CS", "Coded Climatological Monthly Means"),
    ("COD", "FX", "Coded Analysis and Forecasts"),
    ("CSC", "FP", "Canadian Selected Cities Forecast"),
    ("CUR", "FX", "Current Space Weather Indices"),
    ("CWA", "WL", "Center Weather Advisory"),
    ("CWD", "NZ", "Discussion for Coastal Waters Forecast"),
    ("CWF", "FZ", "Coastal Waters Forecast"),
    ("CWS", "WO", "Center Weather Statement"),
    ("DAA", "SD", "170 QPE One Hour Digital Accumulation"),
    ("DAY", "FX", "Daily Space Weather Summary and Forcast"),
    ("DGT", "AX", "Drought Information"),
    ("DHR", "SD", "Digital Hybrid Reflectivity"),
    ("DMO", "WO", "Practice/Demo Warning"),
    ("DOD", "SD", "174 QPE-PPS One Hour Accumulation Difference"),
    ("DPA", "SD", "Radar Hourly Digital Precipitation"),
    ("DPR", "SD", "176 QPE Instanteaneous Precipitation Rate"),
    ("DSA", "AC", "Unnumbered Depression"),
    (
        "DSD",
        "SD",
        "175 QPE-PPS Storm Total Accumulation Difference",
    ),
    ("DSM", "CX", "Asos Daily Summary"),
    ("DSP", "SD", "Adigital Storm Total Precipitation Nexrad"),
    ("DST", "CX", "ASOS Daily Summary"),
    ("DSW", "WW", "Dust Storm Warning"),
    ("DU3", "SD", "173 QPE User Selectable 3 Hour Accumulation"),
    ("DU6", "SD", "173 QPE User Selectable 24 Hour Accumulation"),
    (
        "DVL",
        "SD",
        "134 Radar Digital Vertically Integrated Liquid",
    ),
    ("EAN", "WO", "Presidential Alert"),
    ("EET", "SD", "135 Radar Enhanced Echo Tops"),
    ("EFP", "FE", "3 TO 5 Day Extended Forecast"),
    ("EOL", "FE", "Average 6 TO 10 Day Weather Outlook"),
    ("EON", "FE", "Average 6 TO 10 Day Weather Outlook"),
    ("EQI", "SE", "Earthquake Information Bulletin"),
    ("EQR", "SE", "Earthquake Report"),
    ("EQW", "WO", "Earthquake Warning"),
    ("ESF", "FG", "Flood Potential Outlook"),
    ("ESG", "FG", "Extended Streamflow Guidance"),
    ("ESP", "FG", "Extended Streamflow Prediction"),
    ("ESS", "FG", "Water Supply Outlook"),
    ("EVI", "WO", "Evacuation Immediate"),
    ("EWW", "WF", "Extreme Wind Warning"),
    ("FA0", "FA", "Aviation Area Forecast (Pacific)"),
    ("FA1", "FA", "Aviation Area Forecast (Northeast U.S.)"),
    ("FA2", "FA", "Aviation Area Forecast (Southeast U.S.)"),
    ("FA3", "FA", "Aviation Area Forecast (North Cent U.S.)"),
    ("FA4", "FA", "Aviation Area Forecast (South Cent U.S.)"),
    ("FA5", "FA", "Aviation Area Forecast (U.S. Rocky Mts.)"),
    ("FA6", "FA", "Aviation Area Forecast (U.S. West Coast)"),
    ("FA7", "FA", "Aviation Area Forecast (Juneau)"),
    ("FA8", "FA", "Aviation Area Forecast (Anchorage)"),
    ("FA9", "FA", "Aviation Area Forecast (Fairbanks)"),
    ("FAK", "FA", "Alaskan Model Output Statistics Forecast"),
    ("FAN", "FE", "AVN Based MOS Guidance"),
    ("FAV", "FA", "Aviation Area forecast"),
    ("FBP", "FO", "AK FOUS 24, 36, and 24 Boundary"),
    ("FD0", "FB", "24 Hr FD Winds Aloft Forecast"),
    ("FD1", "FB", "6 Hour Winds Aloft Forecast"),
    ("FD2", "FB", "12 Hour Winds Aloft Forecast"),
    ("FD3", "FB", "24 Hour Winds Aloft Forecast"),
    ("FD8", "FB", "24 Hour Winds Aloft Forecast"),
    ("FD9", "FB", "6 Hour Winds Aloft Forecast"),
    ("FDI", "FN", "Fire Danger Indices"),
    ("FFA", "WG", "Flash Flood Watch"),
    ("FFG", "FO", "Flash Flood Guidance"),
    ("FFH", "FO", "Headwater Guidance"),
    ("FFS", "WG", "Flash Flood Statement"),
    ("FFW", "WG", "Flash Flood Warning"),
    ("FLN", "FG", "National Flood Summary"),
    ("FLS", "WG", "Flood Statement"),
    ("FLW", "WG", "Flood Warning"),
    ("FMR", "FE", "Forecast Medium Range Guidance"),
    ("FOF", "FD", "Upper Wind Fallout Forecast"),
    ("FOH", "FO", "ETA FOUS Freezing and Relative Humidity"),
    ("FRH", "FO", "FOUS Relative Humidity/Temperature Guidance"),
    ("FRW", "WO", "Fire Warning"),
    ("FSH", "NO", "National Marine Fisheries Message"),
    (
        "FSS",
        "FG",
        "Urban and Small Stream Flood Advisory (Obsolete)",
    ),
    ("FTJ", "FO", "FOUS Trajectory Forecast"),
    ("FTM", "NO", "WSR-88D Radar Status Notification"),
    ("FTP", "FO", "FOUS Prog Max/Min Temp/Pop Guidance"),
    ("FWA", "NO", "Fire Weather Administrative Message"),
    ("FWC", "FO", "NGM Mos Guidance"),
    ("FWD", "FN", "Fire Weather Outlook Discussion"),
    ("FWE", "FE", "Extended Fire Weather Outlook"),
    ("FWF", "FN", "Routine Fire WX Forecasts"),
    ("FWL", "FN", "Land Management Forecasts"),
    ("FWM", "FN", "Miscellaneous Fire Weather Product"),
    ("FWN", "SH", "Fire Weather Notification"),
    ("FWO", "SH", "Fire Weather Observation"),
    ("FWS", "FN", "Suppression Forecast"),
    ("FZL", "UX", "Freezing Level Data"),
    ("GLF", "FZ", "Great Lakes Forecast"),
    ("GLO", "FZ", "Great Lakes Storm Outlook"),
    ("GLS", "WW", "Great Lakes Storm Summary"),
    ("GSM", "NX", "General Status Message"),
    ("HCM", "NG", "Hydrometeorlogical Coordination Message"),
    ("HD1", "FP", "RFC Derived QPF Data Product"),
    ("HD2", "FP", "RFC Derived QPF Data Product"),
    ("HD3", "FP", "RFC Derived QPF Data Product"),
    ("HD4", "FP", "RFC Derived QPF Data Product"),
    ("HD5", "FP", "RFC Derived QPF Data Product"),
    ("HD6", "FP", "RFC Derived QPF Data Product"),
    ("HD7", "FP", "RFC Derived QPF Data Product"),
    ("HD8", "FP", "RFC Derived QPF Data Product"),
    ("HD9", "FP", "RFC Derived QPF Data Product"),
    ("HDP", "AG", "WSR-88D Hourly Digital Precipitation"),
    ("HHC", "SD", "177 Hybrid Scan Hydrometeor Classification"),
    ("HLS", "WT", "Hurricane Local Statement"),
    ("HMD", "AG", "Hydrometeorological Discussion"),
    ("HML", "FD", "Hyrdo Obs/Forecasts XML"),
    ("HMW", "WO", "Hazardous Materials Warning"),
    ("HP1", "FP", "RFC QPF Verification Product"),
    ("HP2", "FP", "RFC QPF Verification Product"),
    ("HP3", "FP", "RFC QPF Verification Product"),
    ("HP4", "FP", "RFC QPF Verification Product"),
    ("HP5", "FP", "RFC QPF Verification Product"),
    ("HP6", "FP", "RFC QPF Verification Product"),
    ("HP7", "FP", "RFC QPF Verification Product"),
    ("HP8", "FP", "RFC QPF Verification Product"),
    ("HP9", "FP", "RFC QPF Verification Product"),
    ("HRR", "NZ", "Hourly Weather Roundup"),
    ("HSF", "FZ", "High Seas Forecast"),
    ("HWO", "FL", "Hazardous Weather Outlook"),
    ("HYD", "SX", "Rainfall Reports"),
    (
        "HYM",
        "CS",
        "Monthly Hydrometeorological Plain Language Product",
    ),
    (
        "HYW",
        "CW",
        "Weekly Hydrometeorological Plain Language Product",
    ),
    ("ICE", "FZ", "Ice Forecast"),
    ("ICO", "FZ", "Ice Outlook"),
    ("INI", "NO", "Administration"),
    ("IOB", "SV", "Ice Observation"),
    ("IRM", "SD", "Interim Radar Message"),
    ("KPA", "NT", "Keep Alive Message"),
    ("LAE", "WO", "Local Area Emergency"),
    ("LAW", "SM", "Great Lakes Weather Observation"),
    ("LCO", "SX", "Local Cooperative Observation"),
    ("LEW", "WO", "Law Enforcement Warning"),
    ("LFP", "FL", "Local Forecast"),
    ("LLS", "UX", "Low-Level Sounding"),
    ("LSH", "WH", "Lakeshore Warning or Statement"),
    ("LSR", "NW", "Local Storm Report"),
    ("LTG", "SF", "Lightning Data"),
    ("MAN", "US", "Rawinsonde Observation Mandatory Levels"),
    ("MAP", "AG", "Mean Areal Precipitation"),
    ("MAV", "FO", "GFS Based MOS Guidance"),
    ("MAW", "FO", "MOS Avaiation-based Guidance"),
    ("MEF", "NZ", "AFOS Forecast Verification Matrix"),
    ("MET", "FO", "ETA Based MOS Guidance"),
    ("MEX", "FE", "GFX Extended Based MOS Guidance"),
    ("MIM", "AG", "Marine Interpretation Message"),
    ("MIS", "AX", "Miscellaneous Local Product"),
    ("MON", "NT", "Test Message"),
    ("MRM", "NO", "Missing Radar Message"),
    ("MRP", "AG", "Techniques Development Laboratory Marine"),
    ("MSM", "CS", "ASOS Monthly Summary Message"),
    ("MST", "CS", "ASOS Monthly Summary Message Test"),
    ("MTR", "SA", "METAR Formatted Surface Weather Observation"),
    ("MTT", "SX", "METAR Test Message"),
    ("MWS", "FZ", "Marine Weather Statement"),
    ("MWW", "WH", "Marine Weather Message"),
    (
        "N0C",
        "SD",
        "161 RADAR 0.4-0.8 CORR CFFCNT .13NM RES 256 LEVELS ID 161/DCC DS.161c0",
    ),
    (
        "N0H",
        "SD",
        "165 RADAR 0.4-0.8 HYDROMETEOR CLASS .13NM RES ID 165/DHC DS.165h0",
    ),
    (
        "N0K",
        "SD",
        "163 RADAR 0.4-0.8 SPEC DIFF PHASE .13NM RES 256 LVLS ID 163/DKD DS.163k0",
    ),
    (
        "N0M",
        "SD",
        "166 RADAR 0.4-0.8 MELTING LAYER ID 166/ML DS.166m0",
    ),
    (
        "N0Q",
        "SD",
        "94 RADAR 0.4-0.8 REFLECTIVITY .54NM RES 256 LEVELS ID 94/DR Add No 20K 6-14/hr RPCCDS & SBN DS.p94r0",
    ),
    (
        "N0R",
        "SD",
        "RADAR .5 REFLECTIVITY .54NM RES 16 LEVELS ID 19/R",
    ),
    (
        "N0S",
        "SD",
        "RADAR .5 STORM REL. VELOCITY .54NM RES 16 LVLS ID 56/SRM",
    ),
    (
        "N0U",
        "SD",
        "99 RADAR 0.4-0.8 VELOCITY .13NM RES 256 LEVELS ID 99/DV Add No 55K 6-14/hr RPCCDS & SBN DS.p99v0",
    ),
    ("N0V", "SD", "RADAR .5 VELOCITY .54NM RES 16 LEVELS ID 27/V"),
    ("N0W", "SD", "RADAR BASE RADIAL VELOCITY 25/V"),
    (
        "N0X",
        "SD",
        "159 RADAR 0.4-0.8 DIFF RFLCTVTY .13NM RES 256 LEVELS ID 159/DZD DS.159x0",
    ),
    (
        "N0Z",
        "SD",
        "RADAR .5 REFLECTIVITY 1.1NM RES 16 LEVELS ID 20/R",
    ),
    (
        "N1C",
        "SD",
        "161 RADAR 1.2-1.6 CORR CFFCNT .13NM RES 256 LEVELS ID 161/DCC DS.161c1",
    ),
    (
        "N1H",
        "SD",
        "165 RADAR 1.2-1.6 HYDROMETEOR CLASS .13NM RES ID 165/DHC DS.165h1",
    ),
    (
        "N1K",
        "SD",
        "163 RADAR 1.2-1.6 SPEC DIFF PHASE .13NM RES 256 LVLS ID 163/DKD DS.163k1",
    ),
    (
        "N1M",
        "SD",
        "166 RADAR 1.2-1.6 MELTING LAYER ID 166/ML DS.166m1",
    ),
    (
        "N1P",
        "SD",
        "RADAR 1 HOUR PRECIPITATION ACCUMULATION 78/OHP",
    ),
    (
        "N1Q",
        "SD",
        "94 RADAR 1.2-1.6 REFLECTIVITY .54NM RES 256 LEVELS ID 94/DR Add No 12K 6-14/hr RPCCDS & SBN DS.p94r1",
    ),
    (
        "N1R",
        "SD",
        "RADAR 1.5 REFLECTIVITY .54NM RES 16 LEVELS ID 19/R",
    ),
    (
        "N1S",
        "SD",
        "RADAR 1.5 STORM REL. VELOCITY .54NM RES 16 LVLS ID 56/SRM",
    ),
    (
        "N1U",
        "SD",
        "99 RADAR 1.2-1.6 VELOCITY .13NM RES 256 LEVELS ID 99/DV Add No 30K 6-14/hr RPCCDS & SBN DS.p99v1",
    ),
    (
        "N1V",
        "SD",
        "RADAR 1.5 VELOCITY .54NM RES 16 LEVELS ID 27/V",
    ),
    (
        "N1X",
        "SD",
        "159 RADAR 1.2-1.6 DIFF RFLCTVTY .13NM RES 256 LEVELS ID 159/DZD DS.159x1",
    ),
    (
        "N2C",
        "SD",
        "161 RADAR 2.1-2.6 CORR CFFCNT .13NM RES 256 LEVELS ID 161/DCC DS.161c2",
    ),
    (
        "N2H",
        "SD",
        "165 RADAR 2.1-2.6 HYDROMETEOR CLASS .13NM RES ID 165/DHC DS.165h2",
    ),
    (
        "N2K",
        "SD",
        "163 RADAR 2.1-2.6 SPEC DIFF PHASE .13NM RES 256 LVLS ID 163/DKD DS.163k2",
    ),
    (
        "N2M",
        "SD",
        "166 RADAR 2.1-2.6 MELTING LAYER ID 166/ML DS.166m2",
    ),
    (
        "N2Q",
        "SD",
        "94 RADAR 2.1-2.6 REFLECTIVITY .54NM RES 256 LEVELS ID 94/DR Add No 11K 6-14/hr RPCCDS & SBN DS.p94r2",
    ),
    (
        "N2R",
        "SD",
        "RADAR 2.4/2.5 REFLECTIVITY .54NM RES 16 LEVELS ID 19/R",
    ),
    (
        "N2S",
        "SD",
        "RADAR 2.4/2.5 STORM REL VLCTY .54NM RES 16 LVLS ID 56/SRM",
    ),
    (
        "N2U",
        "SD",
        "99 RADAR 2.1-2.6 VELOCITY .13NM RES 256 LEVELS ID 99/DV Add No 27K 6-14/hr RPCCDS & SBN DS.p99v2",
    ),
    (
        "N2V",
        "SD",
        "RADAR 2.4/2.5 VELOCITY .54NM RES 16 LEVELS ID 27/V",
    ),
    (
        "N2X",
        "SD",
        "159 RADAR 2.1-2.6 DIFF RFLCTVTY .13NM RES 256 LEVELS ID 159/DZD DS.159x2",
    ),
    (
        "N3C",
        "SD",
        "161 RADAR 2.7-3.6 CORR CFFCNT .13NM RES 256 LEVELS ID 161/DCC DS.161c3",
    ),
    (
        "N3H",
        "SD",
        "165 RADAR 2.7-3.6 HYDROMETEOR CLASS .13NM RES ID 165/DHC DS.165h3",
    ),
    (
        "N3K",
        "SD",
        "163 RADAR 2.7-3.6 SPEC DIFF PHASE .13NM RES 256 LVLS ID 163/DKD DS.163k3",
    ),
    (
        "N3M",
        "SD",
        "166 RADAR 2.7-3.6 MELTING LAYER ID 166/ML DS.166m3",
    ),
    (
        "N3P",
        "SD",
        "RADAR 3 HOUR PRECIPITATION ACCUMULATION 79/THP",
    ),
    (
        "N3Q",
        "SD",
        "94 RADAR 2.7-3.6 REFLECTIVITY .54NM RES 256 LEVELS ID 94/DR Add No 10K 6-14/hr RPCCDS & SBN DS.p94r3",
    ),
    (
        "N3R",
        "SD",
        "RADAR 3.4/3.5 REFLECTIVITY .54NM RES 16 LEVELS ID 19/R",
    ),
    (
        "N3S",
        "SD",
        "RADAR 3.4/3.5 STORM REL VLCTY .54NM RES 16 LVLS ID 56/SRM",
    ),
    (
        "N3U",
        "SD",
        "99 RADAR 2.7-3.6 VELOCITY .13NM RES 256 LEVELS ID 99/DV Add No 23K 6-14/hr RPCCDS & SBN DS.p99v3",
    ),
    (
        "N3V",
        "SD",
        "RADAR 3.4/3.5 VELOCITY .54NM RES 16 LEVELS ID 27/V",
    ),
    (
        "N3X",
        "SD",
        "159 RADAR 2.7-3.6 DIFF RFLCTVTY .13NM RES 256 LEVELS ID 159/DZD DS.159x3",
    ),
    (
        "N6P",
        "SD",
        "USER SELECTABLE PRECIPITATION 6 HOUR ACCUMULATION 31/USP",
    ),
    (
        "NAC",
        "SD",
        "161 RADAR 0.9-1.1 CORR CFFCNT .13NM RES 256 LEVELS ID 161/DCC DS.161ca",
    ),
    (
        "NAH",
        "SD",
        "165 RADAR 0.9-1.1 HYDROMETEOR CLASS .13NM RES ID 165/DHC DS.165ha",
    ),
    (
        "NAK",
        "SD",
        "163 RADAR 0.9-1.1 SPEC DIFF PHASE .13NM RES 256 LVLS ID 163/DKD DS.163ka",
    ),
    (
        "NAM",
        "SD",
        "166 RADAR 0.9-1.1 MELTING LAYER ID 166/ML DS.166ma",
    ),
    (
        "NAQ",
        "SD",
        "94 RADAR 0.9-1.1 REFLECTIVITY .54NM RES 256 LEVELS ID 94/DR Add No 16K 6-14/hr RPCCDS & SBN DS.p94ra",
    ),
    (
        "NAU",
        "SD",
        "99 RADAR 0.9-1.1 VELOCITY .13NM RES 256 LEVELS ID 99/DV Add No 47K 6-14/hr RPCCDS & SBN DS.p99va",
    ),
    (
        "NAX",
        "SD",
        "159 RADAR 0.9-1.1 DIFF RFLCTVTY .13NM RES 256 LEVELS ID 159/DZD DS.159xa",
    ),
    (
        "NBC",
        "SD",
        "161 RADAR 1.7-2.0 CORR CFFCNT .13NM RES 256 LEVELS ID 161/DCC DS.161cb",
    ),
    (
        "NBH",
        "SD",
        "165 RADAR 1.7-2.0 HYDROMETEOR CLASS .13NM RES ID 165/DHC DS.165hb",
    ),
    (
        "NBK",
        "SD",
        "163 RADAR 1.7-2.0 SPEC DIFF PHASE .13NM RES 256 LVLS ID 163/DKD DS.163kb",
    ),
    (
        "NBM",
        "SD",
        "166 RADAR 1.7-2.0 MELTING LAYER ID 166/ML DS.166mb",
    ),
    (
        "NBQ",
        "SD",
        "94 RADAR 1.7-2.0 REFLECTIVITY .54NM RES 256 LEVELS ID 94/DR Add No 11K 6-14/hr RPCCDS & SBN DS.p94rb",
    ),
    (
        "NBU",
        "SD",
        "99 RADAR 1.7-2.0 VELOCITY .13NM RES 256 LEVELS ID 99/DV Add No 29K 6-14/hr RPCCDS & SBN DS.p99vb",
    ),
    (
        "NBX",
        "SD",
        "159 RADAR 1.7-2.0 DIFF RFLCTVTY .13NM RES 256 LEVELS ID 159/DZD DS.159xb",
    ),
    ("NC1", "SD", "CLUTTER FILTER CONTROL (CFC) - SEGMENT 1"),
    ("NC2", "SD", "CLUTTER FILTER CONTROL (CFC) - SEGMENT 2"),
    ("NC3", "SD", "CLUTTER FILTER CONTROL (CFC) - SEGMENT 3"),
    ("NC4", "SD", "CLUTTER FILTER CONTROL (CFC) - SEGMENT 4"),
    ("NC5", "SD", "CLUTTER FILTER CONTROL (CFC) - SEGMENT 5"),
    ("NCF", "SD", "CLUTTER FILTER CONTROL"),
    (
        "NCO",
        "SD",
        "RADAR COMPOSITE REFLECTIVITY 2.2NM RES 8 LEVELS ID 36/CR",
    ),
    (
        "NCR",
        "SD",
        "RADAR COMPOSITE REFLECTIVITY .54NM RES 16 LEVELS ID 37/CR",
    ),
    (
        "NCZ",
        "SD",
        "RADAR COMPOSITE REFLECTIVITY 2.2NM RES 16 LEVELS ID 38/CR",
    ),
    ("NET", "SD", "RADAR ECHO TOPS ID 41/ET"),
    ("NHI", "SD", "HAIL INDEX 59/HI"),
    (
        "NHL",
        "SD",
        "RADAR HIGH LAYER COMPOSITE REFLECTIVITY MAX ID 90/LRM",
    ),
    ("NIC", "WO", "National Information Center"),
    (
        "NLA",
        "SD",
        "RADAR LOW LAYER COMPOSITE REFLECTIVITY AP RMVD ID 67/ARP",
    ),
    (
        "NLL",
        "SD",
        "RADAR LOW LAYER COMPOSITE REFLECTIVITY MAX ID 65/LRM",
    ),
    ("NME", "SD", "MESOCYCLONE 60/M"),
    (
        "NML",
        "SD",
        "RADAR MIDDLE LAYER COMPOSITE REFLECTIVITY MAX ID 66/LRM",
    ),
    ("NMN", "WO", "Network Message Notification"),
    ("NOW", "FP", "Short Term Forecast"),
    ("NPT", "WO", "National Periodic Test"),
    ("NPW", "WW", "Non-Precipitation Message"),
    ("NSH", "FZ", "Nearshore Marine Forecast / Surf Forecast"),
    ("NSP", "SD", "Base Spectrum Width 32NM .13NM X 1 Deg"),
    ("NSS", "SD", "Storm Structure"),
    ("NST", "SD", "Storm Tracking Information"),
    ("NSW", "SD", "Base Spectrum Width 124NM .54NM X 1 Deg"),
    ("NTP", "SD", "Radar Storm Total Precipitation Accumulation"),
    ("NTV", "SD", "Tornado Vortex Signature"),
    ("NUP", "SD", "Radar User Select Precipitation Accumulation"),
    ("NUW", "WO", "Nuclear Power Plant Warning"),
    ("NVL", "SD", "Radar Vertically Integrated Liquid"),
    ("NVW", "SD", "Radar Velocity Azimuth Display Wind Profile"),
    ("NWP", "SD", "Severe Weather Probability"),
    ("NWR", "NZ", "NOAA Weather radio Forecast"),
    ("OAV", "NO", "Other Aviation Products"),
    ("OCD", "AG", "Oceanographic Data"),
    ("OEP", "NZ", "Operational Evolution Plan"),
    ("OFA", "FA", "Offshore Aviation Area Forecast"),
    ("OFF", "FZ", "Offshore Forecast"),
    ("OMR", "SX", "Other Marine Products"),
    ("OPU", "FP", "Other Public Products"),
    ("OSB", "AG", "Oceanographic Spectral Bulletin"),
    ("OSO", "SX", "Other Surface Observations"),
    ("OSW", "AG", "Ocean Surface Winds"),
    ("OUA", "UX", "Other Upper Air Data"),
    ("PAR", "NO", "Performance Accomplishment Report"),
    ("PBF", "FN", "Prescribed Burn Forecast"),
    ("PFM", "FO", "Point Forecast Matrices"),
    ("PFW", "FO", "Point Fire Weather Forecast Matrices"),
    ("PIB", "UP", "Pibal Observation"),
    ("PIR", "UA", "Pilot Report"),
    ("PLS", "SX", "Plain Language Ship Report"),
    ("PMD", "FX", "Prognostic Meteorological Discussion"),
    ("PNS", "NO", "Public Information Statement"),
    ("POE", "FX", "Probabiilty of Exceed"),
    ("PRB", "FM", "Days 3--7 Heat Index Forecast Tables"),
    ("PRC", "UA", "State Pilot Report Collective"),
    ("PSH", "AC", "Post Storm Hurricane Report"),
    ("PSM", "FX", "Pronostic Snow Melt"),
    ("PVM", "FX", "Public Verification Matrix"),
    ("PWO", "WW", "Public Severe Weather Alert"),
    ("QCD", "NT", "ASOS Daily Quality Control Message"),
    ("QCH", "NT", "ASOS Hourly Quality Control Message"),
    ("QCM", "NT", "ASOS Monthly Quality Control Message"),
    ("QCW", "NT", "ASOS Weekly Quality Control Message"),
    ("QPF", "FS", "Quantitative Precipitation Forecast"),
    ("QPS", "FS", "Quantitative Precipitation Statement"),
    ("RBG", "PX", "Red Book Graphic"),
    ("RCM", "SD", "WSR-88D Radar Coded Message"),
    ("RDF", "FO", "Revised Digital Forecast"),
    ("RDG", "FO", "Revised Digital Guidance"),
    ("REC", "SX", "Recreational Report"),
    ("RER", "SX", "Record Event Report"),
    ("RFD", "FN", "Rangeland Fire Danger Forecast"),
    ("RFR", "FR", "Route Forecast"),
    ("RFW", "WW", "Red Flag Warning"),
    ("RHW", "WO", "Radiological Hazard Warning"),
    ("RMT", "WO", "Required Monthly Test"),
    ("RR1", "SR", "Hydrology Meteorology Data Report Part 1"),
    ("RR2", "SR", "Hydrology Meteorology Data Report Part 2"),
    ("RR3", "SR", "Hydrology Meteorology Data Report Part 3"),
    ("RR4", "SR", "Hydrology Meteorology Data Report Part 4"),
    ("RR5", "SR", "Hydrology Meteorology Data Report Part 5"),
    ("RR6", "SR", "ASOS Shef Precip Criteria Message"),
    ("RR7", "SR", "ASOS Shef Hourly Routine Message"),
    ("RR8", "SR", "Hydrology Meteorology Data Report Part 8"),
    ("RR9", "SR", "Hydrology Meteorology Data Report Part 9"),
    ("RRA", "SR", "Automated Hydrologic Observation STA Report"),
    ("RRC", "SR", "SCS Manual Snow Course Data"),
    ("RRM", "SR", "Miscellaneous Hydrologic Data"),
    ("RRS", "SR", "Special Automated Hydromet Data Report"),
    ("RRX", "SR", "ASOS Shef Precipitation Criteria Test Message"),
    ("RRY", "SR", "ASOS Shef Hourly Routine Test Message"),
    ("RSD", "CX", "Daily Snotel Data"),
    ("RSL", "SD", "WSR-88D Rpg System Status Logs"),
    ("RSM", "CS", "Monthly Snotel Data"),
    ("RTP", "AS", "Max/Min Temperature and Precipitation Table"),
    ("RVA", "SR", "River Summary"),
    ("RVD", "FG", "Daily River Forecast"),
    ("RVF", "FG", "River Forecast"),
    ("RVG", "NZ", "Hydrologic Statement"),
    ("RVI", "FI", "River Ice Statement"),
    ("RVK", "NZ", "Hydrologic Statement"),
    ("RVM", "SR", "Miscellaneous River Product"),
    ("RVR", "FG", "River Recreation Statement"),
    ("RVS", "FG", "River Statement"),
    ("RWO", "SD", "Radar Superob"),
    ("RWR", "AS", "Weather Roundup"),
    ("RWS", "AW", "Weather Summary"),
    ("RWT", "WO", "Required Weekly Test"),
    ("RZF", "FP", "Regional Zone Forecast"),
    ("SAA", "XX", "Space Environment Alert Advisory"),
    ("SAB", "WW", "Snow Avalanche Bulletin"),
    ("SAD", "AW", "Daily Surface Aviation Weather Summary"),
    (
        "SAF",
        "FN",
        "Speci Agri WX FCST/Advisory/Flying Farmer FCST Outlook",
    ),
    ("SAG", "FW", "Snow Avalanche Guidance"),
    ("SAM", "AW", "Montly Surface Avaiation Weather Summary"),
    ("SAW", "WW", "Prelim Notice of Watch & Cancellation MSG"),
    ("SCC", "AC", "Storm Summary"),
    ("SCD", "CX", "Supplementary Climatological Data (ASOS)"),
    ("SCN", "SX", "Soil Climate Analysis Network Data"),
    ("SCP", "TC", "Satellite Cloud Product"),
    ("SCS", "FP", "Selected Cities Summary"),
    ("SCV", "SR", "Satellite Areal Extent of Snow Cover"),
    ("SDO", "SX", "Supplementary Data Observations (ASOS)"),
    ("SDS", "FU", "Special Dispersion Statement"),
    (
        "SEL",
        "WW",
        "Severe Local Storm Watch and Watch Cancellation",
    ),
    ("SES", "XX", "Space Enviornment Summary"),
    ("SEV", "WW", "SPC Watch Point Information Message"),
    ("SFD", "FX", "State Forecast Discussion"),
    ("SFP", "FP", "State Forecast"),
    ("SFT", "FP", "Tabular State Forecast"),
    ("SGL", "UM", "Rawinsonde Observation Significant Levels"),
    ("SGW", "FA", "Plain Language Significant Weather Forecast"),
    ("SIG", "WS", "International Sigmet/Convective Sigmet"),
    ("SIM", "AT", "Satellite Interpretation Message"),
    ("SLS", "WW", "Severe Local Storm Watch and Areal Outline"),
    ("SMA", "WW", "Marine Subtropical Storm Advistory"),
    ("SMF", "FN", "Smoke Management Weather Forecast"),
    ("SMW", "WH", "Special Marine Warning"),
    ("SPE", "TX", "Satellite Precipitation Estimates"),
    ("SPF", "WT", "Storm Strike Probability Bulletin"),
    ("SPS", "WW", "Special Weather Statement"),
    ("SPW", "WO", "Shelter in Place Warning"),
    ("SQW", "WW", "Snow Squall Warning"),
    ("SRF", "FZ", "Surf Forecast"),
    ("SSA", "XX", "Space Environment Summary Advisory"),
    ("STA", "NW", "SPC Tornado and Severe Thunderstorm Reports"),
    ("STD", "AT", "Satellite Tropical Disturbance Summary"),
    ("STO", "SX", "Road Condition Reports"),
    (
        "STP",
        "AS",
        "State Max/Min Temperature and Precipitation Table",
    ),
    ("STQ", "BM", "Spot Forecast Request"),
    ("STW", "WW", "Canadian Storm Summary"),
    ("SVR", "WU", "Severe Thunderstorm Warning"),
    ("SVS", "WW", "Severe Weather Statement"),
    ("SWD", "XX", "Space Environment Warning Advistory"),
    ("SWE", "SR", "Estimated Snow Water Equivalent by Basin"),
    ("SWO", "AC", "Severe Storm Outlook Narrative"),
    ("SWS", "AW", "State Weather Summary"),
    ("SYN", "NZ", "Hot-Air Balloon Forecast for Colorado"),
    ("TAF", "FT", "Terminal Aerodrome Forecast"),
    ("TAP", "FA", "Terminal Alerting Products"),
    ("TAV", "FP", "Travelers Forecast Table"),
    ("TCA", "FK", "Tropical Cyclone Advisory"),
    ("TCD", "WT", "Tropical Cyclone Discussion"),
    ("TCE", "WT", "Tropical Cyclone Position Estimate"),
    ("TCM", "WT", "Marine/Aviation Tropical Cyclone Advisory"),
    ("TCP", "WT", "Public Tropical Cyclone Advisory"),
    ("TCS", "TX", "Satellite Tropical Cyclone Summary"),
    ("TCU", "WT", "Tropical Cyclone Update"),
    ("TCV", "WT", "Tropical Cyclone Watch/Warning"),
    ("TIB", "WE", "Tsunami Information Bulletin"),
    ("TID", "SO", "Tide Report"),
    ("TLS", "WE", "Tsunami Local Statement"),
    ("TMA", "SE", "Tsunami Tide/Seismic Message Acknowledgment"),
    ("TOE", "WO", "911 Telephone Outage Emergency"),
    ("TOR", "WF", "Tornado Warning"),
    ("TPT", "SX", "Temperature Precipitation Table"),
    ("TSM", "SE", "Tsunami Tide/Seismic Message"),
    ("TST", "NT", "Test Message"),
    ("TSU", "WE", "Tsunami Watch/Warning"),
    ("TVL", "FP", "Travelers Forecast"),
    ("TWB", "FR", "Transcribed Weather Broadcast"),
    ("TWD", "AC", "Tropical Weather Discussion"),
    ("TWO", "AC", "Tropical Weather Outlook and Summary"),
    ("TWS", "AC", "Tropical Weather Summary"),
    ("UJX", "UJ", "JSC Upper Air Soundings"),
    (
        "ULG",
        "NX",
        "Upper Air Site Performance and Logistics Message",
    ),
    ("UVI", "AE", "Ultraviolet Index"),
    ("VAA", "FV", "Volcanic Activity Advisory"),
    ("VER", "FX", "Forecast Verification Statistics"),
    ("VFT", "NX", "Terminal Aerodrome Forecast Verification"),
    ("VGP", "PX", "Vector Graphic"),
    ("VOW", "WO", "Volcano Warning"),
    ("WA0", "WA", "Airmet (Pacific)"),
    ("WA1", "WA", "Airmet (Northeast US)"),
    ("WA2", "WA", "Airmet (Southeast US)"),
    ("WA3", "WA", "Airmet (North Central US)"),
    ("WA4", "WA", "Airmet (South Central US)"),
    ("WA5", "WA", "Airmet (US Rocky Mountains)"),
    ("WA6", "WA", "Airmet (US West Coast)"),
    ("WA7", "WA", "Airmet (Juneau Alaska)"),
    ("WA8", "WA", "Airmet (Anchorage Alaska)"),
    ("WA9", "WA", "Airmet (Fairbanks Alaska)"),
    ("WAC", "WA", "Airmet (Caribbean)"),
    ("WAR", "WO", "Space Environment Warning"),
    ("WAT", "WO", "Space Weather Message Watch"),
    ("WCL", "NW", "Watch Clearance Message"),
    ("WCN", "WW", "Watch County Notification"),
    ("WCR", "AX", "Weekly Weather and Crop Report"),
    ("WDA", "CX", "Weekly Data for Agriculture"),
    ("WEK", "FX", "27-day Space Weather Forecast"),
    ("WOU", "WO", "Watch Outline Update"),
    ("WPD", "IU", "Wind Profiler Data"),
    ("WRK", "NZ", "Local Work File"),
    ("WS1", "WS", "Sigmet (Northeast US)"),
    ("WS2", "WS", "Sigmet (Southeast US)"),
    ("WS3", "WS", "Sigmet (North Central US)"),
    ("WS4", "WS", "Sigmet (South central US)"),
    ("WS5", "WS", "Sigmet (US Rocky Mountains)"),
    ("WS6", "WS", "Sigmet (US West Coast)"),
    ("WS7", "WS", "Sigmet (Juneau Alaska)"),
    ("WS8", "WS", "Sigmet (Anchorage Alaska)"),
    ("WS9", "WS", "Sigmet (Fairbanks Alaska)"),
    ("WST", "WC", "Tropical Cyclone Sigmet"),
    ("WSV", "WV", "Volcanic Activity Sigmet"),
    ("WSW", "WW", "Winter Weather Message"),
    ("WTS", "NT", "Warning test Message"),
    ("WVM", "NW", "Warning Verification Message"),
    ("WWA", "NW", "Watch Status Report"),
    ("WWP", "WW", "Severe Thunderstorm Watch Probabilities"),
    ("ZFP", "FP", "Zone Forecast Product"),
];

#[cfg(test)]
mod tests {
    use super::detect_product_meta;

    #[test]
    fn detects_taf_with_origin_region() {
        let meta = detect_product_meta("TAFPDKGA.TXT").expect("expected metadata");
        assert_eq!(meta.pil.as_deref(), Some("TAF"));
        assert_eq!(meta.title, "Terminal Aerodrome Forecast");
        assert_eq!(meta.origin.as_deref(), Some("PDK"));
        assert_eq!(meta.region.as_deref(), Some("GA"));
    }

    #[test]
    fn detects_radar_graphic_family() {
        let meta = detect_product_meta("RADUMSVY.GIF").expect("expected metadata");
        assert_eq!(meta.family, "radar_graphic");
        assert_eq!(meta.title, "Radar graphic");
    }

    #[test]
    fn unknown_code_returns_none() {
        let meta = detect_product_meta("COMP1117.ZIP");
        assert!(meta.is_none());
    }
}

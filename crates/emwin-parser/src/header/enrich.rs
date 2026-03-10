use crate::{TextProductHeader, pil_catalog_entry};
use serde::Serialize;

/// Classification of WMO BBB (Bulletin Amendment/Correction) indicators.
///
/// BBB indicators are used to indicate product corrections, amendments, or retransmissions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BbbKind {
    /// Amendment (AA*)
    Amendment,
    /// Correction (CC*)
    Correction,
    /// Delayed Repeat (RR*)
    DelayedRepeat,
    /// Other BBB indicator type
    Other,
}

/// Enriched information about a parsed text product header.
///
/// Provides semantic metadata derived from the raw header fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TextProductEnrichment<'a> {
    /// First 3 characters of the AFOS PIL (product type code)
    pub pil_nnn: Option<&'a str>,
    /// Human-readable product type description, if known
    pub pil_description: Option<&'static str>,
    /// Classification of the BBB indicator, if present
    pub bbb_kind: Option<BbbKind>,
}

/// Enriches a parsed header with semantic information.
///
/// Extracts the PIL prefix (first 3 characters), looks up a product type description,
/// and classifies the BBB indicator.
///
/// # Arguments
///
/// * `header` - Parsed text product header
///
/// # Returns
///
/// [`TextProductEnrichment`] containing extracted and enriched metadata
///
/// # Example
///
/// ```
/// use emwin_parser::{parse_text_product, enrich_header};
///
/// let raw_text = b"000 \nFXUS61 KBOX 022101\nAFDBOX\nAREA FORECAST DISCUSSION\n";
/// let header = parse_text_product(raw_text)?;
/// let enriched = enrich_header(&header);
///
/// assert_eq!(enriched.pil_nnn, Some("AFD"));
/// assert_eq!(enriched.pil_description, Some("Area Forecast Discussion"));
/// # Ok::<(), emwin_parser::ParserError>(())
/// ```
pub fn enrich_header(header: &TextProductHeader) -> TextProductEnrichment<'_> {
    let pil_nnn = if header.afos.len() >= 3 {
        Some(&header.afos[..3])
    } else {
        None
    };
    let catalog_entry = pil_nnn.and_then(pil_catalog_entry);
    let pil_description = catalog_entry.map(|entry| entry.title);
    let bbb_kind = header.bbb.as_deref().map(classify_bbb);

    TextProductEnrichment {
        pil_nnn,
        pil_description,
        bbb_kind,
    }
}

/// Classifies a BBB indicator into its amendment/correction type.
///
/// Recognizes:
/// - AA* -> Amendment
/// - CC* -> Correction  
/// - RR* -> Delayed Repeat
/// - Other -> Other
fn classify_bbb(bbb: &str) -> BbbKind {
    let normalized = bbb.trim().to_ascii_uppercase();
    if normalized.starts_with("AA") {
        BbbKind::Amendment
    } else if normalized.starts_with("CC") {
        BbbKind::Correction
    } else if normalized.starts_with("RR") {
        BbbKind::DelayedRepeat
    } else {
        BbbKind::Other
    }
}

#[cfg(test)]
mod tests {
    use super::{BbbKind, enrich_header};
    use crate::TextProductHeader;

    fn header(afos: &str, bbb: Option<&str>) -> TextProductHeader {
        TextProductHeader {
            ttaaii: "FXUS61".to_string(),
            cccc: "KBOX".to_string(),
            ddhhmm: "022101".to_string(),
            bbb: bbb.map(str::to_string),
            afos: afos.to_string(),
        }
    }

    #[test]
    fn enriches_known_pil() {
        let header = header("AFDBOX", None);
        let enriched = enrich_header(&header);
        assert_eq!(enriched.pil_nnn, Some("AFD"));
        assert!(enriched.pil_description.is_some());
        assert_eq!(enriched.bbb_kind, None);
    }

    #[test]
    fn bbb_kind_is_classified() {
        assert_eq!(
            enrich_header(&header("AFDBOX", Some("AAA"))).bbb_kind,
            Some(BbbKind::Amendment)
        );
        assert_eq!(
            enrich_header(&header("AFDBOX", Some("CCA"))).bbb_kind,
            Some(BbbKind::Correction)
        );
        assert_eq!(
            enrich_header(&header("AFDBOX", Some("RRA"))).bbb_kind,
            Some(BbbKind::DelayedRepeat)
        );
        assert_eq!(
            enrich_header(&header("AFDBOX", Some("PAA"))).bbb_kind,
            Some(BbbKind::Other)
        );
        assert_eq!(enrich_header(&header("ZZZBOX", None)).pil_description, None);
    }
}

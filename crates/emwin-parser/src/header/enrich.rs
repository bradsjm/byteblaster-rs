use crate::{TextProductHeader, text_product_catalog_entry};
use serde::Serialize;

/// Classification of WMO BBB amendment and correction markers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BbbKind {
    /// Amendment family (`AA*`).
    Amendment,
    /// Correction family (`CC*`).
    Correction,
    /// Delayed repeat family (`RR*`).
    DelayedRepeat,
    /// Any recognized BBB value outside the handled families.
    Other,
}

/// Semantic metadata derived from a parsed text-product header.
///
/// Enrichment keeps routing decisions out of the string parser. Callers can work with stable
/// catalog-backed metadata instead of re-slicing the raw AFOS line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TextProductEnrichment<'a> {
    /// First three AFOS characters when the PIL is long enough to classify.
    pub pil_nnn: Option<&'a str>,
    /// Catalog title for the PIL prefix.
    pub pil_description: Option<&'static str>,
    /// BBB classification derived from the raw header field.
    pub bbb_kind: Option<BbbKind>,
}

/// Enriches a parsed header with catalog-backed metadata used later in the pipeline.
pub fn enrich_header(header: &TextProductHeader) -> TextProductEnrichment<'_> {
    let pil_nnn = if header.afos.len() >= 3 {
        Some(&header.afos[..3])
    } else {
        None
    };
    let catalog_entry = pil_nnn.and_then(text_product_catalog_entry);
    let pil_description = catalog_entry.map(|entry| entry.title);
    let bbb_kind = header.bbb.as_deref().map(classify_bbb);

    TextProductEnrichment {
        pil_nnn,
        pil_description,
        bbb_kind,
    }
}

/// Classifies the BBB field into the small set of routing-relevant categories.
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

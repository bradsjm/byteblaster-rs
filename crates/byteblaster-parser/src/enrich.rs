use crate::TextProductHeader;
use crate::lookup::pil_description;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BbbKind {
    Amendment,
    Correction,
    DelayedRepeat,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextProductEnrichment<'a> {
    pub pil_nnn: Option<&'a str>,
    pub pil_description: Option<&'static str>,
    pub bbb_kind: Option<BbbKind>,
}

pub fn enrich_header(header: &TextProductHeader) -> TextProductEnrichment<'_> {
    let pil_nnn = if header.afos.len() >= 3 {
        Some(&header.afos[..3])
    } else {
        None
    };
    let pil_description = pil_nnn.and_then(pil_description);
    let bbb_kind = header.bbb.as_deref().map(classify_bbb);

    TextProductEnrichment {
        pil_nnn,
        pil_description,
        bbb_kind,
    }
}

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
        assert_eq!(enriched.pil_description, Some("Area Forecast Discussion"));
        assert_eq!(enriched.bbb_kind, None);
    }

    #[test]
    fn unknown_pil_has_no_description() {
        let header = header("ZZZBOX", None);
        let enriched = enrich_header(&header);
        assert_eq!(enriched.pil_nnn, Some("ZZZ"));
        assert_eq!(enriched.pil_description, None);
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
    }
}

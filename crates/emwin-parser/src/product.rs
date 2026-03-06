use crate::data::{classify_non_text_product, container_from_filename};
use crate::{
    BbbKind, ParserError, TextProductHeader, enrich_header, parse_text_product, wmo_prefix_for_pil,
};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductEnrichmentSource {
    TextHeader,
    FilenameNonText,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProductParseWarning {
    pub kind: &'static str,
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProductEnrichment {
    pub source: ProductEnrichmentSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    pub container: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pil: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wmo_prefix: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<TextProductHeader>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bbb_kind: Option<BbbKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<ProductParseWarning>,
}

pub fn enrich_product(filename: &str, bytes: &[u8]) -> ProductEnrichment {
    if is_text_product(filename) {
        return enrich_text_product(filename, bytes);
    }

    if let Some(meta) = classify_non_text_product(filename) {
        return ProductEnrichment {
            source: ProductEnrichmentSource::FilenameNonText,
            family: Some(meta.family),
            title: Some(meta.title),
            code: Some(meta.code),
            container: meta.container,
            pil: meta.pil.map(str::to_string),
            wmo_prefix: meta.wmo_prefix,
            header: None,
            bbb_kind: None,
            warning: None,
        };
    }

    ProductEnrichment {
        source: ProductEnrichmentSource::Unknown,
        family: None,
        title: None,
        code: None,
        container: container_from_filename(filename),
        pil: None,
        wmo_prefix: None,
        header: None,
        bbb_kind: None,
        warning: None,
    }
}

fn enrich_text_product(filename: &str, bytes: &[u8]) -> ProductEnrichment {
    match parse_text_product(bytes) {
        Ok(header) => {
            let header_enrichment = enrich_header(&header);
            let pil = header_enrichment.pil_nnn.map(str::to_string);
            let title = header_enrichment.pil_description;
            let bbb_kind = header_enrichment.bbb_kind;

            ProductEnrichment {
                source: ProductEnrichmentSource::TextHeader,
                family: Some("nws_text_product"),
                title,
                code: Some(header.afos.clone()),
                container: container_from_filename(filename),
                pil: pil.clone(),
                wmo_prefix: pil.as_deref().and_then(wmo_prefix_for_pil),
                header: Some(header),
                bbb_kind,
                warning: None,
            }
        }
        Err(error) => ProductEnrichment {
            source: ProductEnrichmentSource::TextHeader,
            family: Some("nws_text_product"),
            title: None,
            code: None,
            container: container_from_filename(filename),
            pil: None,
            wmo_prefix: None,
            header: None,
            bbb_kind: None,
            warning: Some(ProductParseWarning {
                kind: "text_product_parse",
                code: parser_error_code(&error),
                message: error.to_string(),
                line: parser_error_line(&error).map(str::to_string),
            }),
        },
    }
}

fn is_text_product(filename: &str) -> bool {
    let upper = filename.to_ascii_uppercase();
    upper.ends_with(".TXT") || upper.ends_with(".WMO")
}

fn parser_error_code(error: &ParserError) -> &'static str {
    match error {
        ParserError::EmptyInput => "empty_input",
        ParserError::MissingWmoLine => "missing_wmo_line",
        ParserError::InvalidWmoHeader { .. } => "invalid_wmo_header",
        ParserError::MissingAfosLine => "missing_afos_line",
        ParserError::MissingAfos { .. } => "missing_afos",
    }
}

fn parser_error_line(error: &ParserError) -> Option<&str> {
    match error {
        ParserError::InvalidWmoHeader { line } | ParserError::MissingAfos { line } => Some(line),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{ProductEnrichmentSource, enrich_product};

    #[test]
    fn text_products_use_header_enrichment() {
        let enrichment =
            enrich_product("TAFPDKGA.TXT", b"000 \nFTUS42 KFFC 022320\nTAFPDK\nBody\n");

        assert_eq!(enrichment.source, ProductEnrichmentSource::TextHeader);
        assert_eq!(enrichment.pil.as_deref(), Some("TAF"));
        assert_eq!(enrichment.wmo_prefix, Some("FT"));
        assert_eq!(
            enrichment
                .header
                .as_ref()
                .map(|header| header.afos.as_str()),
            Some("TAFPDK")
        );
        assert!(enrichment.warning.is_none());
    }

    #[test]
    fn text_products_do_not_fall_back_to_filename_heuristics() {
        let enrichment = enrich_product("TAFPDKGA.TXT", b"000 \nINVALID HEADER\nTAFPDK\nBody\n");

        assert_eq!(enrichment.source, ProductEnrichmentSource::TextHeader);
        assert_eq!(enrichment.family, Some("nws_text_product"));
        assert_eq!(enrichment.pil, None);
        assert!(enrichment.warning.is_some());
    }

    #[test]
    fn non_text_products_use_filename_classification() {
        let enrichment = enrich_product("RADUMSVY.GIF", b"ignored");

        assert_eq!(enrichment.source, ProductEnrichmentSource::FilenameNonText);
        assert_eq!(enrichment.family, Some("radar_graphic"));
        assert_eq!(enrichment.title, Some("Radar graphic"));
        assert_eq!(enrichment.code.as_deref(), Some("RADUMSVY"));
        assert!(enrichment.header.is_none());
    }

    #[test]
    fn unknown_non_text_products_are_marked_unknown() {
        let enrichment = enrich_product("mystery.bin", b"ignored");

        assert_eq!(enrichment.source, ProductEnrichmentSource::Unknown);
        assert_eq!(enrichment.container, "raw");
        assert!(enrichment.family.is_none());
    }
}

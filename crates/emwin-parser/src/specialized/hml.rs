//! Parsing for Hydrological Markup Language products.

use quick_xml::Reader;
use quick_xml::events::Event;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HmlBulletin {
    pub documents: Vec<HmlDocument>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HmlDocument {
    pub station_id: String,
    pub station_name: Option<String>,
    pub originator: Option<String>,
    pub generation_time: Option<String>,
    pub observed: Option<HmlSeries>,
    pub forecast: Option<HmlSeries>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HmlSeries {
    pub issued: Option<String>,
    pub primary_name: Option<String>,
    pub primary_units: Option<String>,
    pub secondary_name: Option<String>,
    pub secondary_units: Option<String>,
    pub rows: Vec<HmlDatum>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HmlDatum {
    pub valid: String,
    pub primary: Option<f64>,
    pub secondary: Option<f64>,
}

pub(crate) fn parse_hml_bulletin(text: &str) -> Option<HmlBulletin> {
    let mut documents = Vec::new();
    for token in text.split("<?xml").filter(|chunk| !chunk.trim().is_empty()) {
        let xml = format!("<?xml{token}");
        if let Some(doc) = parse_hml_document(&xml) {
            documents.push(doc);
        }
    }
    (!documents.is_empty()).then_some(HmlBulletin { documents })
}

fn parse_hml_document(xml: &str) -> Option<HmlDocument> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut doc = HmlDocument {
        station_id: String::new(),
        station_name: None,
        originator: None,
        generation_time: None,
        observed: None,
        forecast: None,
    };
    let mut current_series: Option<HmlSeries> = None;
    let mut current_tag: Option<String> = None;
    let mut current_valid: Option<String> = None;
    let mut current_primary: Option<f64> = None;
    let mut current_secondary: Option<f64> = None;
    let mut current_kind: Option<String> = None;

    loop {
        match reader.read_event_into(&mut buf).ok()? {
            Event::Start(event) => {
                let name = String::from_utf8_lossy(event.local_name().as_ref()).to_string();
                match name.as_str() {
                    "site" => {
                        for attr in event.attributes().flatten() {
                            let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                            let value = attr.unescape_value().ok()?.to_string();
                            match key.as_str() {
                                "id" => doc.station_id = value,
                                "name" => doc.station_name = Some(value.trim().to_string()),
                                "originator" => doc.originator = Some(value),
                                "generationtime" => doc.generation_time = Some(value),
                                _ => {}
                            }
                        }
                    }
                    "observed" | "forecast" => {
                        current_kind = Some(name.clone());
                        let mut series = HmlSeries {
                            issued: None,
                            primary_name: None,
                            primary_units: None,
                            secondary_name: None,
                            secondary_units: None,
                            rows: Vec::new(),
                        };
                        for attr in event.attributes().flatten() {
                            let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                            let value = attr.unescape_value().ok()?.to_string();
                            match key.as_str() {
                                "issued" => series.issued = Some(value),
                                "primaryName" => series.primary_name = Some(value),
                                "primaryUnits" => series.primary_units = Some(value),
                                "secondaryName" => series.secondary_name = Some(value),
                                "secondaryUnits" => series.secondary_units = Some(value),
                                _ => {}
                            }
                        }
                        current_series = Some(series);
                    }
                    "datum" => {
                        current_valid = None;
                        current_primary = None;
                        current_secondary = None;
                    }
                    "valid" | "primary" | "secondary" => current_tag = Some(name),
                    _ => {}
                }
            }
            Event::Text(text) => {
                let value = String::from_utf8_lossy(text.as_ref()).to_string();
                match current_tag.as_deref() {
                    Some("valid") => current_valid = Some(value),
                    Some("primary") => current_primary = maybe_number(&value),
                    Some("secondary") => current_secondary = maybe_number(&value),
                    _ => {}
                }
            }
            Event::End(event) => {
                let name = String::from_utf8_lossy(event.local_name().as_ref()).to_string();
                match name.as_str() {
                    "datum" => {
                        if let (Some(series), Some(valid)) =
                            (current_series.as_mut(), current_valid.take())
                        {
                            series.rows.push(HmlDatum {
                                valid,
                                primary: current_primary.take(),
                                secondary: current_secondary.take(),
                            });
                        }
                    }
                    "observed" | "forecast" => {
                        let series = current_series.take()?;
                        match current_kind.take().as_deref() {
                            Some("observed") => doc.observed = Some(series),
                            Some("forecast") => doc.forecast = Some(series),
                            _ => {}
                        }
                    }
                    "valid" | "primary" | "secondary" => current_tag = None,
                    _ => {}
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }

    (!doc.station_id.is_empty()).then_some(doc)
}

fn maybe_number(value: &str) -> Option<f64> {
    match value.trim() {
        "-999" | "-9999" | "" => None,
        other => other.parse().ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::parse_hml_bulletin;

    #[test]
    fn parses_exact_hml_fixture() {
        let text =
            include_str!("../../tests/fixtures/specialized/202603100002-KMTR-SRUS56-HMLMTR.txt")
                .lines()
                .skip(3)
                .collect::<Vec<_>>()
                .join("\n");
        let bulletin = parse_hml_bulletin(&text).expect("hml bulletin");
        assert!(bulletin.documents.len() > 1);
        assert_eq!(bulletin.documents[0].station_id, "AAMC1");
        assert!(bulletin.documents[0].observed.is_some());
    }
}

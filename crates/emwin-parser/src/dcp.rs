//! Minimal GOES DCP telemetry bulletin parsing for WMO bulletins without AFOS PIL lines.

use serde::Serialize;

use crate::WmoHeader;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DcpBulletin {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform_id: Option<String>,
    pub lines: Vec<String>,
}

pub(crate) fn parse_dcp_bulletin(
    filename: &str,
    wmo_header: &WmoHeader,
    text: &str,
) -> Option<DcpBulletin> {
    if !looks_like_dcp_filename(filename) || !looks_like_dcp_wmo_header(wmo_header) {
        return None;
    }

    let lines = body_lines(text);
    if lines.is_empty() || !looks_like_dcp_payload(&lines) {
        return None;
    }

    Some(DcpBulletin {
        platform_id: lines.first().cloned(),
        lines,
    })
}

fn looks_like_dcp_filename(filename: &str) -> bool {
    let upper = filename.to_ascii_uppercase();
    (upper.starts_with("MISDCP") || upper.starts_with("MISA")) && upper.ends_with(".TXT")
}

fn looks_like_dcp_wmo_header(wmo_header: &WmoHeader) -> bool {
    wmo_header.cccc == "KWAL" && wmo_header.ttaaii.starts_with("SX")
}

fn body_lines(text: &str) -> Vec<String> {
    text.lines()
        .map(strip_control_chars)
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.trim().to_string())
        .collect()
}

fn strip_control_chars(line: &str) -> String {
    line.chars()
        .filter(|ch| !ch.is_ascii_control() || ch.is_ascii_whitespace())
        .collect()
}

fn looks_like_dcp_payload(lines: &[String]) -> bool {
    let first = match lines.first() {
        Some(first) => first,
        None => return false,
    };

    first.chars().filter(|ch| ch.is_ascii_digit()).count() >= 8
        && first
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch.is_ascii_whitespace())
        && lines.iter().skip(1).any(|line| {
            line.chars().all(|ch| {
                ch.is_ascii_alphanumeric() || ch.is_ascii_whitespace() || ".+-".contains(ch)
            })
        })
}

#[cfg(test)]
mod tests {
    use super::parse_dcp_bulletin;
    use crate::WmoHeader;

    fn wmo() -> WmoHeader {
        WmoHeader {
            ttaaii: "SXMS50".to_string(),
            cccc: "KWAL".to_string(),
            ddhhmm: "070258".to_string(),
            bbb: None,
        }
    }

    #[test]
    fn parses_misdcp_bulletin() {
        let text =
            "83786162 066025814\n16.23\n003\n137\n071\n088\n12.9\n137\n007\n00000\n 42-0NN  45E\n";
        let bulletin = parse_dcp_bulletin("MISDCPSV.TXT", &wmo(), text)
            .expect("expected DCP bulletin parsing to succeed");

        assert_eq!(bulletin.platform_id.as_deref(), Some("83786162 066025814"));
        assert_eq!(bulletin.lines.len(), 11);
    }

    #[test]
    fn ignores_non_dcp_filename() {
        let text = "83786162 066025814\n16.23\n";
        assert!(parse_dcp_bulletin("mystery.txt", &wmo(), text).is_none());
    }

    #[test]
    fn parses_misa_bulletin_with_control_character_prefix() {
        let text = "\x1eD6805150 066030901 \n05.06 \n008 \n180 \n056 \n098 \n12.8 \n183 \n018 \n00000 \n 39-0NN 141E\n";
        let bulletin = parse_dcp_bulletin(
            "MISA50US.TXT",
            &WmoHeader {
                ttaaii: "SXPA50".to_string(),
                cccc: "KWAL".to_string(),
                ddhhmm: "070309".to_string(),
                bbb: None,
            },
            text,
        )
        .expect("expected MISA bulletin parsing to succeed");

        assert_eq!(bulletin.platform_id.as_deref(), Some("D6805150 066030901"));
        assert_eq!(bulletin.lines.len(), 11);
    }
}

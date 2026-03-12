//! Parsing for SPC watch bulletins.

use regex::Regex;
use serde::Serialize;
use std::sync::OnceLock;

use crate::specialized::wwp::SpcWatchType;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SelBulletin {
    pub watch_number: u16,
    pub watch_type: SpcWatchType,
    pub is_test: bool,
}

pub(crate) fn parse_sel_bulletin(text: &str) -> Option<SelBulletin> {
    let normalized = text.replace('\r', "");
    let captures = watch_re().captures(&normalized)?;
    let watch_number = captures.name("num")?.as_str().parse().ok()?;
    Some(SelBulletin {
        watch_number,
        watch_type: if captures
            .name("typ")?
            .as_str()
            .eq_ignore_ascii_case("TORNADO")
        {
            SpcWatchType::Tornado
        } else {
            SpcWatchType::SevereThunderstorm
        },
        is_test: watch_number > 9000 || normalized.to_ascii_uppercase().contains("...TEST"),
    })
}

fn watch_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?im)^(?:TEST\.\.\.)?(?P<typ>TORNADO|SEVERE THUNDERSTORM)\s+WATCH(?:\s*-\s*|\s+)NUMBER\s+(?P<num>\d{1,4})(?:\.\.\.TEST)?\s*$",
        )
        .expect("valid SEL watch regex")
    })
}

#[cfg(test)]
mod tests {
    use super::{SelBulletin, parse_sel_bulletin};
    use crate::specialized::wwp::SpcWatchType;

    #[test]
    fn parses_tornado_watch() {
        let text = "\
URGENT - IMMEDIATE BROADCAST REQUESTED
Tornado Watch Number 532
NWS Storm Prediction Center Norman OK
";
        assert_eq!(
            parse_sel_bulletin(text),
            Some(SelBulletin {
                watch_number: 532,
                watch_type: SpcWatchType::Tornado,
                is_test: false,
            })
        );
    }

    #[test]
    fn parses_severe_thunderstorm_watch() {
        let text = "\
URGENT - IMMEDIATE BROADCAST REQUESTED
Severe Thunderstorm Watch Number 540
NWS Storm Prediction Center Norman OK
";
        assert_eq!(
            parse_sel_bulletin(text),
            Some(SelBulletin {
                watch_number: 540,
                watch_type: SpcWatchType::SevereThunderstorm,
                is_test: false,
            })
        );
    }

    #[test]
    fn detects_test_marker() {
        let text = "\
URGENT - IMMEDIATE BROADCAST REQUESTED
Tornado Watch Number 9999
...TEST
";
        assert_eq!(
            parse_sel_bulletin(text),
            Some(SelBulletin {
                watch_number: 9999,
                watch_type: SpcWatchType::Tornado,
                is_test: true,
            })
        );
    }

    #[test]
    fn detects_watch_number_test_threshold() {
        let text = "\
URGENT - IMMEDIATE BROADCAST REQUESTED
Severe Thunderstorm Watch Number 9001
";
        assert_eq!(
            parse_sel_bulletin(text),
            Some(SelBulletin {
                watch_number: 9001,
                watch_type: SpcWatchType::SevereThunderstorm,
                is_test: true,
            })
        );
    }

    #[test]
    fn parses_mesonet_test_issuance_line() {
        let text = "\
URGENT - IMMEDIATE BROADCAST REQUESTED
TEST...Severe Thunderstorm Watch Number 9999...TEST
NWS Storm Prediction Center Norman OK
";
        assert_eq!(
            parse_sel_bulletin(text),
            Some(SelBulletin {
                watch_number: 9999,
                watch_type: SpcWatchType::SevereThunderstorm,
                is_test: true,
            })
        );
    }
}

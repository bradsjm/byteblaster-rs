//! Direct VTEC/HVTEC regression coverage for edge-case code parsing.

use emwin_parser::{HvtecCause, HvtecRecord, HvtecSeverity, parse_hvtec_codes, parse_vtec_codes};

const FFA_SAMPLE: &str = include_str!("fixtures/products/generic/flood_watch/FFA.txt");
const FLSBOU_SAMPLE: &str = include_str!("fixtures/products/generic/flood_statement/FLSBOU.txt");
const FLWLZK_SAMPLE: &str = include_str!("fixtures/products/generic/flood_warning/FLWLZK.txt");
const FLSSEW_VTEC1970_SAMPLE: &str =
    include_str!("fixtures/products/generic/flood_statement/FLS-FLSSEW_vtec1970.txt");
const FLSLBF_0_SAMPLE: &str =
    include_str!("fixtures/products/generic/flood_statement/FLSLBF-0.txt");
const MWWAJK_SAMPLE: &str =
    include_str!("fixtures/products/generic/marine_weather_message/MWWAJK.txt");

fn mwwajk_excerpt() -> &'static str {
    let end = MWWAJK_SAMPLE
        .find("\nPKZ022-")
        .expect("expected later MWWAJK segment boundary");
    &MWWAJK_SAMPLE[..end]
}

#[test]
fn parses_ffa_sample_with_experimental_vtec_and_zeroed_hvtec() {
    let vtec = parse_vtec_codes(FFA_SAMPLE);
    let hvtec = parse_hvtec_codes(FFA_SAMPLE);

    assert_eq!(vtec.len(), 1);
    assert_eq!(vtec[0].status, 'X');
    assert_eq!(vtec[0].office, "KIWX");
    assert_eq!(vtec[0].phenomena, "FL");
    assert_eq!(vtec[0].end, None);

    assert_eq!(hvtec.len(), 1);
    assert_eq!(hvtec[0].nwslid, "NWYI3");
    assert_eq!(hvtec[0].severity, HvtecSeverity::None);
    assert_eq!(hvtec[0].cause, HvtecCause::ExcessiveRainfall);
    assert_eq!(hvtec[0].begin, None);
    assert_eq!(hvtec[0].crest, None);
    assert_eq!(hvtec[0].end, None);
    assert_eq!(hvtec[0].record, HvtecRecord::NotApplicable);
}

#[test]
fn parses_flsbou_sample_with_named_none_severity_and_rs_cause() {
    let vtec = parse_vtec_codes(FLSBOU_SAMPLE);
    let hvtec = parse_hvtec_codes(FLSBOU_SAMPLE);

    assert_eq!(vtec.len(), 1);
    assert_eq!(vtec[0].action, "EXT");
    assert_eq!(vtec[0].begin, None);
    assert!(vtec[0].end.is_some());

    assert_eq!(hvtec.len(), 1);
    assert_eq!(hvtec[0].nwslid, "00000");
    assert_eq!(hvtec[0].severity, HvtecSeverity::None);
    assert_eq!(hvtec[0].cause, HvtecCause::RainAndSnowmelt);
    assert_eq!(hvtec[0].begin, None);
    assert_eq!(hvtec[0].crest, None);
    assert_eq!(hvtec[0].end, None);
}

#[test]
fn parses_flwlzk_sample_with_open_ended_vtec_and_hvtec() {
    let vtec = parse_vtec_codes(FLWLZK_SAMPLE);
    let hvtec = parse_hvtec_codes(FLWLZK_SAMPLE);

    assert_eq!(vtec.len(), 1);
    assert_eq!(vtec[0].office, "KLZK");
    assert_eq!(vtec[0].phenomena, "FL");
    assert_eq!(vtec[0].significance, 'W');
    assert_eq!(vtec[0].end, None);

    assert_eq!(hvtec.len(), 1);
    assert_eq!(hvtec[0].nwslid, "PTTA4");
    assert_eq!(hvtec[0].severity, HvtecSeverity::Minor);
    assert_eq!(hvtec[0].cause, HvtecCause::ExcessiveRainfall);
    assert!(hvtec[0].begin.is_some());
    assert!(hvtec[0].crest.is_some());
    assert_eq!(hvtec[0].end, None);
}

#[test]
fn parses_vtec_1970_sample_as_unspecified_begin() {
    let vtec = parse_vtec_codes(FLSSEW_VTEC1970_SAMPLE);
    let hvtec = parse_hvtec_codes(FLSSEW_VTEC1970_SAMPLE);

    assert_eq!(vtec.len(), 1);
    assert_eq!(vtec[0].office, "KSEW");
    assert_eq!(vtec[0].begin, None);
    assert!(vtec[0].end.is_some());

    assert_eq!(hvtec.len(), 1);
    assert_eq!(hvtec[0].nwslid, "SQUW1");
    assert_eq!(hvtec[0].severity, HvtecSeverity::Minor);
    assert!(hvtec[0].begin.is_some());
    assert!(hvtec[0].crest.is_some());
    assert!(hvtec[0].end.is_some());
}

#[test]
fn parses_cross_year_flslbf_sample() {
    let vtec = parse_vtec_codes(FLSLBF_0_SAMPLE);
    let hvtec = parse_hvtec_codes(FLSLBF_0_SAMPLE);

    assert_eq!(vtec.len(), 1);
    assert_eq!(vtec[0].office, "KLBF");
    assert_eq!(vtec[0].action, "NEW");
    assert!(vtec[0].begin.is_some());
    assert!(vtec[0].end.is_some());

    assert_eq!(hvtec.len(), 1);
    assert_eq!(hvtec[0].nwslid, "00000");
    assert_eq!(hvtec[0].severity, HvtecSeverity::None);
    assert_eq!(hvtec[0].cause, HvtecCause::IceJam);
    assert_eq!(hvtec[0].begin, None);
    assert_eq!(hvtec[0].crest, None);
    assert_eq!(hvtec[0].end, None);
}

#[test]
fn parses_multi_segment_mwwajk_excerpt() {
    let vtec = parse_vtec_codes(mwwajk_excerpt());

    assert_eq!(vtec.len(), 6);
    assert_eq!(vtec[0].action, "EXT");
    assert_eq!(vtec[1].action, "CAN");
    assert_eq!(vtec[2].office, "PAJK");
    assert_eq!(vtec[2].begin, None);
    assert_eq!(vtec[3].phenomena, "GL");
    assert_eq!(vtec[3].significance, 'W');
    assert_eq!(vtec[4].phenomena, "UP");
    assert_eq!(vtec[4].begin, None);
    assert_eq!(vtec[5].phenomena, "UP");
    assert!(vtec[5].begin.is_some());
}

use emwin_parser::{HvtecCause, HvtecRecord, HvtecSeverity, parse_hvtec_codes, parse_vtec_codes};

// Upstream sample: akrherz/pyIEM data/product_examples/FFA.txt
const FFA_SAMPLE: &str = r#"157
WGUS63 KIWX 151621
FFAIWX
URGENT - IMMEDIATE BROADCAST REQUESTED
FLOOD WATCH
NATIONAL WEATHER SERVICE NORTHERN INDIANA
1114 AM EST MON JAN 15 2007

INZ020>021-161614-
/X.NEW.KIWX.FL.A.0001.070115T1614Z-000000T0000Z/
/NWYI3.0.ER.000000T0000Z.000000T0000Z.000000T0000Z.OO/
1114 AM EST MON JAN 15 2007
"#;

// Upstream sample: akrherz/pyIEM data/product_examples/FLSBOU.txt
const FLSBOU_SAMPLE: &str = r#"277
WGUS85 KBOU 271528
FLSBOU

FLOOD ADVISORY
NATIONAL WEATHER SERVICE DENVER CO
925 AM MDT TUE MAY 27 2014

COC049-057-300330-
/O.EXT.KBOU.FA.Y.0018.000000T0000Z-140530T0330Z/
/00000.N.RS.000000T0000Z.000000T0000Z.000000T0000Z.OO/
JACKSON CO-GRAND CO-
925 AM MDT TUE MAY 27 2014
"#;

// Upstream sample: akrherz/pyIEM data/product_examples/FLWLZK.txt
const FLWLZK_SAMPLE: &str = r#"WGUS44 KLZK 061628
FLWLZK

&&

ARC067-147-070728-
/O.NEW.KLZK.FL.W.0108.150809T1342Z-000000T0000Z/
/PTTA4.1.ER.150809T1342Z.150810T1800Z.000000T0000Z.NO/
1128 AM CDT THU AUG 6 2015
"#;

// Upstream sample: akrherz/pyIEM data/product_examples/FLS/FLSSEW_vtec1970.txt
const FLSSEW_VTEC1970_SAMPLE: &str = r#"WAC033-080941-
/O.EXT.KSEW.FL.W.0005.700101T0000Z-200108T1230Z/
/SQUW1.1.ER.200107T0835Z.200107T1500Z.200107T2325Z.NO/
542 PM PST Tue Jan 7 2020
"#;

// Upstream sample: akrherz/pyIEM data/product_examples/FLSLBF/0.txt
const FLSLBF_0_SAMPLE: &str = r#"WGUS83 KLBF 291904
FLSLBF

NEC111-011900-
/O.NEW.KLBF.FA.Y.0029.141229T1904Z-150101T1900Z/
/00000.N.IJ.000000T0000Z.000000T0000Z.000000T0000Z.OO/
LINCOLN NE-
104 PM CST MON DEC 29 2014
"#;

// Upstream sample excerpt: akrherz/pyIEM data/product_examples/MWWAJK.txt
const MWWAJK_EXCERPT: &str = r#"071
WHAK77 PAJK 021205
MWWAJK

PKZ032-022015-
/O.EXT.PAJK.SC.Y.0081.260302T1500Z-260303T1500Z/
/O.CAN.PAJK.SC.Y.0080.260303T0300Z-260303T1500Z/
Northern Chatham Strait-
305 AM AKST Mon Mar 2 2026

$$

PKZ011-021315-
/O.CAN.PAJK.SC.Y.0080.000000T0000Z-260302T1500Z/
Glacier Bay-
305 AM AKST Mon Mar 2 2026

$$

PKZ012-022015-
/O.CON.PAJK.GL.W.0050.000000T0000Z-260303T1500Z/
/O.CON.PAJK.UP.W.0006.000000T0000Z-260302T2100Z/
/O.CON.PAJK.UP.W.0007.260303T0600Z-260303T1500Z/
Northern Lynn Canal-
305 AM AKST Mon Mar 2 2026
"#;

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
    let vtec = parse_vtec_codes(MWWAJK_EXCERPT);

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

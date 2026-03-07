use chrono::Utc;
use emwin_parser::{
    parse_hvtec_codes, parse_latlon_polygons, parse_time_mot_loc_entries, parse_ugc_sections,
    parse_vtec_codes, parse_wind_hail_entries,
};

#[test]
fn integration_parse_all_types() {
    let text = r#"
000
WUUS53 KOAX 051200
        SVROAX

URGENT - IMMEDIATE BROADCAST REQUESTED
        Severe Thunderstorm Warning
National Weather Service Omaha/Valley NE
1200 PM CST Wed Mar 5 2025

NEC001>003-051300-
        /O.NEW.KOAX.SV.W.0001.250305T1200Z-250305T1800Z/
        /MSRM1.3.ER.250305T1200Z.250305T1800Z.250306T0000Z.NO/

        LAT...LON 4143 9613 4145 9610 4140 9608 4138 9612
        TIME...MOT...LOC 1200Z 300DEG 25KT 4143 9613 4140 9608
        HAILTHREAT...RADARINDICATED
        MAXHAILSIZE...1.00 IN
        WINDTHREAT...OBSERVED
        MAXWINDGUST...60 MPH

        Severe Thunderstorm Warning for...
East Central Cuming County in northeastern Nebraska...

This is a test product.
$$
        "#;

    let vtec_codes = parse_vtec_codes(text);
    assert_eq!(vtec_codes.len(), 1);
    assert_eq!(vtec_codes[0].office, "KOAX");
    assert_eq!(vtec_codes[0].phenomena, "SV");

    let ugc_sections = parse_ugc_sections(text, Utc::now());
    assert_eq!(ugc_sections.len(), 1);
    assert_eq!(
        ugc_sections[0].counties["NE"]
            .iter()
            .map(|area| area.id)
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );

    let hvtec_codes = parse_hvtec_codes(text);
    assert_eq!(hvtec_codes.len(), 1);
    assert_eq!(hvtec_codes[0].nwslid, "MSRM1");

    let polygons = parse_latlon_polygons(text);
    assert_eq!(polygons.len(), 1);
    assert_eq!(polygons[0].points.len(), 5);

    let time_mot_loc_entries = parse_time_mot_loc_entries(text, Utc::now());
    assert_eq!(time_mot_loc_entries.len(), 1);
    assert_eq!(time_mot_loc_entries[0].direction_degrees, 300);

    let wind_hail_entries = parse_wind_hail_entries(text);
    assert_eq!(wind_hail_entries.len(), 4);
}

#[test]
fn integration_no_matches_returns_empty() {
    let text = "This is just regular text with no codes.";

    assert!(parse_vtec_codes(text).is_empty());
    assert!(parse_hvtec_codes(text).is_empty());
    assert!(parse_latlon_polygons(text).is_empty());
    assert!(parse_time_mot_loc_entries(text, Utc::now()).is_empty());
    assert!(parse_ugc_sections(text, Utc::now()).is_empty());
    assert!(parse_wind_hail_entries(text).is_empty());
}

//! NWS text product body parsing module.
//!
//! This module provides parsing for geographic and temporal codes found within
//! NWS text product bodies, including VTEC, UGC, HVTEC, and LAT...LON polygons.

mod enrich;
mod hvtec;
mod latlon;
mod time_mot_loc;
mod ugc;
mod vtec;
mod wind_hail;

pub use enrich::{ProductBody, enrich_body};
pub use hvtec::{
    HvtecCause, HvtecCode, HvtecRecord, HvtecSeverity, parse_hvtec_codes,
    parse_hvtec_codes_with_issues,
};
pub use latlon::{LatLonPolygon, parse_latlon_polygons, parse_latlon_polygons_with_issues};
pub use time_mot_loc::{
    TimeMotLocEntry, parse_time_mot_loc_entries, parse_time_mot_loc_entries_with_issues,
};
pub use ugc::{
    UgcArea, UgcClass, UgcCode, UgcSection, parse_ugc_sections, parse_ugc_sections_with_issues,
};
pub use vtec::{VtecCode, parse_vtec_codes, parse_vtec_codes_with_issues};
pub use wind_hail::{
    WindHailEntry, WindHailKind, parse_wind_hail_entries, parse_wind_hail_entries_with_issues,
};

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

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

        // Test VTEC parsing
        let vtec_codes = parse_vtec_codes(text);
        assert_eq!(vtec_codes.len(), 1);
        assert_eq!(vtec_codes[0].office, "KOAX");
        assert_eq!(vtec_codes[0].phenomena, "SV");

        // Test UGC parsing
        let ugc_sections = parse_ugc_sections(text, Utc::now());
        assert_eq!(ugc_sections.len(), 1);
        assert_eq!(
            ugc_sections[0].counties["NE"]
                .iter()
                .map(|area| area.id)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );

        // Test HVTEC parsing
        let hvtec_codes = parse_hvtec_codes(text);
        assert_eq!(hvtec_codes.len(), 1);
        assert_eq!(hvtec_codes[0].nwslid, "MSRM1");

        // Test LAT/LON parsing
        let polygons = parse_latlon_polygons(text);
        assert_eq!(polygons.len(), 1);
        assert_eq!(polygons[0].points.len(), 5); // 4 points + 1 closed

        // Test TIME...MOT...LOC parsing
        let time_mot_loc_entries = parse_time_mot_loc_entries(text, Utc::now());
        assert_eq!(time_mot_loc_entries.len(), 1);
        assert_eq!(time_mot_loc_entries[0].direction_degrees, 300);

        // Test WIND/HAIL parsing
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
}

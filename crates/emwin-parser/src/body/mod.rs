//! NWS text product body parsing module.
//!
//! This module provides parsing for geographic and temporal codes found within
//! NWS text product bodies, including VTEC, UGC, HVTEC, and LAT...LON polygons.

mod hvtec;
mod latlon;
mod ugc;
mod vtec;

pub use hvtec::{HvtecCause, HvtecCode, HvtecRecord, HvtecSeverity, parse_hvtec_codes};
pub use latlon::{LatLonPolygon, parse_latlon_polygons};
pub use ugc::{UgcClass, UgcCode, UgcSection, parse_ugc_sections};
pub use vtec::{VtecAction, VtecCode, parse_vtec_codes};

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn integration_parse_all_types() {
        let text = r#"
000 
WUUS53 KOAX 051200
FFWOAX

URGENT - IMMEDIATE BROADCAST REQUESTED
Flash Flood Warning
National Weather Service Omaha/Valley NE
1200 PM CST Wed Mar 5 2025

NEC001>003-051300-
/O.NEW.KOAX.FF.W.0001.250305T1200Z-250305T1800Z/
        /MSRM1.3.ER.250305T1200Z.250305T1800Z.250306T0000Z.NO/

LAT...LON 4143 9613 4145 9610 4140 9608 4138 9612

Flash Flood Warning for...
East Central Cuming County in northeastern Nebraska...

This is a test product.
$$            
        "#;

        // Test VTEC parsing
        let vtec_codes = parse_vtec_codes(text);
        assert_eq!(vtec_codes.len(), 1);
        assert_eq!(vtec_codes[0].office, "KOAX");
        assert_eq!(vtec_codes[0].phenomena, "FF");

        // Test UGC parsing
        let ugc_sections = parse_ugc_sections(text, Utc::now());
        assert_eq!(ugc_sections.len(), 1);
        assert_eq!(ugc_sections[0].codes.len(), 3);
        assert_eq!(ugc_sections[0].codes[0].state, "NE");

        // Test HVTEC parsing
        let hvtec_codes = parse_hvtec_codes(text);
        assert_eq!(hvtec_codes.len(), 1);
        assert_eq!(hvtec_codes[0].nwslid, "MSRM1");

        // Test LAT/LON parsing
        let polygons = parse_latlon_polygons(text);
        assert_eq!(polygons.len(), 1);
        assert_eq!(polygons[0].points.len(), 5); // 4 points + 1 closed
    }

    #[test]
    fn integration_no_matches_returns_empty() {
        let text = "This is just regular text with no codes.";

        assert!(parse_vtec_codes(text).is_empty());
        assert!(parse_hvtec_codes(text).is_empty());
        assert!(parse_latlon_polygons(text).is_empty());
        assert!(parse_ugc_sections(text, Utc::now()).is_empty());
    }
}

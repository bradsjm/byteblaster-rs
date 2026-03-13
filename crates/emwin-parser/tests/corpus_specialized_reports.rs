mod common;

use common::{assert_specialized, assert_supported_family, enrich, fixture_cases};

#[test]
fn pirep_corpus_routes_to_structured_bulletins() {
    for case in fixture_cases("specialized", "pirep") {
        let enrichment = enrich(&case);
        let artifact = assert_specialized(
            &enrichment,
            "pirep_bulletin",
            &case,
            &["invalid_pirep_bulletin"],
        );
        let bulletin = artifact
            .as_pirep()
            .unwrap_or_else(|| panic!("{} -> expected PIREP artifact", case.name));

        assert!(
            !bulletin.reports.is_empty(),
            "{} -> expected at least one PIREP report",
            case.name
        );
        assert!(
            bulletin
                .reports
                .iter()
                .all(|report| !report.raw.trim().is_empty()),
            "{} -> expected preserved raw reports",
            case.name
        );
        for report in &bulletin.reports {
            assert!(
                report.station.is_some()
                    || report.time.is_some()
                    || report.location_raw.is_some()
                    || report.location.is_some()
                    || report.flight_level_ft.is_some()
                    || report.aircraft_type.is_some()
                    || report.sky_condition.is_some()
                    || report.turbulence.is_some()
                    || report.icing.is_some()
                    || report.temperature_c.is_some()
                    || report.remarks.is_some(),
                "{} -> expected PIREP semantic fields",
                case.name
            );
            if report.location.is_some() {
                assert!(
                    report.location_raw.is_some(),
                    "{} -> parsed PIREP location requires raw location text",
                    case.name
                );
            }
        }
    }
}

#[test]
fn lsr_corpus_routes_to_structured_bulletins() {
    for case in fixture_cases("specialized", "lsr") {
        let enrichment = enrich(&case);
        assert_supported_family(
            &enrichment,
            "lsr_bulletin",
            &case,
            &["invalid_lsr_bulletin", "invalid_lsr_report"],
        );
        let Some(artifact) = enrichment.parsed.as_ref() else {
            continue;
        };
        let bulletin = artifact
            .as_lsr()
            .unwrap_or_else(|| panic!("{} -> expected LSR artifact", case.name));
        assert!(
            !bulletin.reports.is_empty(),
            "{} -> expected at least one LSR report",
            case.name
        );
        for report in &bulletin.reports {
            assert!(
                report.magnitude_value.is_some() || report.magnitude_units.is_none(),
                "{} -> invalid magnitude decomposition for report {report:#?}",
                case.name
            );
        }

        if case.name == "LSR-LSRREV_trace.txt" {
            assert_eq!(
                bulletin.reports.len(),
                2,
                "{} -> expected two reports",
                case.name
            );
            assert_eq!(bulletin.reports[0].magnitude_value, Some(5.0));
            assert_eq!(
                bulletin.reports[0].magnitude_units.as_deref(),
                Some("INCH"),
                "{} -> expected normalized mixed-case units",
                case.name
            );
            assert_eq!(
                bulletin.reports[0].magnitude_qualifier.as_deref(),
                Some("M"),
                "{} -> expected mixed-case qualifier preserved",
                case.name
            );
            assert!(
                bulletin.reports[1].magnitude_value.is_none(),
                "{} -> trace-style report should not invent a numeric magnitude",
                case.name
            );
            assert!(
                bulletin.reports[1].magnitude_units.is_none(),
                "{} -> trace-style report should not populate units without a value",
                case.name
            );
        }
    }
}

#[test]
fn cli_corpus_routes_to_structured_bulletins() {
    for case in fixture_cases("specialized", "cli") {
        let enrichment = enrich(&case);
        let artifact = assert_specialized(&enrichment, "cli_bulletin", &case, &[]);
        let bulletin = artifact
            .as_cli()
            .unwrap_or_else(|| panic!("{} -> expected CLI artifact", case.name));
        assert!(
            !bulletin.reports.is_empty(),
            "{} -> expected CLI reports",
            case.name
        );
        assert!(
            bulletin
                .reports
                .iter()
                .all(|report| !report.station.trim().is_empty()),
            "{} -> expected CLI station names",
            case.name
        );
        for report in &bulletin.reports {
            assert!(
                !report.raw.trim().is_empty(),
                "{} -> expected CLI raw section text",
                case.name
            );
            assert!(
                report.valid_date.is_some()
                    || report.temperature_maximum.is_some()
                    || report.temperature_minimum.is_some()
                    || report.precip_month_inches.is_some()
                    || report.precip_year_inches.is_some()
                    || report.precip_today_inches.is_some()
                    || report.snow_month_inches.is_some()
                    || report.snow_season_inches.is_some()
                    || report.snow_today_inches.is_some()
                    || report.snow_depth_inches.is_some()
                    || report.average_sky_cover.is_some()
                    || report.average_wind_speed_mph.is_some()
                    || report.resultant_wind_speed_mph.is_some()
                    || report.resultant_wind_direction_degrees.is_some()
                    || report.highest_wind_speed_mph.is_some()
                    || report.highest_wind_direction_degrees.is_some()
                    || report.highest_gust_speed_mph.is_some(),
                "{} -> expected CLI metrics",
                case.name
            );
            if report.temperature_maximum_time.is_some() {
                assert!(
                    report.temperature_maximum.is_some(),
                    "{} -> CLI maximum time requires maximum temperature",
                    case.name
                );
            }
            if report.temperature_minimum_time.is_some() {
                assert!(
                    report.temperature_minimum.is_some(),
                    "{} -> CLI minimum time requires minimum temperature",
                    case.name
                );
            }
            if report.highest_gust_direction_degrees.is_some() {
                assert!(
                    report.highest_gust_speed_mph.is_some(),
                    "{} -> CLI gust direction requires gust speed",
                    case.name
                );
            }
        }
    }
}

#[test]
fn mos_corpus_routes_to_structured_bulletins() {
    for case in fixture_cases("specialized", "mos") {
        let enrichment = enrich(&case);
        assert_supported_family(
            &enrichment,
            "mos_bulletin",
            &case,
            &["invalid_mos_bulletin"],
        );
        let Some(artifact) = enrichment.parsed.as_ref() else {
            continue;
        };
        let bulletin = artifact
            .as_mos()
            .unwrap_or_else(|| panic!("{} -> expected MOS artifact", case.name));

        assert!(
            !bulletin.sections.is_empty(),
            "{} -> expected MOS sections",
            case.name
        );
        assert!(
            bulletin
                .sections
                .iter()
                .all(|section| !section.station.trim().is_empty()
                    && !section.model.trim().is_empty()),
            "{} -> expected populated MOS section metadata",
            case.name
        );
    }
}

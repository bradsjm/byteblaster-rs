mod common;

use common::{assert_specialized, assert_supported_family, enrich, fixture_cases};

#[test]
fn cwa_corpus_routes_to_structured_bulletins() {
    for case in fixture_cases("specialized", "cwa") {
        let enrichment = enrich(&case);
        assert_supported_family(
            &enrichment,
            "cwa_bulletin",
            &case,
            &["invalid_cwa_bulletin", "missing_reference_time"],
        );
        let Some(artifact) = enrichment.parsed.as_ref() else {
            continue;
        };
        let bulletin = artifact
            .as_cwa()
            .unwrap_or_else(|| panic!("{} -> expected CWA artifact", case.name));

        assert!(bulletin.number > 0, "{} -> expected CWA number", case.name);
        if bulletin.is_cancelled {
            assert!(
                bulletin.geometry.is_none(),
                "{} -> cancelled CWA should not carry geometry",
                case.name
            );
        } else {
            assert!(
                bulletin.geometry.is_some(),
                "{} -> active CWA should carry geometry",
                case.name
            );
        }
    }
}

#[test]
fn wwp_corpus_routes_to_structured_bulletins() {
    for case in fixture_cases("specialized", "wwp") {
        let enrichment = enrich(&case);
        assert_supported_family(
            &enrichment,
            "wwp_bulletin",
            &case,
            &["invalid_wwp_bulletin"],
        );
        let Some(artifact) = enrichment.parsed.as_ref() else {
            continue;
        };
        let bulletin = artifact
            .as_wwp()
            .unwrap_or_else(|| panic!("{} -> expected WWP artifact", case.name));
        assert!(
            bulletin.watch_number > 0,
            "{} -> expected watch number",
            case.name
        );
        assert!(
            bulletin.max_hail_inches > 0.0,
            "{} -> expected maximum hail size",
            case.name
        );
        assert!(
            bulletin.max_wind_gust_knots > 0,
            "{} -> expected maximum wind gust",
            case.name
        );
        assert!(bulletin.max_tops_feet > 0, "{} -> expected tops", case.name);
        assert!(
            bulletin.storm_motion_knots > 0,
            "{} -> expected storm motion speed",
            case.name
        );
        assert!(
            bulletin.prob_tornadoes_2_or_more <= 100
                && bulletin.prob_tornadoes_1_or_more_strong <= 100
                && bulletin.prob_severe_wind_10_or_more <= 100
                && bulletin.prob_wind_1_or_more_65kt <= 100
                && bulletin.prob_severe_hail_10_or_more <= 100
                && bulletin.prob_hail_1_or_more_2inch <= 100
                && bulletin.prob_combined_hail_wind_6_or_more <= 100,
            "{} -> expected WWP probabilities within 0-100",
            case.name
        );
    }
}

#[test]
fn saw_corpus_routes_to_structured_bulletins() {
    for case in fixture_cases("specialized", "saw") {
        let enrichment = enrich(&case);
        let artifact = assert_specialized(&enrichment, "saw_bulletin", &case, &[]);
        let bulletin = artifact
            .as_saw()
            .unwrap_or_else(|| panic!("{} -> expected SAW artifact", case.name));

        assert!(
            bulletin.watch_number > 0,
            "{} -> expected watch number",
            case.name
        );
        if matches!(bulletin.action, emwin_parser::SawAction::Issue) {
            assert!(
                bulletin.polygon.is_some() || enrichment.body.is_some(),
                "{} -> expected watch geometry or generic body for issue product",
                case.name
            );
        }
    }
}

#[test]
fn sel_corpus_routes_to_structured_bulletins() {
    for case in fixture_cases("specialized", "sel") {
        let enrichment = enrich(&case);
        let artifact = assert_specialized(&enrichment, "sel_bulletin", &case, &[]);
        let bulletin = artifact
            .as_sel()
            .unwrap_or_else(|| panic!("{} -> expected SEL artifact", case.name));
        assert!(
            bulletin.watch_number > 0,
            "{} -> expected watch number",
            case.name
        );
    }
}

#[test]
fn mcd_corpus_routes_to_structured_bulletins() {
    for case in fixture_cases("specialized", "mcd") {
        let enrichment = enrich(&case);
        let artifact = assert_specialized(&enrichment, "mcd_bulletin", &case, &[]);
        let bulletin = artifact
            .as_mcd()
            .unwrap_or_else(|| panic!("{} -> expected MCD artifact", case.name));
        assert!(
            bulletin.discussion_number > 0,
            "{} -> expected discussion number",
            case.name
        );
        assert!(
            !bulletin.raw.trim().is_empty(),
            "{} -> expected preserved MCD text",
            case.name
        );
        assert_eq!(
            bulletin.valid_from.is_some(),
            bulletin.valid_to.is_some(),
            "{} -> MCD validity bounds must be paired",
            case.name
        );
        if let Some(watch_probability_percent) = bulletin.watch_probability_percent {
            assert!(
                watch_probability_percent <= 100,
                "{} -> invalid MCD watch probability",
                case.name
            );
        }
        assert!(
            bulletin
                .attn_wfo
                .iter()
                .all(|entry| !entry.trim().is_empty()),
            "{} -> expected non-empty MCD WFO attention entries",
            case.name
        );
        assert!(
            bulletin
                .attn_rfc
                .iter()
                .all(|entry| !entry.trim().is_empty()),
            "{} -> expected non-empty MCD RFC attention entries",
            case.name
        );
    }
}

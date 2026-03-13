mod common;

use common::{assert_family, assert_specialized, enrich, fixture_cases, matches_any};

#[test]
fn cwa_corpus_routes_to_structured_bulletins() {
    for case in fixture_cases("specialized", "cwa") {
        let enrichment = enrich(&case);
        if enrichment.family != Some("cwa_bulletin") {
            assert_family(&enrichment, "nws_text_product", &case);
            assert!(
                enrichment.parsed.is_none(),
                "{} -> expected generic CWA fallback",
                case.name
            );
            continue;
        }
        let artifact = assert_specialized(&enrichment, "cwa_bulletin", &case, &[]);
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
        if enrichment.family != Some("wwp_bulletin") {
            assert_family(&enrichment, "nws_text_product", &case);
            assert!(
                enrichment.parsed.is_none(),
                "{} -> expected generic WWP fallback",
                case.name
            );
            continue;
        }
        let artifact = assert_specialized(&enrichment, "wwp_bulletin", &case, &[]);
        let bulletin = artifact
            .as_wwp()
            .unwrap_or_else(|| panic!("{} -> expected WWP artifact", case.name));
        assert!(
            bulletin.watch_number > 0,
            "{} -> expected watch number",
            case.name
        );
        assert!(bulletin.max_tops_feet > 0, "{} -> expected tops", case.name);
    }
}

#[test]
fn saw_corpus_routes_to_structured_bulletins() {
    let noisy = ["jabber"];

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
        if matches_any(&case.name, &noisy) {
            continue;
        }
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
    }
}

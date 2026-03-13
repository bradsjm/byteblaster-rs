mod common;

use common::{assert_family, assert_specialized, enrich, fixture_cases, matches_any};

#[test]
fn cf6_corpus_routes_to_structured_bulletins() {
    let sparse = ["bad", "empty", "error", "future"];

    for case in fixture_cases("specialized", "cf6") {
        let enrichment = enrich(&case);
        if enrichment.family != Some("cf6_bulletin") {
            assert_family(&enrichment, "nws_text_product", &case);
            assert!(
                enrichment.parsed.is_none(),
                "{} -> expected generic CF6 fallback",
                case.name
            );
            continue;
        }
        let artifact = assert_specialized(&enrichment, "cf6_bulletin", &case, &[]);
        let bulletin = artifact
            .as_cf6()
            .unwrap_or_else(|| panic!("{} -> expected CF6 artifact", case.name));
        assert!(
            !bulletin.station.trim().is_empty(),
            "{} -> expected station",
            case.name
        );
        assert!(
            (1..=12).contains(&bulletin.month),
            "{} -> expected month",
            case.name
        );
        if !matches_any(&case.name, &sparse) {
            assert!(
                !bulletin.rows.is_empty(),
                "{} -> expected day rows",
                case.name
            );
        }
    }
}

#[test]
fn dsm_corpus_routes_to_structured_bulletins() {
    for case in fixture_cases("specialized", "dsm") {
        let enrichment = enrich(&case);
        if enrichment.family != Some("dsm_bulletin") {
            assert!(
                matches!(
                    enrichment.family,
                    Some("nws_text_product") | Some("unsupported_wmo_bulletin")
                ),
                "{} -> expected generic or unsupported-wmo DSM fallback, got {:?}",
                case.name,
                enrichment.family
            );
            assert!(
                enrichment.parsed.is_none(),
                "{} -> expected generic DSM fallback",
                case.name
            );
            continue;
        }
        let artifact = assert_specialized(&enrichment, "dsm_bulletin", &case, &[]);
        let bulletin = artifact
            .as_dsm()
            .unwrap_or_else(|| panic!("{} -> expected DSM artifact", case.name));
        assert!(
            !bulletin.summaries.is_empty(),
            "{} -> expected DSM summaries",
            case.name
        );
    }
}

#[test]
fn hml_corpus_routes_to_structured_bulletins() {
    for case in fixture_cases("specialized", "hml") {
        let enrichment = enrich(&case);
        let artifact = assert_specialized(&enrichment, "hml_bulletin", &case, &[]);
        let bulletin = artifact
            .as_hml()
            .unwrap_or_else(|| panic!("{} -> expected HML artifact", case.name));
        assert!(
            !bulletin.documents.is_empty(),
            "{} -> expected HML documents",
            case.name
        );
    }
}

#[test]
fn ero_corpus_routes_to_structured_bulletins() {
    for case in fixture_cases("specialized", "ero") {
        let enrichment = enrich(&case);
        let artifact = assert_specialized(&enrichment, "ero_bulletin", &case, &[]);
        let bulletin = artifact
            .as_ero()
            .unwrap_or_else(|| panic!("{} -> expected ERO artifact", case.name));
        assert!(
            !bulletin.outlooks.is_empty(),
            "{} -> expected ERO outlooks",
            case.name
        );
    }
}

#[test]
fn spc_outlook_corpus_routes_to_structured_bulletins() {
    let sparse = [
        "empty",
        "nullgeom",
        "nogeom",
        "badpoly",
        "invalid",
        "shapelyerror",
    ];
    for case in fixture_cases("specialized", "spc_outlook") {
        let enrichment = match std::panic::catch_unwind(|| enrich(&case)) {
            Ok(enrichment) => enrichment,
            Err(_) => continue,
        };
        if enrichment.family != Some("spc_outlook_bulletin") {
            assert_family(&enrichment, "nws_text_product", &case);
            assert!(
                enrichment.parsed.is_none(),
                "{} -> expected generic SPC outlook fallback",
                case.name
            );
            continue;
        }
        let artifact = assert_specialized(&enrichment, "spc_outlook_bulletin", &case, &[]);
        let bulletin = artifact
            .as_spc_outlook()
            .unwrap_or_else(|| panic!("{} -> expected SPC outlook artifact", case.name));
        assert!(
            !bulletin.days.is_empty(),
            "{} -> expected SPC days",
            case.name
        );
        if !matches_any(&case.name, &sparse) {
            assert!(
                bulletin.days.iter().all(|day| !day.outlooks.is_empty()),
                "{} -> expected day outlooks",
                case.name
            );
        }
    }
}

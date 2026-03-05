use std::fs;
use std::path::{Path, PathBuf};

const PROTOCOL_DOC_PATH: &str = "../../docs/protocol.md";

const REQUIRED_REQUIREMENTS: &[&str] = &[
    "| P-001 |",
    "| P-002 |",
    "| P-003 |",
    "| P-004 |",
    "| P-005 |",
    "| P-006 |",
    "| P-007 |",
    "| P-008 |",
    "| P-009 |",
    "| P-010 |",
    "| P-011 |",
    "| P-012 |",
    "| P-013 |",
    "| P-014 |",
    "| P-015 |",
    "| P-016 |",
    "| P-017 |",
    "| P-018 |",
    "| P-019 |",
    "| P-020 |",
    "| P-021 |",
];

const REQUIRED_TEST_TARGET_SYMBOLS: &[&str] = &[
    "find_sync_recovers_after_garbage",
    "parse_header_valid",
    "parse_header_invalid_missing_fields",
    "v2_dl_bounds",
    "v2_header_gate",
    "v2_decompress_failure_drops_segment_and_emits_warning",
    "checksum_matches_reference",
    "v1_checksum_masking",
    "checksum_strict_drop",
    "fillfile_filtered",
    "trim_padding_text_wmo",
    "server_list_simple_parse",
    "server_list_full_parse",
    "unknown_frame_resync",
    "handler_error_isolated",
    "reconnect_backoff_logic",
    "watchdog_timeout_trigger",
    "backpressure_drop_emits_warning_with_counters",
    "backpressure_drop_warning_reports_and_resets_window",
    "sync_recovery",
    "inspect_valid_v1_fixture",
    "v2_fixture_bounds",
    "v2_corrupt_payload_policy",
    "checksum_fixture",
    "stream_policy_behavior",
    "server_update_simple",
    "server_update_full_format",
    "mixed_corruption_stream",
    "reconnect_failover_rotates_endpoints_with_backoff",
    "watchdog_timeout_reconnects_without_termination",
    "live_capture_replay_manifest_cases",
    "cli_output_channeling",
    "cli_stream_json_fixture",
];

const SEARCH_PATHS: &[&str] = &[
    "src/qbt_receiver/protocol/codec.rs",
    "src/qbt_receiver/protocol/checksum.rs",
    "src/qbt_receiver/protocol/server_list.rs",
    "src/qbt_receiver/client/mod.rs",
    "src/qbt_receiver/client/reconnect.rs",
    "src/qbt_receiver/client/watchdog.rs",
    "tests/protocol_parity.rs",
    "tests/reconnect_failover.rs",
    "tests/live_capture_replay.rs",
    "../byteblaster-cli/tests/cli_contract.rs",
];

#[test]
fn protocol_doc_includes_complete_requirement_matrix() {
    let protocol_doc = read_repo_file(PROTOCOL_DOC_PATH);
    for requirement in REQUIRED_REQUIREMENTS {
        assert!(
            protocol_doc.contains(requirement),
            "protocol spec missing matrix row {requirement}"
        );
    }
}

#[test]
fn protocol_doc_test_targets_resolve_to_real_symbols() {
    let haystacks: Vec<String> = SEARCH_PATHS
        .iter()
        .map(|relative_path| read_core_relative_file(relative_path))
        .collect();

    for symbol in REQUIRED_TEST_TARGET_SYMBOLS {
        assert!(
            haystacks.iter().any(|content| content.contains(symbol)),
            "protocol matrix target symbol not found in repository: {symbol}"
        );
    }
}

fn read_repo_file(relative_path: &str) -> String {
    let base = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = base.join(relative_path);
    fs::read_to_string(&path).unwrap_or_else(|err| {
        panic!(
            "failed to read repository file {}: {err}",
            path.to_string_lossy()
        )
    })
}

fn read_core_relative_file(relative_path: &str) -> String {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = base.join(relative_path);
    fs::read_to_string(&path).unwrap_or_else(|err| {
        panic!(
            "failed to read conformance search file {}: {err}",
            path.to_string_lossy()
        )
    })
}

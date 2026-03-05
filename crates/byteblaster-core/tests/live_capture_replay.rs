use byteblaster_core::qbt_receiver::{
    QbtFrameDecoder, QbtFrameEvent, QbtProtocolDecoder, QbtProtocolWarning,
};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

const FIXTURE_ROOT: &str = "../../tests/fixtures/live";
const MANIFEST_PATH: &str = "../../tests/fixtures/live/replay-cases.json";
const REQUIRED_CASE_IDS: &[&str] = &[
    "live-stream0-server-payload-001",
    "mutation-no-xor-wire",
    "mutation-strip-first-dl",
    "mutation-v2-sig-to-pk",
    "mutation-v1-cs-plus-65536",
    "mutation-remove-first-suffix-null6",
];

#[derive(Debug, Deserialize)]
struct ReplayManifest {
    version: u32,
    cases: Vec<ReplayCase>,
}

#[derive(Debug, Deserialize)]
struct ReplayCase {
    id: String,
    wire_path: String,
    chunk_bytes: Option<usize>,
    expected: ReplayExpected,
}

#[derive(Debug, Deserialize)]
struct ReplayExpected {
    result: String,
    error_contains: Option<String>,
    must_emit: Option<Vec<String>>,
    must_not_emit: Option<Vec<String>>,
    min_data_blocks: Option<usize>,
}

#[test]
fn live_capture_replay_manifest_cases() {
    let manifest_path = repo_path(MANIFEST_PATH);
    assert!(
        manifest_path.exists(),
        "live replay manifest must exist at {}",
        manifest_path.display()
    );

    let manifest_raw = fs::read_to_string(&manifest_path)
        .expect("live replay manifest should be readable utf-8 json");
    let manifest: ReplayManifest =
        serde_json::from_str(&manifest_raw).expect("live replay manifest should be valid json");

    assert_eq!(
        manifest.version, 1,
        "unsupported live replay manifest version"
    );

    assert!(
        !manifest.cases.is_empty(),
        "live replay manifest must contain at least one case"
    );

    for required_case_id in REQUIRED_CASE_IDS {
        assert!(
            manifest
                .cases
                .iter()
                .any(|case| case.id == *required_case_id),
            "live replay manifest missing required case id: {required_case_id}"
        );
    }

    for case in manifest.cases {
        run_case(&case);
    }
}

fn run_case(case: &ReplayCase) {
    let wire_path = repo_path(FIXTURE_ROOT).join(&case.wire_path);
    let wire = fs::read(&wire_path)
        .unwrap_or_else(|err| panic!("case {} missing wire file {wire_path:?}: {err}", case.id));
    let chunk_bytes = case.chunk_bytes.unwrap_or(wire.len().max(1)).max(1);

    let mut decoder = QbtProtocolDecoder::default();
    let mut all_events = Vec::new();
    let mut first_error: Option<String> = None;

    for chunk in wire.chunks(chunk_bytes) {
        match decoder.feed(chunk) {
            Ok(events) => all_events.extend(events),
            Err(err) => {
                first_error = Some(err.to_string());
                break;
            }
        }
    }

    match case.expected.result.as_str() {
        "success" => {
            assert!(
                first_error.is_none(),
                "case {} expected success but got decode error: {:?}",
                case.id,
                first_error
            );
            assert_case_events(case, &all_events);
        }
        "error" => {
            let err = first_error.unwrap_or_else(|| {
                panic!(
                    "case {} expected error but decode completed successfully",
                    case.id
                )
            });
            if let Some(needle) = &case.expected.error_contains {
                assert!(
                    err.contains(needle),
                    "case {} expected error containing {:?}, got {:?}",
                    case.id,
                    needle,
                    err
                );
            }
        }
        other => panic!(
            "case {} has unsupported expected.result value: {other}",
            case.id
        ),
    }
}

fn repo_path(relative_path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative_path)
}

fn assert_case_events(case: &ReplayCase, events: &[QbtFrameEvent]) {
    if let Some(min_data_blocks) = case.expected.min_data_blocks {
        let data_block_count = events
            .iter()
            .filter(|evt| matches!(evt, QbtFrameEvent::DataBlock(_)))
            .count();
        assert!(
            data_block_count >= min_data_blocks,
            "case {} expected at least {} data blocks, got {}",
            case.id,
            min_data_blocks,
            data_block_count
        );
    }

    if let Some(required) = &case.expected.must_emit {
        for token in required {
            assert!(
                events.iter().any(|evt| event_token(evt) == token.as_str()),
                "case {} missing required token {token}",
                case.id
            );
        }
    }

    if let Some(forbidden) = &case.expected.must_not_emit {
        for token in forbidden {
            assert!(
                events.iter().all(|evt| event_token(evt) != token.as_str()),
                "case {} emitted forbidden token {token}",
                case.id
            );
        }
    }
}

fn event_token(event: &QbtFrameEvent) -> &'static str {
    match event {
        QbtFrameEvent::DataBlock(_) => "DataBlock",
        QbtFrameEvent::ServerListUpdate(_) => "ServerListUpdate",
        QbtFrameEvent::Warning(warning) => match warning {
            QbtProtocolWarning::ChecksumMismatch { .. } => "Warning:ChecksumMismatch",
            QbtProtocolWarning::DecompressionFailed { .. } => "Warning:DecompressionFailed",
            QbtProtocolWarning::DecoderRecovered { .. } => "Warning:DecoderRecovered",
            QbtProtocolWarning::MalformedServerEntry { .. } => "Warning:MalformedServerEntry",
            QbtProtocolWarning::TimestampParseFallback { .. } => "Warning:TimestampParseFallback",
            QbtProtocolWarning::HandlerError { .. } => "Warning:HandlerError",
            QbtProtocolWarning::BackpressureDrop { .. } => "Warning:BackpressureDrop",
            _ => "Warning:Unknown",
        },
        _ => "Unknown",
    }
}

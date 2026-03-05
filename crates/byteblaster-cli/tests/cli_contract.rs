use serde_json::Value;
use std::io::Write;
use std::process::Command;

fn xor_encode(input: &[u8]) -> Vec<u8> {
    input.iter().map(|b| b ^ 0xFF).collect()
}

#[test]
fn cli_output_channeling() {
    let mut fixture = tempfile::NamedTempFile::new().expect("temp file should create");
    let payload = b"/ServerList/a.example:2211|b.example:1000\0";
    let mut decoded = Vec::new();
    decoded.extend_from_slice(b"\0\0\0\0\0\0");
    decoded.extend_from_slice(payload);
    let wire = xor_encode(&decoded);
    fixture
        .write_all(&wire)
        .expect("fixture write should succeed");

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_byteblaster-cli"));
    let output = cmd
        .args(["stream", fixture.path().to_string_lossy().as_ref()])
        .output()
        .expect("command should run");
    assert!(output.status.success(), "command should succeed");

    let stdout = String::from_utf8(output.stdout).expect("stdout should be valid utf8");
    assert!(
        stdout.trim().is_empty(),
        "stream should not write payloads to stdout"
    );

    let stderr = String::from_utf8(output.stderr).expect("stderr should be valid utf8");
    assert!(
        stderr.contains("stream capture complete"),
        "stderr should include structured stream completion log"
    );
}

#[test]
fn cli_stream_json_fixture() {
    let mut fixture = tempfile::NamedTempFile::new().expect("temp file should create");
    let payload = b"/ServerList/a.example:2211|b.example:1000\0";
    let mut decoded = Vec::new();
    decoded.extend_from_slice(b"\0\0\0\0\0\0");
    decoded.extend_from_slice(payload);
    let wire = xor_encode(&decoded);
    fixture
        .write_all(&wire)
        .expect("fixture write should succeed");

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_byteblaster-cli"));
    let output = cmd
        .args(["inspect", fixture.path().to_string_lossy().as_ref()])
        .output()
        .expect("command should run");
    assert!(output.status.success(), "command should succeed");

    assert!(
        output.stderr.is_empty(),
        "stderr must be empty for successful inspect"
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout should be valid utf8");
    let parsed: Value = serde_json::from_str(stdout.trim()).expect("stdout must be valid json");

    assert_eq!(parsed["command"], "inspect");
    assert_eq!(parsed["status"], "ok");
    assert!(parsed["event_count"].as_u64().is_some());
    assert!(parsed["events"].is_array());
}

#[test]
fn cli_download_writes_files() {
    let mut fixture = tempfile::NamedTempFile::new().expect("temp file should create");
    let body = [b'X'; 1024];
    let checksum = body.iter().map(|v| *v as u32).sum::<u32>() & 0xFFFF;

    let mut header = format!("/PFout.txt /PN 1 /PT 1 /CS {checksum} /FD01/01/2024 01:00:00 AM\r\n");
    while header.len() < 80 {
        header.push(' ');
    }

    let mut decoded = Vec::new();
    decoded.extend_from_slice(b"\0\0\0\0\0\0");
    decoded.extend_from_slice(&header.as_bytes()[..80]);
    decoded.extend_from_slice(&body);
    let wire = xor_encode(&decoded);
    fixture
        .write_all(&wire)
        .expect("fixture write should succeed");

    let out_dir = tempfile::tempdir().expect("temp dir should create");

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_byteblaster-cli"));
    let output = cmd
        .args([
            "download",
            out_dir.path().to_string_lossy().as_ref(),
            fixture.path().to_string_lossy().as_ref(),
        ])
        .output()
        .expect("command should run");
    assert!(output.status.success(), "command should succeed");

    assert!(output.stderr.is_empty(), "stderr must be empty");
    let stdout = String::from_utf8(output.stdout).expect("stdout should be valid utf8");
    let parsed: Value = serde_json::from_str(stdout.trim()).expect("stdout must be valid json");

    assert_eq!(parsed["command"], "download");
    assert_eq!(parsed["status"], "ok");
    assert_eq!(parsed["written_files"].as_array().map(|v| v.len()), Some(1));
    assert_eq!(parsed["file_events"].as_array().map(|v| v.len()), Some(1));
    assert_eq!(parsed["file_events"][0]["timestamp_utc"], 1704070800);

    let expected = out_dir.path().join("out.txt");
    assert!(expected.exists(), "output file should exist");
}

#[test]
fn cli_stream_optional_output_dir_writes_completed_files() {
    let mut fixture = tempfile::NamedTempFile::new().expect("temp file should create");
    let body = [b'Y'; 1024];
    let checksum = body.iter().map(|v| *v as u32).sum::<u32>() & 0xFFFF;

    let mut header =
        format!("/PFstream.txt /PN 1 /PT 1 /CS {checksum} /FD01/01/2024 01:00:00 AM\r\n");
    while header.len() < 80 {
        header.push(' ');
    }

    let mut decoded = Vec::new();
    decoded.extend_from_slice(b"\0\0\0\0\0\0");
    decoded.extend_from_slice(&header.as_bytes()[..80]);
    decoded.extend_from_slice(&body);
    let wire = xor_encode(&decoded);
    fixture
        .write_all(&wire)
        .expect("fixture write should succeed");

    let out_dir = tempfile::tempdir().expect("temp dir should create");

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_byteblaster-cli"));
    let output = cmd
        .args([
            "stream",
            "--output-dir",
            out_dir.path().to_string_lossy().as_ref(),
            fixture.path().to_string_lossy().as_ref(),
        ])
        .output()
        .expect("command should run");
    assert!(output.status.success(), "command should succeed");

    let stdout = String::from_utf8(output.stdout).expect("stdout should be valid utf8");
    assert!(
        stdout.trim().is_empty(),
        "stream should not write payloads to stdout"
    );

    let stderr = String::from_utf8(output.stderr).expect("stderr should be valid utf8");
    assert!(
        stderr.contains("wrote file"),
        "stderr should include structured file-write logs"
    );

    let expected = out_dir.path().join("stream.txt");
    assert!(expected.exists(), "output file should exist");
}

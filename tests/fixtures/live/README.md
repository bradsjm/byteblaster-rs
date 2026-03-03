# Live Capture Replay Fixtures

This directory stores replayable raw-wire captures for validating live-server behavior against implementation and draft assumptions.

## File Layout

- `replay-cases.json`: manifest consumed by `crates/byteblaster-core/tests/live_capture_replay.rs`.
- `*.bin`: raw inbound TCP byte streams exactly as received from server.
- `notes/*.md` (optional): human notes for provenance/anomalies.

## Capture Rules

1. Save raw bytes before any local transforms.
2. Keep each `.bin` file small and focused (single hypothesis where possible).
3. Use UTC timestamps in file names.
4. Record endpoint, capture window, and any filters in notes.

## Naming Convention

Use:

`YYYYMMDDTHHMMSSZ_<hypothesis>_<sequence>.bin`

Example:

`20260303T221500Z_xorff_confirmed_001.bin`

## Manifest Tokens

`must_emit` and `must_not_emit` support:

- `DataBlock`
- `ServerListUpdate`
- `Warning:ChecksumMismatch`
- `Warning:DecompressionFailed`
- `Warning:DecoderRecovered`
- `Warning:MalformedServerEntry`
- `Warning:TimestampParseFallback`
- `Warning:HandlerError`
- `Warning:BackpressureDrop`

## Running Replay Tests

From repository root:

`cargo test -p byteblaster-core --test live_capture_replay`

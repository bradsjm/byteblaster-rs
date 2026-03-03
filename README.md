# byteblaster-rs

Rust monorepo for ByteBlaster protocol decoding, client runtime, and CLI tooling.

## Workspace layout

- `crates/byteblaster-core` - protocol + runtime library
- `crates/byteblaster-cli` - command-line interface built on `byteblaster-core`
- `docs/protocol.md` - authoritative protocol requirements, evidence, and test mapping
- `docs/EMWIN QBT Satellite Broadcast Protocol draft v1.0.3.md` - historical external draft reference
- `tests/fixtures` - binary/json fixture corpus metadata

## Current scope

- Stateful decoder for XOR-obfuscated ByteBlaster streams (`/PF`, `/ServerList`)
- V1 + V2 segment handling with configurable checksum and compression policies
- Client connection loop with reconnect/backoff, auth ticker, watchdog, and handler isolation
- Server-list parsing and persisted lifecycle management
- File assembly with duplicate suppression
- CLI commands for stream, download, inspect, and server flows

## Rust/toolchain

- Edition: `2024`
- MSRV/toolchain target: `1.85`
- Workspace lint: `unsafe_code = forbid`

## Build and quality gates

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Quick start

Capture-file decode:

```bash
cargo run -p byteblaster-cli -- --format json inspect path/to/capture.bin
cargo run -p byteblaster-cli -- --format json stream path/to/capture.bin
cargo run -p byteblaster-cli -- --format json download ./out path/to/capture.bin
```

Live stream/download mode:

```bash
cargo run -p byteblaster-cli -- --format json stream --email you@example.com --max-events 100
cargo run -p byteblaster-cli -- --format text download ./out --email you@example.com --idle-timeout-secs 30
```

Live server mode (SSE + JSON endpoints):

```bash
cargo run -p byteblaster-cli -- server --email you@example.com --bind 127.0.0.1:8080
```

Useful server flags:

- `--stats-interval-secs 30` (set `0` to disable periodic stats logging)
- `--quiet` (suppress non-error logs)
- `--max-clients 100` (cap concurrent SSE clients)
- `--file-retention-secs 300` (in-memory completed-file TTL)
- `--max-retained-files 1000` (in-memory completed-file capacity)
- `--cors-origin "*"` or `--cors-origin "https://your-ui.example"`

Server endpoints:

- `GET /events?filter=*.TXT` - SSE event stream with wildcard filename filter (`*`, case-insensitive)
- `GET /files` - retained completed-file metadata
- `GET /files/*filename` - retained file download (URL-encoded path segment)
- `GET /health` - server health summary
- `GET /metrics` - JSON telemetry snapshot

Optional live-mode endpoint/persistence overrides:

- `--server host:port` (repeatable or comma-delimited)
- `--server-list-path ./servers.json`

## Governance

- Treat `docs/protocol.md` as the source of truth for protocol behavior.
- Any protocol behavior change must update:
  - implementation
  - tests
  - `docs/protocol.md`

## Public API Compatibility

- Stable API is provided by root-level re-exports in `byteblaster_core`.
- `byteblaster_core::unstable` has no compatibility guarantees and may change at any time.
- Public enums/structs marked `#[non_exhaustive]` must be matched with wildcard arms by consumers.
- Telemetry serde support is optional and enabled via the `telemetry-serde` crate feature.

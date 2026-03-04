# byteblaster-rs

Rust monorepo for ByteBlaster protocol decoding, client runtime, and CLI tooling.

## Workspace layout

- `crates/byteblaster-core` - protocol + runtime library
- `crates/byteblaster-cli` - command-line interface built on `byteblaster-core`
- `docs/protocol.md` - authoritative protocol requirements, evidence, and test mapping
- `docs/server-mode.md` - HTTP/SSE API contract for `byteblaster-cli server`
- `docs/relay-mode.md` - TCP relay mode behavior and metrics contract
- `docs/EMWIN QBT Satellite Broadcast Protocol draft v1.0.3.md` - historical external draft reference
- `tests/fixtures` - binary/json fixture corpus metadata

## Current scope

- Stateful decoder for XOR-obfuscated ByteBlaster streams (`/PF`, `/ServerList`)
- V1 + V2 segment handling with configurable checksum and compression policies
- Client connection loop with reconnect/backoff, auth ticker, watchdog, and handler isolation
- Server-list parsing and persisted lifecycle management
- File assembly with duplicate suppression
- CLI commands for stream, download, inspect, and server flows
- Integrated relay command for passthrough retransmission with per-client buffering limits

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

## Install

Install latest release via script:

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/bradsjm/byteblaster-rs/releases/latest/download/byteblaster-cli-installer.sh | sh
```

Run via Docker (no local Rust toolchain required):

```bash
docker run --rm ghcr.io/bradsjm/byteblaster-rs/byteblaster-cli:latest --help
docker run --rm -v "$PWD:/work" ghcr.io/bradsjm/byteblaster-rs/byteblaster-cli:latest --format json inspect /work/path/to/capture.bin
docker run --rm -p 2211:2211 -p 9090:9090 ghcr.io/bradsjm/byteblaster-rs/byteblaster-cli:latest relay --email you@example.com
```

## Use `byteblaster-core` in your app

Add the crate from this monorepo:

```toml
[dependencies]
byteblaster-core = { git = "https://github.com/bradsjm/byteblaster-rs", tag = "v0.1.0", package = "byteblaster-core" }
```

Use stable top-level exports from the crate root:

```rust
use byteblaster_core::{ClientConfig, ProtocolDecoder};

fn main() {
    let _config = ClientConfig::default();

    let mut decoder = ProtocolDecoder::default();
    let events = decoder.feed(&[]).expect("decode should not fail");
    println!("decoded {} event(s)", events.len());
}
```

For active development against local changes, use a path dependency instead:

```toml
[dependencies]
byteblaster-core = { path = "../byteblaster-rs/crates/byteblaster-core" }
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
cargo run -p byteblaster-cli -- --format text stream --output-dir ./out --email you@example.com --max-events 100
cargo run -p byteblaster-cli -- --format text download ./out --email you@example.com --idle-timeout-secs 30
```

Optional stream file writing:

- `stream --output-dir <PATH>` writes each completed assembled file while still emitting stream events.
- Applies to both capture mode (`stream <capture.bin>`) and live mode (`stream --email ...`).
- JSON stream output includes `written_files` when `--output-dir` is used.

CLI logging format:

- Human-readable diagnostics use colorized level tags (for example `[OK]`, `[INFO]`, `[WARN]`, `[ERROR]`, `[STATS]`).
- Command payloads remain on `stdout`; diagnostics/logging remain on `stderr`.

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

Relay mode (raw TCP passthrough + metrics):

```bash
cargo run -p byteblaster-cli -- relay --email you@example.com
```

Useful relay flags:

- `--bind 0.0.0.0:2211` (downstream client listener)
- `--max-clients 100` (connection cap; over-capacity clients receive server-list frame then disconnect)
- `--auth-timeout-secs 720` (downstream re-authentication window)
- `--client-buffer-bytes 65536` (per-client backpressure budget)
- `--metrics-bind 127.0.0.1:9090` (metrics listener)

Relay endpoints:

- `GET /health` - relay health summary
- `GET /metrics` - relay telemetry snapshot (connections, auth, buffering, and quality state)

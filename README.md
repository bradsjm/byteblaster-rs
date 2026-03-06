# byteblaster-rs

Rust monorepo for EMWIN protocol decoding, client runtime, and CLI tooling.

## Install

Install latest release via script:

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/bradsjm/byteblaster-rs/releases/latest/download/byteblaster-cli-installer.sh | sh
```

Run via Docker (no local Rust toolchain required):

```bash
docker run --rm ghcr.io/bradsjm/byteblaster-rs/byteblaster-cli:latest --help
docker run --rm -p 2211:2211 -p 9090:9090 ghcr.io/bradsjm/byteblaster-rs/byteblaster-cli:latest relay --username you@example.com
```

## Use `byteblaster-core` in your app

Add the crate from this monorepo:

```toml
[dependencies]
byteblaster-core = { git = "https://github.com/bradsjm/byteblaster-rs", tag = "v0.1.0", package = "byteblaster-core" }
```

Use protocol-specific namespaces from the crate root:

```rust
use byteblaster_core::qbt_receiver::{QbtFrameDecoder, QbtProtocolDecoder};

fn main() {
    let mut decoder = QbtProtocolDecoder::default();
    let events = decoder.feed(&[]).expect("decode should not fail");
    println!("decoded {} event(s)", events.len());
}
```

For active development against local changes, use a path dependency instead:

```toml
[dependencies]
byteblaster-core = { path = "../byteblaster-rs/crates/byteblaster-core" }
```

`byteblaster-core` protocol feature flags:

```toml
[dependencies]
byteblaster-core = { path = "../byteblaster-rs/crates/byteblaster-core", default-features = false, features = ["qbt"] }
```

## Quick start

Live stream/download mode:

```bash
cargo run -p byteblaster-cli -- stream --username you@example.com --max-events 100
cargo run -p byteblaster-cli -- stream --output-dir ./out --username you@example.com --max-events 100
cargo run -p byteblaster-cli -- download ./out --username you@example.com --idle-timeout-secs 30
cargo run -p byteblaster-cli -- stream --receiver wxwire --username you@example.com --password 'secret'
cargo run -p byteblaster-cli -- download ./out --receiver wxwire --username you@example.com --password 'secret'
```

Optional stream file writing:

- `stream --output-dir <PATH>` writes each completed assembled file while still emitting stream events.
- Stream output is structured logs on `stderr` only; stream does not emit JSON payloads.

CLI logging format:

- Diagnostics/logging use canonical `tracing-subscriber` formatting and `RUST_LOG` filtering.
- Command payloads remain on `stdout`; diagnostics/logging remain on `stderr`.
- This `stdout`/`stderr` split applies to all modes, including `relay`.

Live server mode (SSE + JSON endpoints):

```bash
cargo run -p byteblaster-cli -- server --username you@example.com --bind 127.0.0.1:8080
cargo run -p byteblaster-cli -- server --receiver wxwire --username you@example.com --password 'secret' --bind 127.0.0.1:8080
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
cargo run -p byteblaster-cli -- relay --username you@example.com
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

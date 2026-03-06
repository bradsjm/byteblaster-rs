# emwin-protocol

Core Rust library for EMWIN protocol parsing, client runtime, and file assembly.

## What it provides

- Protocol layer
  - XOR wire transform handling (`0xFF`)
  - Stateful frame decoder (`/PF`, `/ServerList`)
  - V1 + V2 body handling and checksum validation policies
  - Compression policy for V2 payloads (`RequireZlibHeader`, `TryAlways`)
- Client runtime
  - TCP connection loop with endpoint rotation and backoff
  - Authentication heartbeat
  - Watchdog health monitoring
  - Event stream + handler fanout with fault isolation
  - Server-list persistence and rotation management
- Unified ingest runtime
  - Single receiver abstraction over QBT and Weather Wire
  - Normalized product, telemetry, and warning events
- File layer
  - Segment-to-file assembly
  - Duplicate block/file suppression

## Public modules

- `ingest`
- `qbt_receiver`
- `wxwire_receiver`

## Receiver lifecycle contract

- Receiver configs are validated before startup.
- `start()` may only be called once per running instance.
- `events()` is a single-consumer subscription; calling it more than once now returns an error.
- `stop()` is idempotent and shuts down background tasks before returning.

## Re-exported entry points

See `src/lib.rs` for current canonical exports.

Key exported QBT receiver types include:

- `QbtReceiver`, `QbtReceiverBuilder`, `QbtReceiverEvent`
- `QbtReceiverConfig`, `QbtDecodeConfig`, `QbtChecksumPolicy`, `QbtV2CompressionPolicy`
- `QbtFrameEvent`, `QbtSegment`, `QbtServerList`
- `QbtFileAssembler`, `QbtCompletedFile`

## Example (decoder)

```rust
use emwin_core::qbt_receiver::{QbtFrameDecoder, QbtProtocolDecoder};

fn decode_wire_chunk(wire: &[u8]) {
    let mut decoder = QbtProtocolDecoder::default();
    let events = decoder.feed(wire).expect("decode failed");
    println!("decoded {} event(s)", events.len());
}
```

## Using from another app

Add `emwin-protocol` from the repository:

```toml
[dependencies]
emwin-protocol = { git = "https://github.com/bradsjm/emwin-rs", tag = "v0.1.0", package = "emwin-protocol" }
```

Or use a local path while developing both projects:

```toml
[dependencies]
emwin-protocol = { path = "../emwin-rs/crates/emwin-protocol" }
```

Minimal usage example:

```rust
use emwin_core::ingest::{IngestConfig, IngestReceiver};
use emwin_core::qbt_receiver::{QbtDecodeConfig, QbtReceiverConfig, default_qbt_upstream_servers};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut receiver = IngestReceiver::build(IngestConfig::Qbt(QbtReceiverConfig {
        email: "you@example.com".to_string(),
        servers: default_qbt_upstream_servers(),
        server_list_path: None,
        follow_server_list_updates: true,
        reconnect_delay_secs: 5,
        connection_timeout_secs: 5,
        watchdog_timeout_secs: 49,
        max_exceptions: 10,
        decode: QbtDecodeConfig::default(),
    }))?;
    receiver.start()?;
    receiver.stop().await?;
    Ok(())
}
```

## Quality gates

From workspace root:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p emwin-protocol
```

## Normative source

Protocol requirements and expected behavior are defined in:

- `../../docs/protocol.md`

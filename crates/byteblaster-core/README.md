# byteblaster-core

Core Rust library for ByteBlaster protocol parsing, client runtime, and file assembly.

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
- File layer
  - Segment-to-file assembly
  - Duplicate block/file suppression

## Public modules

- `qbt_receiver`
- `wxwire_receiver`

## Re-exported entry points

See `src/lib.rs` for current canonical exports.

Key exported QBT receiver types include:

- `QbtReceiver`, `QbtReceiverBuilder`, `QbtReceiverEvent`
- `QbtReceiverConfig`, `QbtDecodeConfig`, `QbtChecksumPolicy`, `QbtV2CompressionPolicy`
- `QbtFrameEvent`, `QbtSegment`, `QbtServerList`
- `QbtFileAssembler`, `QbtCompletedFile`

## Example (decoder)

```rust
use byteblaster_core::qbt_receiver::{QbtFrameDecoder, QbtProtocolDecoder};

fn decode_wire_chunk(wire: &[u8]) {
    let mut decoder = QbtProtocolDecoder::default();
    let events = decoder.feed(wire).expect("decode failed");
    println!("decoded {} event(s)", events.len());
}
```

## Using from another app

Add `byteblaster-core` from the repository:

```toml
[dependencies]
byteblaster-core = { git = "https://github.com/bradsjm/byteblaster-rs", tag = "v0.1.0", package = "byteblaster-core" }
```

Or use a local path while developing both projects:

```toml
[dependencies]
byteblaster-core = { path = "../byteblaster-rs/crates/byteblaster-core" }
```

Minimal usage example:

```rust
use byteblaster_core::qbt_receiver::{QbtFrameDecoder, QbtProtocolDecoder};

fn main() {
    let mut decoder = QbtProtocolDecoder::default();
    let events = decoder.feed(&[]).expect("decode should not fail");
    println!("decoded {} event(s)", events.len());
}
```

## Quality gates

From workspace root:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p byteblaster-core
```

## Normative source

Protocol requirements and expected behavior are defined in:

- `../../docs/protocol.md`

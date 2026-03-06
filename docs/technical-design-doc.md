# ByteBlaster Authoritative Design Document

## Document Information

- Version: 3.1.0
- Last Updated: 2026-03-05
- Status: Authoritative product/functionality specification
- Protocol normative authority: `docs/protocol.md`

---

## 1) Repository Scope

`byteblaster-rs` is a Rust workspace with three crates:

- `crates/byteblaster-core`: protocol decoding, client runtime, server-list lifecycle, file assembly, stream adapters.
- `crates/byteblaster-cli`: command-line interface built on `byteblaster-core` and `byteblaster-parser`.
- `crates/byteblaster-parser`: WMO/AFOS text product parsing with header enrichment and PIL lookup.

The workspace targets:

- Edition `2024`
- Rust version `1.88`
- `unsafe_code = forbid`

---

## 2) Current Implementation Status

- Stateful decoder handles sync recovery, unknown-frame resync, V1/V2 parsing, server-list frames, and chunk-boundary prefix splits.
- `/FD` timestamps are parsed into `QbtSegment.timestamp_utc`; parse failures emit a warning and use receive-time fallback.
- Server-list lifecycle manager supports load/save/rotation/update with atomic persistence writes.
- WMO/AFOS text product parser handles header extraction, text conditioning, and BBB indicator classification.
- PIL (Product Identifier List) lookup provides product type descriptions with binary search.
- CLI commands `inspect`, `stream`, `download`, `server`, and `relay` are implemented.
- `server` command provides HTTP/SSE API for real-time event streaming and file access.
- `relay` command provides low-latency TCP passthrough with metrics endpoints.
- CLI live-network mode is implemented for `stream`, `download`, and `server` when no capture input is provided.
- Workspace quality gates are `fmt`, `clippy -D warnings`, and `test`.

---

## 3) Functional Specification (Non-Protocol)

This document defines product scope and functional expectations. Protocol wire semantics,
validation rules, and requirement-to-test mapping are defined only in `docs/protocol.md`.

### 3.1 Client Runtime Behavior

- Client startup validates config before network activity.
- Runtime supports endpoint rotation, bounded reconnect backoff, and watchdog-driven recovery.
- Server-list lifecycle supports load/save/rotation/update with persistence.
- Receiver event subscriptions are single-consumer by contract; repeated `events()` calls are an error.
- Shared runtime lifecycle mechanics are centralized so QBT and Weather Wire follow the same start/stop/subscription rules.

### 3.2 CLI Functional Contract

- `inspect` decodes capture input and emits event summaries.
- `stream` and `download` support capture-file mode and live-network mode.
- Live mode requires user email and produces continuous output until limits/timeout/shutdown.
- Output channel boundary is strict: payloads on `stdout`, diagnostics on `stderr`.

### 3.3 File Assembly and Delivery

- `download` reconstructs completed files from valid segments.
- Incomplete/corrupt segments are excluded by core validation policy.
- Output summaries are available in both text and JSON formats.

---

## 4) Workspace Layout

```text
byteblaster-rs/
  Cargo.toml
  Cargo.lock
  rust-toolchain.toml
  crates/
    byteblaster-core/
      Cargo.toml
      src/
        lib.rs
        runtime_support.rs
        config.rs
        error.rs
        protocol/
          mod.rs
          model.rs
          codec.rs
          checksum.rs
          compression.rs
          server_list.rs
          auth.rs
        client/
          mod.rs
          connection.rs
          reconnect.rs
          server_list_manager.rs
          watchdog.rs
        file/
          mod.rs
          assembler.rs
        stream/
          mod.rs
          segment_stream.rs
          file_stream.rs
      tests/
        protocol_parity.rs
        protocol_prop.rs
        reconnect_failover.rs
    byteblaster-cli/
      Cargo.toml
      src/
        main.rs
        default_servers.rs
        product_meta.rs
        cmd/
          mod.rs
          inspect.rs
          stream.rs
          download.rs
          event_output.rs
          server.rs
        live/
          mod.rs
          config.rs
          file_pipeline.rs
          server.rs
          server_support.rs
          server/
            server_http.rs
            server_ingest.rs
            types.rs
          shared.rs
          stream.rs
        relay/
          mod.rs
          auth.rs
          config.rs
          runtime.rs
          server_list.rs
          state.rs
      tests/
        cli_contract.rs
    byteblaster-parser/
      Cargo.toml
      src/
        lib.rs
        parser.rs
        enrich.rs
        lookup/
          mod.rs
          pil_generated.rs
  docs/
    protocol.md
    server-mode.md
    relay-mode.md
    technical-design-doc.md
    EMWIN QBT Satellite Broadcast Protocol draft v1.0.3.md
```

### 4.1 Parser Architecture

`byteblaster-parser` provides WMO/AFOS text product parsing capabilities:

**Core components:**

- **TextProductHeader**: Parsed header structure containing:
  - `ttaaii`: WMO product type indicator (6 chars, normalized from 4 to "00")
  - `cccc`: 4-letter ICAO station code
  - `ddhhmm`: Day and time (UTC)
  - `bbb`: Optional BBB indicator (CORrection, AMEndment, RR, etc.)
  - `afos`: Product Identifier Line (6 chars)

- **Text conditioning**: Handles text encoding issues automatically:
  - Strips SOH (`\u{1}`) and ETX (`\u{3}`) control characters
  - Removes null bytes
  - Inserts missing LDM sequence numbers
  - Normalizes line endings

- **PIL (Product Identifier List) lookup**:
  - Binary search on sorted PIL array (O(log n))
  - Returns human-readable product type descriptions
  - Metadata tracking: entry count, generation timestamp, source repo/commit
  - Common products: AFD (Area Forecast Discussion), SVR (Severe Thunderstorm Warning), TOR (Tornado Warning), etc.

- **Header enrichment**:
  - Extracts PIL NNN (first 3 characters of AFOS)
  - Looks up product type description
  - Classifies BBB indicators: Amendment (AA*), Correction (CC*), Delayed Repeat (RR*), Other

**Error handling:**

Typed errors for all failure modes:
- `EmptyInput`: Text is empty after conditioning
- `MissingWmoLine`: No WMO header line found
- `InvalidWmoHeader`: WMO header format is invalid
- `MissingAfosLine`: No AFOS line found
- `MissingAfos`: Cannot parse AFOS PIL from line

**Public API:**

```rust
// Parse a WMO/AFOS text product header
pub fn parse_text_product(bytes: &[u8]) -> Result<TextProductHeader, ParserError>

// Enrich a parsed header with semantic information
pub fn enrich_header(header: &TextProductHeader) -> TextProductEnrichment<'_>

// Look up product type description by PIL prefix
pub fn pil_description(nnn: &str) -> Option<&'static str>

// PIL metadata constants
pub const PIL_ENTRY_COUNT: usize;
pub const PIL_GENERATED_AT_UTC: &str;
pub const PIL_SOURCE_REPO: &str;
pub const PIL_SOURCE_COMMIT: &str;
```

---

## 5) Root `Cargo.toml` (Current)

```toml
[workspace]
members = [
    "crates/byteblaster-core",
    "crates/byteblaster-cli",
    "crates/byteblaster-parser",
]
resolver = "3"

[workspace.package]
edition = "2024"
version = "0.2.0"
rust-version = "1.88"
license = "MIT"
readme = "README.md"
repository = "https://github.com/bradsjm/byteblaster-rs"
homepage = "https://github.com/bradsjm/byteblaster-rs"

[workspace.dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "macros", "net", "time", "sync", "io-util", "signal"] }
tokio-util = { version = "0.7", features = ["codec"] }
tokio-stream = "0.1"
bytes = { version = "1", features = ["serde"] }
thiserror = "2"
anyhow = "1"
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"] }
flate2 = "1"
futures = "0.3"
regex = "1"
time = { version = "0.3.47", features = ["parsing", "macros"] }
axum = "0.7"
tower-http = { version = "0.5", features = ["cors"] }
tower = "0.5"

[workspace.lints.rust]
unsafe_code = "forbid"

[profile.dist]
inherits = "release"
```

---

## 7) Public API Snapshot

`byteblaster-core/src/lib.rs` exports:

```rust
pub mod client;
pub mod config;
pub mod error;
pub mod file;
pub mod protocol;
pub mod stream;

pub use client::{ByteBlasterClient, Client, ClientBuilder, ClientEvent, ClientTelemetrySnapshot};
pub use config::{ChecksumPolicy, ClientConfig, DecodeConfig, V2CompressionPolicy};
pub use error::{ConfigError, CoreError, CoreResult, ProtocolError};
pub use file::{CompletedFile, FileAssembler, SegmentAssembler};
pub use protocol::checksum::calculate_checksum;
pub use protocol::codec::{FrameDecoder, FrameEncoder, ProtocolDecoder};
pub use protocol::model::{AuthMessage, FrameEvent, ProtocolVersion, ProtocolWarning, QbtSegment, ServerList};
```

Core behavior controls:

- `ChecksumPolicy::StrictDrop`
- `V2CompressionPolicy::{RequireZlibHeader, TryAlways}`
- `DecodeConfig::default()`:
  - `checksum_policy = StrictDrop`
  - `compression_policy = RequireZlibHeader`
  - `max_v2_body_size = 1024`

---

## 8) CLI Command Surface

Supported commands:

- `inspect [input]`
- `stream [input] [--email ... --server ... --server-list-path ... --max-events ... --idle-timeout-secs ... --output-dir ...]`
- `download <output_dir> [input] [--email ... --server ... --server-list-path ... --max-events ... --idle-timeout-secs ...]`
- `server --email ... [--server ... --server-list-path ... --bind ... --cors-origin ... --max-clients ... --stats-interval-secs ... --file-retention-secs ... --max-retained-files ... --quiet]`
- `relay --email ... [--server ... --server-list-path ... --bind ... --max-clients ... --auth-timeout-secs ... --client-buffer-bytes ... --metrics-bind ...]`

Mode behavior:

- If `input` is provided, `stream` and `download` run in capture-file mode.
- If `input` is omitted, `stream`, `download`, and `server` run in live-network mode and require `--email`.
- `server` always runs in live-network mode with HTTP/SSE endpoints.
- `relay` always runs in live-network mode with TCP passthrough.

Output contract:

- `stdout`: command payload (`text` or `json`)
- `stderr`: logs/diagnostics

---

## 9) Test Coverage Snapshot

Current integration targets:

- `crates/byteblaster-core/tests/protocol_parity.rs`
- `crates/byteblaster-core/tests/protocol_prop.rs`
- `crates/byteblaster-core/tests/reconnect_failover.rs`
- `crates/byteblaster-cli/tests/cli_contract.rs`

Coverage ownership:

- Protocol conformance coverage is defined by `docs/protocol.md` requirement matrix.
- Product/CLI behavior coverage remains in crate test suites listed above.

---

## 10) Quality Gates

Run from repository root:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

---

## 11) Governance Rules

- `docs/protocol.md` is the normative source of truth for protocol behavior and requirement-to-test mapping.
- Protocol behavior changes must update:
  - implementation
  - tests
  - `docs/protocol.md`
- Keep CLI concerns in `byteblaster-cli` and protocol/runtime concerns in `byteblaster-core`.
- Parser concerns belong in `byteblaster-parser` (WMO/AFOS parsing, PIL lookup, header enrichment).

---

End of guide.

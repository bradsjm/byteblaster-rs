# ByteBlaster Authoritative Design Document

## Document Information

- Version: 3.0.1
- Last Updated: 2026-03-03
- Status: Authoritative product/functionality specification
- Protocol normative authority: `docs/protocol.md`

---

## 1) Repository Scope

`byteblaster-rs` is a Rust workspace with two crates:

- `crates/byteblaster-core`: protocol decoding, client runtime, server-list lifecycle, file assembly, stream adapters.
- `crates/byteblaster-cli`: command-line interface built on `byteblaster-core`.

The workspace targets:

- Edition `2024`
- Rust version `1.85`
- `unsafe_code = forbid`

---

## 2) Current Implementation Status

- Stateful decoder handles sync recovery, unknown-frame resync, V1/V2 parsing, server-list frames, and chunk-boundary prefix splits.
- `/FD` timestamps are parsed into `QbtSegment.timestamp_utc`; parse failures emit a warning and use receive-time fallback.
- Server-list lifecycle manager supports load/save/rotation/update with atomic persistence writes.
- CLI commands `inspect`, `stream`, and `download` are implemented for capture-file input.
- CLI live-network mode is implemented for `stream` and `download` when no capture input is provided.
- Workspace quality gates are `fmt`, `clippy -D warnings`, and `test`.

---

## 3) Functional Specification (Non-Protocol)

This document defines product scope and functional expectations. Protocol wire semantics,
validation rules, and requirement-to-test mapping are defined only in `docs/protocol.md`.

### 3.1 Client Runtime Behavior

- Client startup validates config before network activity.
- Runtime supports endpoint rotation, bounded reconnect backoff, and watchdog-driven recovery.
- Server-list lifecycle supports load/save/rotation/update with persistence.

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
          manager.rs
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
        cmd/
          mod.rs
          inspect.rs
          stream.rs
          download.rs
        output.rs
      tests/
        cli_contract.rs
  docs/
    byteblaster.md
    protocol.md
    EMWIN QBT Satellite Broadcast Protocol draft v1.0.3.md
```

---

## 5) Root `Cargo.toml` (Current)

```toml
[workspace]
members = ["crates/byteblaster-core", "crates/byteblaster-cli"]
resolver = "3"

[workspace.package]
edition = "2024"
version = "0.1.0"
rust-version = "1.85"
license = "MIT"

[workspace.dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "macros", "net", "time", "sync", "io-util"] }
tokio-util = { version = "0.7", features = ["codec"] }
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
time = { version = "0.3", features = ["parsing", "macros"] }

[workspace.lints.rust]
unsafe_code = "forbid"
```

---

## 6) Public API Snapshot

`byteblaster-core/src/lib.rs` exports:

```rust
pub mod client;
pub mod config;
pub mod error;
pub mod file;
pub mod protocol;
pub mod stream;

pub use client::{Client, ClientBuilder, ClientEvent};
pub use config::{ChecksumPolicy, ClientConfig, DecodeConfig, V2CompressionPolicy};
pub use file::{CompletedFile, FileAssembler};
pub use protocol::model::{FrameEvent, QbtSegment, ServerList};
```

Core behavior controls:

- `ChecksumPolicy::StrictDrop`
- `V2CompressionPolicy::{RequireZlibHeader, TryAlways}`
- `DecodeConfig::default()`:
  - `checksum_policy = StrictDrop`
  - `compression_policy = RequireZlibHeader`
  - `max_v2_body_size = 1024`

---

## 7) CLI Command Surface

Supported commands:

- `inspect [input]`
- `stream [input] [--email ... --server ... --server-list-path ... --max-events ... --idle-timeout-secs ...]`
- `download <output_dir> [input] [--email ... --server ... --server-list-path ... --max-events ... --idle-timeout-secs ...]`

Mode behavior:

- If `input` is provided, `stream` and `download` run in capture-file mode.
- If `input` is omitted, `stream` and `download` run in live-network mode and require `--email`.

Output contract:

- `stdout`: command payload (`text` or `json`)
- `stderr`: logs/diagnostics

---

## 8) Test Coverage Snapshot

Current integration targets:

- `crates/byteblaster-core/tests/protocol_parity.rs`
- `crates/byteblaster-core/tests/protocol_prop.rs`
- `crates/byteblaster-core/tests/reconnect_failover.rs`
- `crates/byteblaster-cli/tests/cli_contract.rs`

Coverage ownership:

- Protocol conformance coverage is defined by `docs/protocol.md` requirement matrix.
- Product/CLI behavior coverage remains in crate test suites listed above.

---

## 9) Quality Gates

Run from repository root:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

---

## 10) Governance Rules

- `docs/protocol.md` is the normative source of truth for protocol behavior and requirement-to-test mapping.
- Protocol behavior changes must update:
  - implementation
  - tests
  - `docs/protocol.md`
- Keep CLI concerns in `byteblaster-cli` and protocol/runtime concerns in `byteblaster-core`.

---

End of guide.

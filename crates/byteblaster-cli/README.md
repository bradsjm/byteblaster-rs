# byteblaster-cli

CLI application for ByteBlaster protocol inspection, event streaming, and file download workflows.

## Commands

- `inspect [input]`
  - Decode a capture file (or stdin) and report parsed events.
- `stream [input]`
  - Capture mode: decode events from a capture file.
  - Live mode (when `input` is omitted): connect to ByteBlaster servers and stream events.
  - Optional `--output-dir <PATH>`: assemble completed files from stream events and write them to disk.
- `download <output_dir> [input]`
  - Capture mode: decode + assemble files from a capture file into `output_dir`.
  - Live mode (when `input` is omitted): connect live, assemble completed files, write to `output_dir`.

## Output formats

- `stream` always emits structured `tracing` logs to `stderr` and does not emit JSON payloads.
- `inspect` and `download` emit machine-readable JSON payloads to `stdout`.

Contract:

- command payloads are written to `stdout`
- diagnostics and warnings are written to `stderr`
- diagnostics use canonical `tracing-subscriber` formatting (configure via `RUST_LOG`; ANSI style via `RUST_LOG_STYLE=auto|always|never`)

## Live mode options

For `stream` and `download` when no positional `input` is provided:

- `--email <EMAIL>` (required)
- `--server <host:port>` (optional, repeatable or comma-delimited)
- `--server-list-path <PATH>` (optional persisted server list path)
- `--max-events <N>` (optional for `stream`; default `200` for `download`)
- `--idle-timeout-secs <SECONDS>` (default `20`)

Additional `stream` option:

- `--output-dir <PATH>` (optional; writes each completed file assembled from streamed blocks)

If `--server` is omitted, built-in default endpoints are used.

## Examples

Capture mode:

```bash
cargo run -p byteblaster-cli -- inspect ./capture.bin
cargo run -p byteblaster-cli -- stream ./capture.bin
cargo run -p byteblaster-cli -- stream --output-dir ./out ./capture.bin
cargo run -p byteblaster-cli -- download ./out ./capture.bin
```

Live mode:

```bash
cargo run -p byteblaster-cli -- stream --email you@example.com --max-events 100
cargo run -p byteblaster-cli -- stream --output-dir ./out --email you@example.com --max-events 100
cargo run -p byteblaster-cli -- download ./out --email you@example.com --idle-timeout-secs 30
```

## Development checks

From workspace root:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p byteblaster-cli
```

# emwin-cli

CLI application for EMWIN live event streaming and file download workflows. Built on `emwin-protocol` and `emwin-parser`.

## Commands

- `stream`
  - Connect to EMWIN servers and stream events.
  - Optional `--output-dir <PATH>`: assemble completed files from stream events and write them to disk.
- `download <output_dir>`
  - Connect live, assemble completed files, write to `output_dir`.

## Output formats

- `stream` always emits structured `tracing` logs to `stderr` and does not emit JSON payloads.
- `download` emits machine-readable JSON payloads to `stdout`.

Contract:

- command payloads are written to `stdout`
- diagnostics and warnings are written to `stderr`
- diagnostics use canonical `tracing-subscriber` formatting (configure via `RUST_LOG`; ANSI style via `RUST_LOG_STYLE=auto|always|never`)

## Live mode options

For `stream` and `download`:

- `--receiver <qbt|wxwire>` (optional, default `qbt`)
- `--username <EMAIL>` (required)
- `--password <PASSWORD>` (required when `--receiver wxwire`)
- `--server <host:port>` (optional, repeatable or comma-delimited)
- `--server-list-path <PATH>` (optional persisted server list path)
- `--max-events <N>` (optional for `stream`; default `200` for `download`)
- `--idle-timeout-secs <SECONDS>` (default `90`)

Additional `stream` option:

- `--output-dir <PATH>` (optional; writes each completed file assembled from streamed blocks)

If `--server` is omitted, built-in default endpoints are used.
`--server` and `--server-list-path` are only supported for `--receiver qbt`.
When `--server` is provided for QBT live mode, the CLI now pins that explicit server set across
`stream`, `download`, and `server` instead of later replacing it with server-list updates.

## Examples

Live mode:

```bash
cargo run -p emwin-cli -- stream --username you@example.com --max-events 100
cargo run -p emwin-cli -- stream --output-dir ./out --username you@example.com --max-events 100
cargo run -p emwin-cli -- download ./out --username you@example.com --idle-timeout-secs 30
cargo run -p emwin-cli -- stream --receiver wxwire --username you@example.com --password your-pass --max-events 100
cargo run -p emwin-cli -- download ./out --receiver wxwire --username you@example.com --password your-pass --idle-timeout-secs 30
```

## Text product parsing

The CLI leverages `emwin-parser` to parse WMO/AFOS formatted text products:

**Automatic parsing:**
- WMO header extraction (TTAAII, CCCC, DDHHMM, BBB indicators)
- AFOS PIL (Product Identifier Line) parsing
- Text conditioning (SOH/ETX stripping, null byte removal)
- PIL lookup with product type descriptions

**Supported products:**
- Area Forecast Discussions (AFD)
- Severe Thunderstorm Warnings (SVR)
- Tornado Warnings (TOR)
- Flash Flood Warnings (FFW)
- Terminal Aerodrome Forecasts (TAF/FTM)
- And hundreds more meteorological product types

**Parsing handles:**
- BBB indicator classification (Amendment, Correction, Delayed Repeat)
- Missing LDM sequence numbers
- Various text encoding issues
- Correction and amendment flags in WMO headers

## Development checks

From workspace root:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p emwin-cli
```

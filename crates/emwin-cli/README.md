# emwin-cli

CLI application for EMWIN live server workflows. Built on `emwin-protocol` and `emwin-parser`.

## Commands

- `server`
  - Live command.
  - Connects to EMWIN servers, exposes HTTP and SSE endpoints, and retains recent files for `/files` downloads.
  - Optional `--output-dir <PATH>` persists completed payloads asynchronously.

## Output formats

- `server` emits structured `tracing` diagnostics to `stderr` and serves retained payloads over HTTP.

Contract:

- command payloads are written to `stdout`
- diagnostics and warnings are written to `stderr`
- diagnostics use canonical `tracing-subscriber` formatting (configure via `RUST_LOG`; ANSI style via `RUST_LOG_STYLE=auto|always|never`)

## Live mode options

Common live ingest options:

- `--receiver <qbt|wxwire>` (optional, default `qbt`)
- `--username <EMAIL>` (required)
- `--password <PASSWORD>` (required when `--receiver wxwire`)
- `--server <host:port>` (optional, repeatable or comma-delimited)
- `--server-list-path <PATH>` (optional persisted server list path)
- `--post-process-archives <true|false>` (default `true`; extracts the first entry from completed `.ZIP` and `.ZIS` products before parsing and delivery)
- `--persist-queue-capacity <N>` (default `1024`; bounded async persistence queue, evicts oldest queued item when full)
- `--persist-database-url <URL>` (optional; writes normalized metadata into Postgres/PostGIS while still storing payload blobs under `--output-dir`)

Live command: `server`

- `--bind <ADDR>` (default `127.0.0.1:8080`)
- `--cors-origin <ORIGIN>`
- `--max-clients <N>`
- `--stats-interval-secs <SECONDS>`
- `--file-retention-secs <SECONDS>`
- `--max-retained-files <N>`
- `--quiet`
- `--output-dir <PATH>` (optional; writes each matching completed file plus a `.JSON` metadata sidecar)

Persistence behavior when `--output-dir` is set:

- each persisted product writes the payload file and a sibling `.JSON` metadata sidecar
- persistence runs in a background task so live ingest does not wait on filesystem I/O
- when `--persist-database-url` is set, the background task also upserts normalized product metadata and spatial child rows into Postgres/PostGIS
- Postgres metadata failures do not roll back payload or sidecar files already written under `--output-dir`
- if the persistence queue fills, the oldest queued item is evicted so the newest product can still be accepted
- `.ZIP` and `.ZIS` products are extracted before parsing, filtering, and persistence by default; the extracted entry filename replaces the archive filename
- corrupt archives are logged as `Corrupt Zip File Received` and dropped when post-processing is enabled
- sidecar names replace the original extension, for example `AFDBOX.TXT` -> `AFDBOX.JSON`

If `--server` is omitted, built-in default endpoints are used.
`--server` and `--server-list-path` are only supported for `--receiver qbt`.
When `--server` is provided for QBT live mode, the CLI now pins that explicit server set instead
of later replacing it with server-list updates.

## Environment variables and `.env`

The CLI loads `.env` from the current working directory before parsing arguments.
Precedence is:

- CLI args
- process environment
- `.env`
- built-in defaults

Supported environment variables include:

- `EMWIN_TEXT_PREVIEW_CHARS`
- `EMWIN_RECEIVER`
- `EMWIN_USERNAME`
- `EMWIN_PASSWORD`
- `EMWIN_SERVER`
- `EMWIN_SERVER_LIST_PATH`
- `EMWIN_BIND`
- `EMWIN_CORS_ORIGIN`
- `EMWIN_MAX_CLIENTS`
- `EMWIN_STATS_INTERVAL_SECS`
- `EMWIN_FILE_RETENTION_SECS`
- `EMWIN_MAX_RETAINED_FILES`
- `EMWIN_QUIET`
- `EMWIN_POST_PROCESS_ARCHIVES`
- `EMWIN_OUTPUT_DIR`
- `EMWIN_PERSIST_QUEUE_CAPACITY`
- `EMWIN_PERSIST_DATABASE_URL`

Filters are intentionally not configurable through environment variables.

## Examples

Live mode:

```bash
cargo run -p emwin-cli -- server --username you@example.com --bind 127.0.0.1:8080
cargo run -p emwin-cli -- server --username you@example.com --output-dir ./out
cargo run -p emwin-cli -- server --username you@example.com --output-dir ./out --persist-database-url postgres://localhost/emwin
cargo run -p emwin-cli -- server --receiver wxwire --username you@example.com --password your-pass
```

## Server filter examples

When running `server`, `/events` supports parsed-location filters:

- `/events?event=file_complete&lat=41.42&lon=-96.17`
- `/events?event=file_complete&lat=41.42&lon=-96.17&distance_miles=15`

`lat` and `lon` must be provided together. `distance_miles` is optional and defaults to `5.0`.
Matches use parsed `LAT...LON` polygons for containment and parsed `TIME...MOT...LOC`, `UGC`,
and `HVTEC` coordinates for radius checks.

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

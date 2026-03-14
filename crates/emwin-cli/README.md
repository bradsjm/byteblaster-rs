# emwin-cli

CLI application for EMWIN live event streaming workflows. Built on `emwin-protocol` and `emwin-parser`.

## Commands

- `stream`
  - Connect to EMWIN servers and stream events.
  - Optional `--output-dir <PATH>`: assemble completed files from stream events and queue them for asynchronous persistence.

## Output formats

- `stream` always emits structured `tracing` logs to `stderr` and does not emit JSON payloads.

Contract:

- command payloads are written to `stdout`
- diagnostics and warnings are written to `stderr`
- diagnostics use canonical `tracing-subscriber` formatting (configure via `RUST_LOG`; ANSI style via `RUST_LOG_STYLE=auto|always|never`)

## Live mode options

For `stream`:

- `--receiver <qbt|wxwire>` (optional, default `qbt`)
- `--username <EMAIL>` (required)
- `--password <PASSWORD>` (required when `--receiver wxwire`)
- `--server <host:port>` (optional, repeatable or comma-delimited)
- `--server-list-path <PATH>` (optional persisted server list path)
- `--filter <key=value>` (optional, repeatable; reuses `server /events` file filter keys such as `has_issues=true` or `issue_code=invalid_wmo_header`)
- `--max-events <N>` (optional; defaults to unbounded)
- `--idle-timeout-secs <SECONDS>` (default `90`)
- `--post-process-archives <true|false>` (default `true`; extracts the first entry from completed `.ZIP` and `.ZIS` products before parsing and delivery)
- `--persist-queue-capacity <N>` (default `1024`; bounded async persistence queue, evicts oldest queued item when full)

Additional `stream` option:

- `--output-dir <PATH>` (optional; writes each matching completed file plus a `.JSON` metadata sidecar)
- `--persist-queue-capacity <N>` (optional override for the async persistence queue)

Additional `stream --output-dir` behavior:

- each persisted product writes the payload file and a sibling `.JSON` metadata sidecar
- persistence runs in a background task so live ingest does not wait on filesystem I/O
- if the persistence queue fills, the oldest queued item is evicted so the newest product can still be accepted
- `.ZIP` and `.ZIS` products are extracted before parsing, filtering, and persistence by default; the extracted entry filename replaces the archive filename
- corrupt archives are logged as `Corrupt Zip File Received` and dropped when post-processing is enabled
- sidecar names replace the original extension, for example `AFDBOX.TXT` -> `AFDBOX.JSON`

For `server`:

- `--output-dir <PATH>` (optional; persists retained completed files asynchronously using the same payload plus `.JSON` sidecar layout as `stream`)
- `--persist-queue-capacity <N>` (default `1024`; bounded async persistence queue, evicts oldest queued item when full)

If `--server` is omitted, built-in default endpoints are used.
`--server` and `--server-list-path` are only supported for `--receiver qbt`.
When `--server` is provided for QBT live mode, the CLI now pins that explicit server set across
`stream` and `server` instead of later replacing it with server-list updates.

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
- `EMWIN_MAX_EVENTS`
- `EMWIN_IDLE_TIMEOUT_SECS`
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

Filters are intentionally not configurable through environment variables.

## Examples

Live mode:

```bash
cargo run -p emwin-cli -- stream --username you@example.com --max-events 100
cargo run -p emwin-cli -- stream --output-dir ./out --username you@example.com --max-events 100
cargo run -p emwin-cli -- stream --output-dir ./out --username you@example.com --filter has_issues=true
cargo run -p emwin-cli -- stream --output-dir ./out --post-process-archives false --username you@example.com --max-events 100
cargo run -p emwin-cli -- stream --receiver wxwire --username you@example.com --password your-pass --max-events 100
cargo run -p emwin-cli -- stream --output-dir ./out --receiver wxwire --username you@example.com --password your-pass --max-events 100
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

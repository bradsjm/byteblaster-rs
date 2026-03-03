# byteblaster-cli Server Mode API

Version: 1.0
Last Updated: 2026-03-03
Status: Authoritative for `byteblaster-cli server`

## 1. Purpose

This document defines the HTTP/SSE contract exposed by `byteblaster-cli server`.

It covers:

- Available endpoints
- Event stream contract (`/events`)
- Event names and payload shapes
- Field-level definitions

## 2. Start Server Mode

```bash
cargo run -p byteblaster-cli -- server --email you@example.com --bind 127.0.0.1:8080
```

Common options:

- `--bind <ADDR:PORT>`: listen address (default `127.0.0.1:8080`)
- `--max-clients <N>`: max concurrent SSE clients
- `--stats-interval-secs <N>`: periodic stats logging (`0` disables)
- `--file-retention-secs <N>`: retained completed-file TTL
- `--max-retained-files <N>`: retained completed-file capacity
- `--cors-origin "*"|"https://..."`: CORS policy

## 3. Endpoints

## `GET /`

Returns API index with endpoint descriptions.

Response shape:

```json
{
  "service": "byteblaster-cli server",
  "endpoints": [
    {"method":"GET","path":"/","description":"..."},
    {"method":"GET","path":"/events?filter=*.TXT","description":"..."}
  ]
}
```

Fields:

- `service` (string): service identifier
- `endpoints` (array): documented routes
  - `method` (string): HTTP method
  - `path` (string): route path
  - `description` (string): short route description

## `GET /events?filter=<pattern>`

Server-Sent Events stream.

Query params:

- `filter` (optional string): wildcard match (`*`, case-insensitive) against filename.
  - Applies to filename-bearing events (`data_block`, `file_complete`)
  - Non-filename events still pass through (`connected`, `telemetry`, etc.)

SSE framing:

- `id`: monotonically increasing event id
- `event`: event name
- `data`: JSON payload

Example wire form:

```text
id: 42
event: file_complete
data: {"filename":"WARN.txt","size":2140,"download_url":"/files/WARN.txt"}
```

## `GET /files`

Returns retained completed-file metadata.

Response shape:

```json
{
  "files": [
    {
      "filename": "nested/my file.txt",
      "size": 2140,
      "timestamp": 1767488000
    }
  ]
}
```

Fields:

- `files` (array): retained files
  - `filename` (string): logical filename from feed
  - `size` (number): bytes
  - `timestamp` (number): UNIX timestamp seconds when file completed

## `GET /files/*filename`

Downloads retained file content.

Notes:

- `filename` must be URL-encoded when needed
- Returns `404` when file is not retained/expired
- Returns `400` for invalid filename path

Example:

`/files/nested%2Fmy%20file.txt`

## `GET /health`

Response shape:

```json
{
  "status": "ok",
  "connected_clients": 2,
  "retained_files": 17,
  "uptime_secs": 320,
  "upstream_endpoint": "wxmesg.upstateweather.com:2211"
}
```

Fields:

- `status` (string): health status
- `connected_clients` (number): active SSE clients
- `retained_files` (number): retained files currently available
- `uptime_secs` (number): process uptime seconds
- `upstream_endpoint` (string|null): connected upstream endpoint, if connected

## `GET /metrics`

Returns the current telemetry snapshot.

Response shape:

```json
{
  "connection_attempts_total": 0,
  "connection_success_total": 0,
  "connection_fail_total": 0,
  "disconnect_total": 0,
  "watchdog_timeouts_total": 0,
  "watchdog_exception_events_total": 0,
  "auth_logon_sent_total": 0,
  "bytes_in_total": 0,
  "frame_events_total": 0,
  "data_blocks_emitted_total": 0,
  "server_list_updates_total": 0,
  "checksum_mismatch_total": 0,
  "decompression_failed_total": 0,
  "decoder_recovery_events_total": 0,
  "handler_failures_total": 0,
  "backpressure_warning_emitted_total": 0,
  "event_queue_drop_total": 0,
  "telemetry_events_emitted_total": 0
}
```

Field meanings:

- `connection_attempts_total`: outbound connect attempts
- `connection_success_total`: successful connects
- `connection_fail_total`: failed connect attempts
- `disconnect_total`: disconnect events
- `watchdog_timeouts_total`: watchdog no-data timeouts
- `watchdog_exception_events_total`: watchdog exception increments
- `auth_logon_sent_total`: auth/logon writes sent upstream
- `bytes_in_total`: upstream bytes read
- `frame_events_total`: decoded frame events emitted
- `data_blocks_emitted_total`: data block events emitted
- `server_list_updates_total`: server list update events emitted
- `checksum_mismatch_total`: checksum mismatch detections
- `decompression_failed_total`: decompress failures
- `decoder_recovery_events_total`: decoder resync recoveries
- `handler_failures_total`: handler callback failures
- `backpressure_warning_emitted_total`: backpressure warning emissions
- `event_queue_drop_total`: dropped events from queue pressure
- `telemetry_events_emitted_total`: telemetry events emitted

## 4. SSE Event Catalog

All `/events` payloads are JSON in the `data` field.

## `event: connected`

```json
{"endpoint":"wxmesg.upstateweather.com:2211"}
```

Fields:

- `endpoint` (string): current upstream endpoint

## `event: disconnected`

```json
{}
```

No fields.

## `event: data_block`

```json
{
  "type":"data_block",
  "filename":"TAFS31AS.TXT",
  "block_number":1,
  "total_blocks":1,
  "length":104,
  "version":"V1",
  "preview":"SAZS31 ..."
}
```

Fields:

- `type` (string): always `data_block`
- `filename` (string): feed filename
- `block_number` (number): 1-based block index
- `total_blocks` (number): expected blocks for full file
- `length` (number): payload byte length for this block
- `version` (string): protocol version label (for example `V1`, `V2`)
- `preview` (string, optional): text preview when enabled by formatter (not guaranteed)

## `event: server_list`

```json
{
  "type":"server_list",
  "servers":[["host1.example",2211],["host2.example",2211]],
  "sat_servers":[["sat.example",2211]]
}
```

Fields:

- `type` (string): always `server_list`
- `servers` (array): server endpoints as `[host, port]`
- `sat_servers` (array): satellite server endpoints as `[host, port]`

## `event: warning`

Two warning payload forms are currently emitted:

Frame warning form:

```json
{"type":"warning","warning":"..."}
```

SSE lag warning form:

```json
{"message":"client lagged; events dropped","dropped":12,"peer":"127.0.0.1:55555"}
```

Fields:

- Frame form:
  - `type` (string): `warning`
  - `warning` (string): warning detail
- Lag form:
  - `message` (string): warning summary
  - `dropped` (number): dropped event count
  - `peer` (string): client socket address

## `event: file_complete`

```json
{
  "filename":"nested/my file.txt",
  "size":2140,
  "download_url":"/files/nested%2Fmy%20file.txt"
}
```

Fields:

- `filename` (string): completed file name
- `size` (number): file bytes
- `download_url` (string): URL-encoded retrieval path for `GET /files/*filename`

## `event: telemetry`

Payload is the same telemetry object returned by `GET /metrics`.

## `event: error`

```json
{"message":"..."}
```

Fields:

- `message` (string): error message

## `event: unknown`

```json
{"type":"unknown"}
```

Emitted only when an unsupported frame variant is projected to SSE.

## 5. Filtering Rules (`/events?filter=`)

- Matching is wildcard-only with `*`.
- Matching is case-insensitive.
- Filter target is filename only.
- Non-filename events are never filtered out by filename filter.

Examples:

- `*.TXT`
- `WARN*.TXT`
- `*FORECAST*`

## 6. Retention and Availability

- Completed files are retained in memory only.
- Retention is bounded by:
  - max age (`--file-retention-secs`)
  - max entries (`--max-retained-files`)
- When a file expires/evicts, download endpoint returns `404`.

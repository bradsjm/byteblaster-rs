# Persistence Contract

Version: 1.0
Last Updated: 2026-03-14
Status: Authoritative for persisted completed-file metadata and Postgres incident rows

## 1. Purpose

This document defines the persistence contract used by `emwin-cli server` when file
and database persistence are enabled.

It covers:

- the `.JSON` metadata sidecar written next to retained files
- the `product_json` shape stored for immutable product rows
- the synthetic incident-row contract stored in Postgres without a schema migration

This document does not change the live HTTP and SSE payloads exposed by
`GET /files` and `GET /events`. Those endpoints continue to expose parsed `product`
detail and summary payloads derived from the in-memory enriched product.

## 2. Completed-File Metadata Sidecar

When `--output-dir <PATH>` is enabled, each completed file is written with a sibling
`.JSON` sidecar.

The sidecar is intentionally minimal and incident-oriented.

### Sidecar shape

```json
{
  "filename": "A_BCUS53KOAX010200_C_KWIN_20260301020001_881239-2-FFWOAXNE.TXT",
  "size": 2140,
  "timestamp_utc": 1772320800,
  "incidents": [
    {
      "office": "KOAX",
      "phenomena": "FF",
      "significance": "W",
      "etn": 123,
      "latest_vtec_action": "NEW",
      "current_status": "active",
      "issued_at": "2026-03-01T02:00:00Z",
      "start_utc": "2026-03-01T02:00:00Z",
      "end_utc": "2026-03-01T04:00:00Z"
    }
  ]
}
```

### Sidecar field rules

- `filename`: logical filename from the feed
- `size`: retained payload size in bytes
- `timestamp_utc`: UNIX timestamp seconds from the completed EMWIN `/FD` metadata
- `incidents`: deduplicated operational VTEC projections for the product

Each `incidents[]` entry uses the key `(office, phenomena, significance, etn)` and
contains:

- `office`: VTEC office, such as `KOAX`
- `phenomena`: VTEC phenomena code, such as `FF`
- `significance`: VTEC significance code, such as `W`
- `etn`: event tracking number
- `latest_vtec_action`: latest action observed for that incident within the product
- `current_status`: one of `active`, `cancelled`, `expired`, `upgraded`, or `null`
- `issued_at`: current product timestamp mapped from `timestamp_utc`
- `start_utc`: earliest operational VTEC begin time seen for the incident in the product
- `end_utc`: latest operational VTEC end time seen for the incident in the product

### Incident derivation rules

- Only operational VTEC codes (`status == "O"`) are persisted into `incidents`.
- Multiple VTEC codes in the same product collapse into one incident entry per
  `(office, phenomena, significance, etn)`.
- `current_status` is derived from `latest_vtec_action` using this mapping:
  - `NEW`, `CON`, `EXT`, `EXA`, `EXB` -> `active`
  - `CAN` -> `cancelled`
  - `EXP` -> `expired`
  - `UPG` -> `upgraded`
  - `COR`, `ROU` -> `null` so persistence can preserve the prior incident status
  - unknown actions default to `active`

The sidecar does not serialize the full parsed product body, parser origin, or the
live API `product` payload shape.

## 3. Immutable Product Rows

When `--persist-database-url <URL>` is enabled, completed products are still
persisted as immutable rows in `products` plus their normal child rows such as
`product_vtec`, `product_ugc_areas`, `product_hvtec`, and `product_polygons`.

The immutable product row contract changed in one important way:

- `products.product_json` now stores the serialized completed-file metadata sidecar
  shape described above

That means the persisted JSON for immutable product rows now contains only:

- `filename`
- `size`
- `timestamp_utc`
- `incidents`

The full parsed product detail is no longer stored in `product_json`.

## 4. Synthetic Incident Rows

Mutable incident state is persisted without a database migration by reusing the
existing `products` and `product_vtec` tables.

Each incident is represented by a synthetic row in `products` keyed by:

- `filename = "__incident__/{office}/{phenomena}/{significance}/{etn}"`
- `source_timestamp_utc = 0`

This produces one stable upsert target per incident key.

### Synthetic row identity rules

- `source = "incident"`
- `family = "incident"`
- `container = "incident"`
- `title = "Current incident state"`
- `office_code` and `cccc` are set from the incident office
- payload and metadata blob references point at the latest product that updated the incident
- non-VTEC child tables are intentionally left empty for synthetic incident rows

### Synthetic incident JSON shape

Synthetic incident rows store current state in `products.product_json`.

```json
{
  "office": "KOAX",
  "phenomena": "FF",
  "significance": "W",
  "etn": 123,
  "current_status": "active",
  "latest_vtec_action": "NEW",
  "issued_at": "2026-03-01T02:00:00Z",
  "start_utc": "2026-03-01T02:00:00Z",
  "end_utc": "2026-03-01T04:00:00Z",
  "first_product_id": 1001,
  "latest_product_id": 1001,
  "latest_product_timestamp_utc": 1772320800,
  "last_updated_at": "2026-03-01T02:00:01Z"
}
```

### Synthetic incident field rules

- `office`, `phenomena`, `significance`, `etn`: incident identity
- `current_status`: current lifecycle status; may be preserved from the prior row
  when the incoming action is `COR` or `ROU`
- `latest_vtec_action`: latest accepted action for the incident
- `issued_at`: source product timestamp represented as `DateTime<Utc>`
- `start_utc`, `end_utc`: current incident window carried from the accepted update
- `first_product_id`: immutable product row id that first created the incident
- `latest_product_id`: immutable product row id from the latest accepted update
- `latest_product_timestamp_utc`: stale-write guard used during upsert
- `last_updated_at`: server-side timestamp from the accepted update transaction

### Synthetic incident child rows

Each synthetic incident row also owns exactly one current `product_vtec` child row
representing the accepted incident snapshot.

The synthetic `product_vtec` row uses:

- the incident identity fields (`office`, `phenomena`, `significance`, `etn`)
- `action = latest_vtec_action`
- `begin_utc = start_utc`
- `end_utc = end_utc`
- `status = "O"` for active or status-preserving updates
- `status = "X"` for terminal incident states (`cancelled`, `expired`, `upgraded`)

No synthetic rows are inserted into `product_ugc_areas`, `product_hvtec`,
`product_polygons`, `product_time_mot_loc`, `product_wind_hail`, or
`product_search_points`.

## 5. Update Rules

Incident updates are applied in application code during the same transaction that
persists the immutable product row.

Rules:

- immutable product rows are still written first
- synthetic incident rows are prepared from the same completed product metadata
- incident upserts target the stable synthetic key `(filename, source_timestamp_utc)`
- `first_product_id` is preserved once set
- `current_status` is preserved when the incoming update carries `null`
- stale updates are rejected when the existing `latest_product_timestamp_utc` is newer

The stale-write rule prevents an older product from overwriting a newer incident state.

## 6. Compatibility Notes

- Live server payloads are unchanged: `GET /files` and `file_complete` SSE events still
  expose parsed `product` metadata.
- Persisted sidecars and immutable `product_json` are intentionally smaller and no longer
  contain full parsed product detail.
- Consumers that previously treated persisted `product_json` as detail v2 metadata must
  switch to the new sidecar contract or read normalized columns and child tables instead.

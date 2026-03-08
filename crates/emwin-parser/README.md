# emwin-parser

WMO/AFOS text product parsing library for weather and aviation meteorological products.

## Overview

This crate provides parsing, enrichment, and lookup capabilities for WMO (World Meteorological Organization) and AFOS (Automation of Field Operations and Services) formatted text products commonly used in meteorological broadcasting systems.

## Features

- **WMO header parsing**: Extracts TTAAII, CCCC, DDHHMM, and optional BBB indicators
- **AFOS PIL extraction**: Parses the Product Identifier Line (PIL) with robust error handling
- **Text conditioning**: Handles SOH/ETX control characters, null bytes, missing LDM sequences
- **Structured bulletin enrichment**: Detects and decodes FD winds aloft, PIREPs, SIGMETs, METAR collectives, TAF bulletins, and Wallops DCP telemetry bulletins
- **PIL lookup**: Built-in product type descriptions for common meteorological products
- **UGC geography lookup**: Built-in county and zone name catalogs keyed by canonical UGC codes
- **Header enrichment**: Classifies BBB indicators (Amendment, Correction, Delayed Repeat)
- **Zero-copy parsing**: Efficient byte-based parsing with minimal allocations

## Installation

Add this crate to your `Cargo.toml`:

```toml
[dependencies]
emwin-parser = { git = "https://github.com/bradsjm/emwin-rs", tag = "v0.2.0", package = "emwin-parser" }
```

For local development:

```toml
[dependencies]
emwin-parser = { path = "../emwin-rs/crates/emwin-parser" }
```

## Usage

### Basic Header Parsing

Parse a WMO/AFOS text product header:

```rust
use emwin_parser::{parse_text_product, TextProductHeader};

let raw_text = b"000 \nFXUS61 KBOX 022101\nAFDBOX\nAREA FORECAST DISCUSSION\n";
let header: TextProductHeader = parse_text_product(raw_text)?;

println!("AFOS PIL: {}", header.afos);      // AFDBOX
println!("Station: {}", header.cccc);      // KBOX
println!("Time: {}", header.ddhhmm);       // 022101
println!("Type: {}", header.ttaaii);       // FXUS61
println!("Correction: {:?}", header.bbb);  // None
```

### Header Enrichment

Add semantic information to parsed headers:

```rust
use emwin_parser::{parse_text_product, enrich_header};

let header = parse_text_product(raw_text)?;
let enriched = enrich_header(&header);

if let Some(pil_desc) = enriched.pil_description {
    println!("Product type: {}", pil_desc);  // "Area Forecast Discussion"
}

if let Some(bbb_kind) = enriched.bbb_kind {
    println!("This is a {:?}", bbb_kind);  // Amendment, Correction, etc.
}
```

### Product Enrichment

`enrich_product()` routes supported bulletins into structured families when the
body shape is deterministic enough to parse safely.

Current structured families include:

- FD winds and temperatures aloft bulletins
- PIREP bulletins
- SIGMET bulletins
- METAR collectives
- TAF bulletins
- GOES DCP telemetry bulletins

### PIL Lookup

Look up product type descriptions by PIL prefix:

```rust
use emwin_parser::pil_description;

assert_eq!(pil_description("AFD"), Some("Area Forecast Discussion"));
assert_eq!(pil_description("SVR"), Some("Severe Thunderstorm Warning"));
assert_eq!(pil_description("TOR"), Some("Tornado Warning"));
assert_eq!(pil_description("ZZZ"), None);  // Unknown product type
```

### Text Conditioning

The parser handles various text encoding issues automatically:

```rust
// SOH/ETX control characters are stripped
let with_controls = b"\x01123\n000 \nFXUS61 KBOX 022101\nAFDBOX\nbody\x03";
let header = parse_text_product(with_controls)?;

// Null bytes are removed
let with_nulls = b"000 \nFXUS61 KBOX 022101\nAFD\0BOX\nbody";
let header = parse_text_product(with_nulls)?;

// Missing LDM sequence is auto-inserted
let missing_ldm = b"FXUS61 KBOX 022101\nAFDBOX\nbody\n";
let header = parse_text_product(missing_ldm)?;
```

## API Reference

### `parse_text_product`

Parses a WMO/AFOS text product header from raw bytes.

```rust
pub fn parse_text_product(bytes: &[u8]) -> Result<TextProductHeader, ParserError>
```

**Returns**: `TextProductHeader` containing:
- `ttaaii`: WMO product type indicator (6 chars, normalized from 4 to "00")
- `cccc`: 4-letter ICAO station code
- `ddhhmm`: Day and time (UTC)
- `bbb`: Optional BBB indicator (CORrection, AMEndment, RR, etc.)
- `afos`: Product Identifier Line (6 chars)

**Errors**:
- `EmptyInput`: Text is empty after conditioning
- `MissingWmoLine`: No WMO header line found
- `InvalidWmoHeader`: WMO header format is invalid
- `MissingAfosLine`: No AFOS line found
- `MissingAfos`: Cannot parse AFOS PIL from line

### `enrich_header`

Enriches a parsed header with semantic information.

```rust
pub fn enrich_header(header: &TextProductHeader) -> TextProductEnrichment<'_>
```

**Returns**: `TextProductEnrichment` containing:
- `pil_nnn`: First 3 characters of AFOS PIL
- `pil_description`: Human-readable product type description (if known)
- `flags`: Product capability flags from the PIL catalog (if known)
- `bbb_kind`: Classified BBB indicator (Amendment, Correction, DelayedRepeat, Other)

### `pil_description`

Looks up a product type description by PIL prefix.

```rust
pub fn pil_description(nnn: &str) -> Option<&'static str>
```

**Returns**: Description string if the PIL prefix is known, `None` otherwise.

### UGC Lookup

Look up county and zone metadata by canonical UGC code:

```rust
use emwin_parser::{ugc_county_entry, ugc_zone_entry};

assert_eq!(ugc_county_entry("ALC001").map(|entry| entry.name), Some("Autauga"));
assert_eq!(
    ugc_zone_entry("AKZ317").map(|entry| entry.name),
    Some("City and Borough of Yakutat")
);
```

Parsed UGC sections now emit compact enriched area objects:

```json
{
  "counties": {
    "AL": [
      { "id": 1, "name": "Autauga", "lat": 32.5349, "lon": -86.6428 },
      { "id": 3, "name": "Baldwin", "lat": 30.7273, "lon": -87.7169 },
      { "id": 5, "name": "Barbour", "lat": 31.8696, "lon": -85.3932 }
    ]
  }
}
```

### WMO Office Lookup

Look up WMO office metadata by 3-letter office code or 4-letter `CCCC`:

```rust
use emwin_parser::wmo_office_entry;

assert_eq!(
    wmo_office_entry("LWX").map(|entry| entry.office_name),
    Some("WFO Baltimore/Washington")
);
assert_eq!(
    wmo_office_entry("KLWX").map(|entry| entry.city),
    Some("Baltimore/Washington")
);
```

## Error Handling

All parsing operations return `Result` types with typed errors:

```rust
use emwin_parser::{parse_text_product, ParserError};

match parse_text_product(raw_bytes) {
    Ok(header) => println!("Parsed: {}", header.afos),
    Err(ParserError::InvalidWmoHeader { line }) => {
        eprintln!("Invalid WMO header: {}", line);
    }
    Err(ParserError::MissingAfos { line }) => {
        eprintln!("Cannot parse AFOS from: {}", line);
    }
    Err(e) => eprintln!("Parse error: {}", e),
}
```

## PIL Metadata

The built-in PIL lookup table includes:

- `PIL_ENTRY_COUNT`: Number of product types in the lookup table
- `PIL_GENERATED_AT_UTC`: Timestamp when the PIL table was generated
- `pil_catalog_entry()`: Full metadata including `wmo_prefix`, `title`, `ugc`, `vtec`, `cz`, `latlong`, `time_mot_loc`, `wind_hail`, and `hvtec`
- `enrich_header()`: Surfaces those catalog flags for parser decisions and header enrichment

The built-in UGC lookup tables include:

- `UGC_COUNTY_ENTRY_COUNT` and `UGC_ZONE_ENTRY_COUNT`: Number of generated county and zone records
- `UGC_GENERATED_AT_UTC`: Timestamp when the UGC tables were generated
- `UGC_COUNTY_SOURCE_PATH` and `UGC_ZONE_SOURCE_PATH`: Source JSON files for the generated tables
- `ugc_county_entry()` and `ugc_zone_entry()`: Full metadata including `code`, `name`, `latitude`, and `longitude`

The built-in WMO office lookup table includes:

- `WMO_OFFICE_ENTRY_COUNT`: Number of generated office records
- `WMO_OFFICE_GENERATED_AT_UTC`: Timestamp when the office table was generated
- `WMO_OFFICE_SOURCE_PATH`: Source JSON file for the generated table
- `wmo_office_entry()`: Full metadata including `code`, `office_name`, `city`, and `state`
  Serialized product output includes `code`, `city`, and `state`; `office_name` remains available from the lookup API but is omitted from serialized payloads.

## Supported Product Types

The PIL lookup includes common meteorological products:

- **AFD**: Area Forecast Discussion
- **FFW**: Flash Flood Warning
- **SVR**: Severe Thunderstorm Warning
- **TOR**: Tornado Warning
- **RWT**: Tornado Watch
- **WSW**: Winter Storm Warning
- **FTM**: Terminal Aerodrome Forecast (TAF)
- And hundreds more...

## Testing

Run tests:

```bash
cargo test -p emwin-parser
```

Run specific test:

```bash
cargo test -p emwin-parser wmo_header_variations_parse
```

## Integration

This crate is used by `emwin-cli` for parsing weather products received via QBT or Weather Wire protocols.

See `emwin-cli/src/cmd/file_pipeline.rs` for usage examples in a real-world application.

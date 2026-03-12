# `tests/` Organization

This directory holds integration tests for `emwin-parser`.

Use the layout here to keep real-product coverage maintainable as the corpus grows.

## Test File Roles

- `body_smoke.rs`
  - Small end-to-end sanity checks for generic body enrichment behavior.
- `generic_product_corpus.rs`
  - Real-product corpus coverage for generic body extraction and QC behavior.
- `specialized_product_corpus.rs`
  - Real-product corpus coverage for specialized bulletin families.
- `wmo_product_corpus.rs`
  - Real-product corpus coverage for WMO-only bulletin families.
- `ugc_product_parity.rs`
  - Focused parity/regression coverage for repeated-UGC behavior.
- `vtec_hvtec_parity.rs`
  - Focused parser parity coverage for VTEC/HVTEC edge cases.

Use `*_product_corpus.rs` for tracked real bulletin fixtures that exercise end-to-end enrichment behavior.

Use `*_parity.rs` or `*_regression.rs` only when the test is narrowly scoped to a specific parser contract or a specific previously broken bug.

## Fixture Layout

Real-product fixtures live under:

- `fixtures/products/generic/<product_family>/`
- `fixtures/products/specialized/<product_family>/`
- `fixtures/products/wmo/<product_family>/`

Organize fixtures first by parser domain, then by product family.

Examples:

- `fixtures/products/generic/marine_weather_message/MWWBUFNY.TXT`
- `fixtures/products/specialized/lsr/202603100015-KBMX-NWUS54-LSRBMX.txt`
- `fixtures/products/wmo/metar_collective/SABZ31.TXT`

Do not create catch-all buckets such as `misc`, `samples`, `tmp`, `sidecars`, or `other`.

If a fixture represents a product family that does not yet have a directory, add a new family directory with the product-family name spelled out in lowercase snake case.

## Fixture Source Rules

- Prefer real products from Iowa Mesonet `https://mesonet.agron.iastate.edu/api/1/nwstext/{product_id}`.
- Use the documented Iowa Mesonet list endpoints to discover `product_id` values for AFOS-backed products.
- Use `scripts/fetch_nwstext_fixture.py` to retrieve AFOS-backed fixtures from Mesonet and store the raw archived bulletin text.
- Use `pyIEM` only as a sanity-check source or to help locate examples when Mesonet discovery is awkward.
- If a real WMO-only product cannot be discovered from the AFOS list endpoint, a tracked real bulletin text fixture is acceptable, but explain that provenance in the test file.

Integration tests must be reproducible from the repository alone.

## Naming Rules

- Keep the original bulletin filename when it is already stable and meaningful.
- Prefer Mesonet `product_id` filenames for archived AFOS fixtures when those names are already used elsewhere in the corpus.
- Keep extensions as delivered by the fixture source unless there is a strong reason to normalize.

Do not rename fixtures to encode bug history.

## When To Add A Corpus Fixture

Add a real-product corpus fixture when:

- a parser change needs real-world proof
- a product family has no current end-to-end integration coverage
- a real formatting variant should remain supported
- a negative real-world case should continue to emit a specific issue

Do not add large numbers of redundant fixtures that exercise the same behavior with no new coverage value.

## Maintenance Rules

- Keep unit tests under `src/**` inline and module-local.
- Keep real bulletin fixtures under `tests/fixtures/**` and consume them only from integration tests under `tests/**`.
- When behavior changes, update the relevant corpus tests and fixture provenance comments in the same change.
- If a new corpus fixture exposes a broader product-family need, add it to the correct family directory instead of creating a one-off bucket.
- If a suite becomes too large, split it by parser domain or product family, not by the date the tests were added.

## Validation

Run from the repository root:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p emwin-parser
cargo test --workspace
```

# `tests/` Organization

This directory holds integration tests for `emwin-parser`.

The corpus is intentionally fixture-heavy. The goal is to exercise as many real
formatting variants as practical for each supported parser family, not to keep
only a few representative examples.

## Test File Roles

- `body_smoke.rs`
  - Small end-to-end sanity checks for generic body enrichment behavior.
- `corpus_*.rs`
  - Family-scoped or domain-scoped corpus suites over tracked real bulletin fixtures.
- `*_parity.rs`
  - Focused parser parity/regression coverage for narrow contracts and previously broken behavior.

Use corpus suites for real-fixture routing, assembly, and output-shape coverage.
Use parity/regression files only when a narrow parser contract is clearer with a
small targeted sample.

## Fixture Layout

Real-product fixtures live under:

- `fixtures/products/generic/<product_family>/`
- `fixtures/products/specialized/<product_family>/`
- `fixtures/products/wmo/<product_family>/`

Organize fixtures first by parser domain, then by product family.

Examples:

- `fixtures/products/generic/marine_weather_message/MWWAJK.txt`
- `fixtures/products/specialized/pirep/PIREPS-PIREP.txt`
- `fixtures/products/wmo/metar_collective/METAR-collective.txt`

Do not create catch-all buckets such as `misc`, `samples`, `tmp`, `sidecars`, or `other`.

## Fixture Sources

- Existing AFOS-backed fixtures may still come from Iowa Mesonet `https://mesonet.agron.iastate.edu/api/1/nwstext/{product_id}`.
- The exhaustive variation corpus is imported from `akrherz/pyIEM` `data/product_examples`.
- Use `scripts/import_pyiem_product_examples.py <pyiem_checkout>` to import or refresh the pyIEM-derived corpus.
- Use `--dry-run` first when changing the manifest or auditing fixture drift.
- Integration tests must remain reproducible from the repository alone after import.

## Naming Rules

- Preserve the upstream filename when it is already stable and meaningful.
- When importing from a numbered or collision-prone pyIEM subdirectory, prefix the filename with the source directory name.
- Do not rename fixtures to encode bug history.

## Corpus Expectations

- Every fixture directory must have a corpus suite that enumerates every file in that directory.
- Default assertions should cover routing, body kind, parsed artifact kind, and minimal structural validity.
- Explicit allowlists should isolate malformed or intentionally sparse real-world fixtures instead of weakening the whole suite.
- Negative fixtures remain in the corpus; they should assert the expected issues or degraded shape explicitly.

## Maintenance Rules

- Keep unit tests under `src/**` inline and module-local.
- Keep real bulletin fixtures under `tests/fixtures/**` and consume them only from integration tests under `tests/**`.
- When behavior changes, update the relevant corpus suites and provenance comments in the same change.
- If a family grows too large, split by parser family or body model, not by the date the tests were added.

## Validation

Run from the repository root:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p emwin-parser
cargo test --workspace
```

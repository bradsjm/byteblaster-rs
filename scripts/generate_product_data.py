#!/usr/bin/env python3

import json
from datetime import datetime, timezone
from pathlib import Path
from typing import TypedDict

REPO_ROOT = Path(__file__).resolve().parent.parent
CATALOG_PATH = REPO_ROOT / "crates/emwin-parser/data/text_product_catalog.json"
OUTPUT_PATH = REPO_ROOT / "crates/emwin-parser/src/data/generated_text_products.rs"


class CatalogEntry(TypedDict):
    wmo_prefix: str
    title: str
    routing: str
    body_behavior: str
    extractors: list[str]


EXTRACTOR_ORDER = [
    "vtec_events",
    "ugc",
    "latlon",
    "time_mot_loc",
    "wind_hail",
]

EXTRACTOR_VARIANTS = {
    "vtec_events": "BodyExtractorId::VtecEvents",
    "ugc": "BodyExtractorId::Ugc",
    "latlon": "BodyExtractorId::LatLon",
    "time_mot_loc": "BodyExtractorId::TimeMotLoc",
    "wind_hail": "BodyExtractorId::WindHail",
}

ROUTING_VARIANTS = {
    "generic": "TextProductRouting::Generic",
    "fd": "TextProductRouting::Fd",
    "pirep": "TextProductRouting::Pirep",
    "sigmet": "TextProductRouting::Sigmet",
    "lsr": "TextProductRouting::Lsr",
    "cwa": "TextProductRouting::Cwa",
    "wwp": "TextProductRouting::Wwp",
    "cf6": "TextProductRouting::Cf6",
    "dsm": "TextProductRouting::Dsm",
    "hml": "TextProductRouting::Hml",
    "mos": "TextProductRouting::Mos",
}

BODY_BEHAVIOR_VARIANTS = {
    "never": "TextProductBodyBehavior::Never",
    "catalog": "TextProductBodyBehavior::Catalog",
}


def require_extractors(entry: dict[str, object], pil: str) -> list[str]:
    value = entry.get("extractors")
    if not isinstance(value, list):
        raise SystemExit(f"catalog entry {pil} must define extractor list")

    normalized: list[str] = []
    for raw in value:
        if not isinstance(raw, str):
            raise SystemExit(f"catalog entry {pil} extractor names must be strings")
        name = raw.strip()
        if name not in EXTRACTOR_VARIANTS:
            raise SystemExit(f"catalog entry {pil} has unknown extractor {raw!r}")
        normalized.append(name)

    ordered = [name for name in EXTRACTOR_ORDER if name in normalized]
    if len(ordered) != len(normalized):
        raise SystemExit(f"catalog entry {pil} has duplicate extractors")
    if normalized != ordered:
        raise SystemExit(
            f"catalog entry {pil} extractors must be unique and in canonical order"
        )
    return ordered


def require_routing(entry: dict[str, object], pil: str) -> str:
    value = str(entry.get("routing", "")).strip().lower()
    if value not in ROUTING_VARIANTS:
        raise SystemExit(f"catalog entry {pil} has unknown routing {value!r}")
    return value


def require_body_behavior(entry: dict[str, object], pil: str) -> str:
    value = str(entry.get("body_behavior", "")).strip().lower()
    if value not in BODY_BEHAVIOR_VARIANTS:
        raise SystemExit(f"catalog entry {pil} has unknown body_behavior {value!r}")
    return value


def load_catalog() -> list[tuple[str, CatalogEntry]]:
    raw = json.loads(CATALOG_PATH.read_text(encoding="utf-8"))
    if not isinstance(raw, dict):
        raise SystemExit("product catalog JSON must be an object keyed by PIL")

    entries: dict[str, CatalogEntry] = {}
    for pil_key, entry in raw.items():
        if not isinstance(pil_key, str):
            raise SystemExit("product catalog keys must be strings")
        if not isinstance(entry, dict):
            raise SystemExit(f"catalog entry {pil_key} must be an object")

        pil = pil_key.strip().upper()
        wmo_prefix = str(entry.get("wmo_prefix", "")).strip().upper()
        title = str(entry.get("title", "")).strip()
        if len(pil) != 3 or not pil.isalnum():
            raise SystemExit(f"invalid PIL key: {pil_key}")
        if len(wmo_prefix) != 2 or not wmo_prefix.isalnum():
            raise SystemExit(f"invalid wmo_prefix for {pil}: {wmo_prefix!r}")
        if not title:
            raise SystemExit(f"missing title for {pil}")

        routing = require_routing(entry, pil)
        body_behavior = require_body_behavior(entry, pil)
        extractors = require_extractors(entry, pil)
        if body_behavior == "catalog" and not extractors:
            raise SystemExit(
                f"catalog entry {pil} with body_behavior='catalog' must define extractors"
            )
        if body_behavior == "never" and extractors:
            raise SystemExit(
                f"catalog entry {pil} with body_behavior='never' must not define extractors"
            )

        normalized: CatalogEntry = {
            "wmo_prefix": wmo_prefix,
            "title": title,
            "routing": routing,
            "body_behavior": body_behavior,
            "extractors": extractors,
        }

        previous = entries.get(pil)
        if previous and previous != normalized:
            raise SystemExit(
                f"conflicting catalog entry for {pil}: {previous} vs {normalized}"
            )
        entries[pil] = normalized

    return sorted(entries.items())


def rust_string(value: str) -> str:
    escaped = value.replace("\\", "\\\\").replace('"', '\\"')
    return f'"{escaped}"'


def rust_extractors(values: list[str]) -> str:
    if not values:
        return "&[]"
    variants = ", ".join(EXTRACTOR_VARIANTS[value] for value in values)
    return f"&[{variants}]"


def rust_routing(value: str) -> str:
    return ROUTING_VARIANTS[value]


def rust_body_behavior(value: str) -> str:
    return BODY_BEHAVIOR_VARIANTS[value]


def write_output(catalog: list[tuple[str, CatalogEntry]]) -> None:
    generated_at = (
        datetime.now(timezone.utc)
        .replace(microsecond=0)
        .isoformat()
        .replace("+00:00", "Z")
    )

    lines = [
        "// @generated by scripts/generate_product_data.py",
        "// Source file:",
        "// - crates/emwin-parser/data/text_product_catalog.json",
        "// Do not edit manually.",
        "",
        "use crate::body::BodyExtractorId;",
        "use super::{TextProductBodyBehavior, TextProductCatalogEntry, TextProductRouting};",
        "",
        f"pub const TEXT_PRODUCT_GENERATED_AT_UTC: &str = {rust_string(generated_at)};",
        f"pub const TEXT_PRODUCT_ENTRY_COUNT: usize = {len(catalog)};",
        "",
        "pub static TEXT_PRODUCT_CATALOG: [TextProductCatalogEntry; TEXT_PRODUCT_ENTRY_COUNT] = [",
    ]

    for pil, entry in catalog:
        lines.append(
            "    TextProductCatalogEntry { "
            f"pil: {rust_string(pil)}, "
            f"wmo_prefix: {rust_string(entry['wmo_prefix'])}, "
            f"title: {rust_string(entry['title'])}, "
            f"routing: {rust_routing(entry['routing'])}, "
            f"body_behavior: {rust_body_behavior(entry['body_behavior'])}, "
            f"extractors: {rust_extractors(entry['extractors'])} }},"
        )

    lines.extend(
        [
            "];",
            "",
        ]
    )

    OUTPUT_PATH.write_text("\n".join(lines), encoding="utf-8")


def main() -> None:
    catalog = load_catalog()
    write_output(catalog)


if __name__ == "__main__":
    main()

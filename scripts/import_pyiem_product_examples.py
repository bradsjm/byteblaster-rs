#!/usr/bin/env python3

from __future__ import annotations

import argparse
import filecmp
import json
import shutil
import sys
from dataclasses import dataclass
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_MANIFEST_PATH = Path(__file__).with_name("pyiem_product_examples_manifest.json")
SOURCE_ROOT_RELATIVE = Path("data/product_examples")
FIXTURE_ROOT = REPO_ROOT / "crates/emwin-parser/tests/fixtures/products"


@dataclass(frozen=True)
class ManifestEntry:
    source: str
    destination_dir: str
    target_name: str | None = None


@dataclass(frozen=True)
class PlannedCopy:
    entry: ManifestEntry
    source_path: Path
    destination_path: Path
    action: str


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Import selected pyIEM product_examples fixtures into the "
            "emwin-parser test fixture tree."
        )
    )
    parser.add_argument(
        "pyiem_checkout",
        help="Path to a local akrherz/pyIEM checkout.",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Report planned changes without writing files.",
    )
    parser.add_argument(
        "--manifest",
        default=str(DEFAULT_MANIFEST_PATH),
        help=(
            "Path to the JSON manifest that defines source-to-destination mapping. "
            f"Defaults to {DEFAULT_MANIFEST_PATH}."
        ),
    )
    return parser.parse_args()


def _resolve_within(base: Path, relative_path: Path) -> Path:
    candidate = (base / relative_path).resolve()
    try:
        candidate.relative_to(base.resolve())
    except ValueError as err:
        raise SystemExit(f"path escapes base directory: {relative_path}") from err
    return candidate


def _validate_relative_path(raw_value: object, field_name: str) -> Path:
    if not isinstance(raw_value, str):
        raise SystemExit(f"manifest field {field_name!r} must be a string")
    relative_path = Path(raw_value)
    if relative_path.is_absolute():
        raise SystemExit(f"manifest field {field_name!r} must be relative: {raw_value}")
    return relative_path


def _validate_target_name(raw_value: object) -> str:
    if not isinstance(raw_value, str):
        raise SystemExit("manifest field 'target_name' must be a string when provided")
    target_name = Path(raw_value)
    if target_name.name != raw_value or raw_value in {"", ".", ".."}:
        raise SystemExit(
            "manifest field 'target_name' must be a plain filename without path segments"
        )
    return raw_value


def load_manifest(manifest_path: Path) -> list[ManifestEntry]:
    try:
        raw_manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except FileNotFoundError as err:
        raise SystemExit(f"manifest not found: {manifest_path}") from err
    except json.JSONDecodeError as err:
        raise SystemExit(f"invalid JSON in manifest {manifest_path}: {err}") from err

    if not isinstance(raw_manifest, list):
        raise SystemExit(f"manifest root must be a JSON array: {manifest_path}")

    entries: list[ManifestEntry] = []
    for index, raw_entry in enumerate(raw_manifest, start=1):
        if not isinstance(raw_entry, dict):
            raise SystemExit(f"manifest entry #{index} must be an object")

        source = str(_validate_relative_path(raw_entry.get("source"), "source"))
        destination_dir = str(
            _validate_relative_path(raw_entry.get("destination_dir"), "destination_dir")
        )

        target_name: str | None = None
        if "target_name" in raw_entry and raw_entry["target_name"] is not None:
            target_name = _validate_target_name(raw_entry["target_name"])

        entries.append(
            ManifestEntry(
                source=source,
                destination_dir=destination_dir,
                target_name=target_name,
            )
        )

    if not entries:
        raise SystemExit(f"manifest is empty: {manifest_path}")

    return entries


def plan_copies(pyiem_checkout: Path, entries: list[ManifestEntry]) -> list[PlannedCopy]:
    source_root = _resolve_within(pyiem_checkout.resolve(), SOURCE_ROOT_RELATIVE)
    if not source_root.is_dir():
        raise SystemExit(f"pyIEM product_examples directory not found: {source_root}")

    planned: list[PlannedCopy] = []
    destinations: dict[Path, ManifestEntry] = {}

    for entry in entries:
        source_path = _resolve_within(source_root, Path(entry.source))
        destination_dir = _resolve_within(FIXTURE_ROOT, Path(entry.destination_dir))
        target_name = entry.target_name or Path(entry.source).name
        destination_path = _resolve_within(destination_dir, Path(target_name))

        previous = destinations.get(destination_path)
        if previous is not None:
            raise SystemExit(
                "manifest resolves multiple sources to the same destination: "
                f"{previous.source} and {entry.source} -> {destination_path}. "
                "Set an explicit target_name to resolve the collision."
            )
        destinations[destination_path] = entry

        if not source_path.is_file():
            action = "missing-source"
        elif destination_path.exists():
            action = (
                "unchanged"
                if filecmp.cmp(source_path, destination_path, shallow=False)
                else "update"
            )
        else:
            action = "create"

        planned.append(
            PlannedCopy(
                entry=entry,
                source_path=source_path,
                destination_path=destination_path,
                action=action,
            )
        )

    return planned


def print_plan(planned: list[PlannedCopy], *, dry_run: bool, source_root: Path) -> None:
    mode = "dry-run" if dry_run else "apply"
    print(f"Mode: {mode}")
    print(f"Source root: {source_root}")
    print(f"Destination root: {FIXTURE_ROOT}")
    print("")
    for item in planned:
        label = item.action.upper()
        print(
            f"{label:14} {item.source_path} -> {item.destination_path}"
        )


def apply_plan(planned: list[PlannedCopy]) -> None:
    for item in planned:
        if item.action == "missing-source":
            continue
        if item.action == "unchanged":
            continue

        item.destination_path.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(item.source_path, item.destination_path)


def print_summary(planned: list[PlannedCopy]) -> None:
    counts = {
        "create": 0,
        "update": 0,
        "unchanged": 0,
        "missing-source": 0,
    }
    for item in planned:
        counts[item.action] += 1

    print("")
    print("Summary:")
    print(f"  total entries: {len(planned)}")
    print(f"  create: {counts['create']}")
    print(f"  update: {counts['update']}")
    print(f"  unchanged: {counts['unchanged']}")
    print(f"  missing source: {counts['missing-source']}")


def main() -> int:
    args = parse_args()
    manifest_path = Path(args.manifest).resolve()
    pyiem_checkout = Path(args.pyiem_checkout).resolve()
    source_root = _resolve_within(pyiem_checkout, SOURCE_ROOT_RELATIVE)

    entries = load_manifest(manifest_path)
    planned = plan_copies(pyiem_checkout, entries)
    print_plan(planned, dry_run=args.dry_run, source_root=source_root)

    has_missing_sources = any(item.action == "missing-source" for item in planned)
    if not args.dry_run and not has_missing_sources:
        apply_plan(planned)
    elif not args.dry_run and has_missing_sources:
        print("")
        print("Apply skipped because one or more manifest sources were missing.")

    print_summary(planned)

    if has_missing_sources:
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())

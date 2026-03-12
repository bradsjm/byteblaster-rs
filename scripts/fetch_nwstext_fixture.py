#!/usr/bin/env python3

from __future__ import annotations

import argparse
import sys
from pathlib import Path
from urllib.error import HTTPError, URLError
from urllib.request import urlopen


def fetch_fixture(product_id: str) -> bytes:
    url = f"https://mesonet.agron.iastate.edu/api/1/nwstext/{product_id}"
    try:
        with urlopen(url, timeout=30) as response:
            if response.status != 200:
                raise SystemExit(f"unexpected HTTP status {response.status} for {url}")
            body = response.read()
    except HTTPError as err:
        raise SystemExit(f"HTTP error {err.code} while fetching {url}") from err
    except URLError as err:
        raise SystemExit(f"network error while fetching {url}: {err.reason}") from err

    if not body.strip():
        raise SystemExit(f"empty response body for {url}")
    return body


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("product_id")
    parser.add_argument("output_path")
    parser.add_argument("--force", action="store_true")
    args = parser.parse_args()

    output_path = Path(args.output_path)
    if output_path.exists() and not args.force:
        raise SystemExit(f"refusing to overwrite existing fixture: {output_path}")

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_bytes(fetch_fixture(args.product_id))
    print(f"fetched {args.product_id} -> {output_path}")
    return 0


if __name__ == "__main__":
    sys.exit(main())

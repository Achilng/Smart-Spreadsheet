#!/usr/bin/env python3
"""Download a deterministic Danbooru artist dictionary snapshot for bundling."""

from __future__ import annotations

import argparse
import gzip
import json
import shutil
import subprocess
import time
from datetime import datetime, timezone
from pathlib import Path


API_ROOT = "https://danbooru.donmai.us"
PAGE_SIZE = 1000
USER_AGENT = "Smart-Spreadsheet snapshot builder/0.10.0"
QUERIES = {
    "tags": (
        "tags.json?search%5Bcategory%5D=1&search%5Border%5D=id&limit=1000"
        "&only=id,name,post_count,category,is_deprecated"
    ),
    "artists": (
        "artists.json?search%5Border%5D=id&search%5Bis_deleted%5D=false"
        "&search%5Bany_other_name_like%5D=%2A&limit=1000"
        "&only=id,name,other_names,is_deleted,is_banned"
    ),
    "aliases": (
        "tag_aliases.json?search%5Border%5D=id"
        "&search%5Bconsequent_tag%5D%5Bcategory%5D=1&limit=1000"
        "&only=id,antecedent_name,consequent_name,status"
    ),
}


def fetch_json(url: str) -> list[dict[str, object]]:
    curl = shutil.which("curl")
    if curl is None:
        raise RuntimeError("curl is required to update the bundled artist dictionary")
    command = [
        curl,
        "--http1.1",
        "--fail-with-body",
        "--max-time",
        "60",
        "--retry",
        "4",
        "--retry-all-errors",
        "--retry-delay",
        "2",
        "--silent",
        "--show-error",
        "--user-agent",
        USER_AGENT,
        url,
    ]
    for attempt in range(3):
        result = subprocess.run(command, capture_output=True)
        if result.returncode == 0:
            return json.loads(result.stdout)
        if attempt < 2:
            time.sleep(2 ** attempt)
    raise RuntimeError(
        f"curl failed with exit code {result.returncode}: "
        f"{result.stderr.decode('utf-8', errors='replace').strip()}"
    )


def fetch_page(
    name: str,
    cursor: int | None,
    url: str,
    cache_directory: Path | None,
) -> list[dict[str, object]]:
    if cache_directory is None:
        return fetch_json(url)
    cache_directory.mkdir(parents=True, exist_ok=True)
    cache_key = "initial" if cursor is None else f"b{cursor}"
    cache_path = cache_directory / f"{name}-{cache_key}.json"
    if cache_path.exists():
        return json.loads(cache_path.read_text(encoding="utf-8"))
    page = fetch_json(url)
    temporary_path = cache_path.with_suffix(".json.tmp")
    temporary_path.write_text(
        json.dumps(page, ensure_ascii=False, separators=(",", ":")),
        encoding="utf-8",
    )
    temporary_path.replace(cache_path)
    return page


def fetch_pages(
    name: str,
    query: str,
    cache_directory: Path | None,
) -> list[dict[str, object]]:
    items: list[dict[str, object]] = []
    cursor: int | None = None
    for page_number in range(1, 2001):
        cursor_query = "" if cursor is None else f"&page=b{cursor}"
        page = fetch_page(
            name,
            cursor,
            f"{API_ROOT}/{query}{cursor_query}",
            cache_directory,
        )
        if not page:
            break
        next_cursor = int(page[-1]["id"])
        if cursor is not None and next_cursor >= cursor:
            raise RuntimeError(f"{name} cursor did not move backwards: {cursor} -> {next_cursor}")
        items.extend(page)
        cursor = next_cursor
        if page_number == 1 or page_number % 10 == 0 or len(page) < PAGE_SIZE:
            print(f"{name}: page {page_number}, {len(items)} items", flush=True)
        if len(page) < PAGE_SIZE:
            break
        time.sleep(0.12)
    else:
        raise RuntimeError(f"{name} exceeded the page safety limit")
    return items


def write_snapshot(destination: Path, cache_directory: Path | None) -> None:
    snapshot = {
        "snapshotAt": datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
        "dictionary": {
            name: fetch_pages(name, query, cache_directory)
            for name, query in QUERIES.items()
        },
    }
    payload = json.dumps(
        snapshot,
        ensure_ascii=False,
        separators=(",", ":"),
        sort_keys=True,
    ).encode("utf-8")
    destination.parent.mkdir(parents=True, exist_ok=True)
    with destination.open("wb") as output:
        with gzip.GzipFile(filename="", mode="wb", fileobj=output, compresslevel=9, mtime=0) as archive:
            archive.write(payload)
    counts = {name: len(items) for name, items in snapshot["dictionary"].items()}
    print(
        f"wrote {destination} | raw={len(payload)} bytes | gzip={destination.stat().st_size} bytes "
        f"| counts={counts}",
        flush=True,
    )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("destination", type=Path)
    parser.add_argument("--cache", type=Path)
    args = parser.parse_args()
    write_snapshot(
        args.destination.resolve(),
        args.cache.resolve() if args.cache else None,
    )


if __name__ == "__main__":
    main()

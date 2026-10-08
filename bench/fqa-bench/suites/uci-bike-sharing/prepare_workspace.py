#!/usr/bin/env python3
"""Rebuild the compact UCI Bike Sharing workspace from the pinned source zip."""
from __future__ import annotations

import argparse
import csv
import hashlib
from io import StringIO
from pathlib import Path
import sys
from zipfile import ZipFile


HERE = Path(__file__).resolve().parent
ARCHIVE = HERE / "source" / "uci-bike-sharing.zip"
WORKSPACE = HERE / "workspaces" / "capital-bikeshare"
DAILY_OUT = WORKSPACE / "daily.csv"
ROLLUP_OUT = WORKSPACE / "monthly-usage-from-hourly.csv"
EXPECTED_ARCHIVE_SHA256 = "b70182d0d0508e9abbb79306ce5c0cec34869000f8220175ac83d11dbe845401"
ROLLUP_FIELDS = ("yr", "mnth", "rides", "casual_rides", "registered_rides", "hourly_records")


def render() -> tuple[bytes, bytes]:
    archive_bytes = ARCHIVE.read_bytes()
    got_sha256 = hashlib.sha256(archive_bytes).hexdigest()
    if got_sha256 != EXPECTED_ARCHIVE_SHA256:
        raise ValueError(f"source archive SHA-256 mismatch: {got_sha256}")

    with ZipFile(ARCHIVE) as archive:
        daily = archive.read("day.csv")
        hourly_text = archive.read("hour.csv").decode("utf-8-sig")

    rows = csv.DictReader(StringIO(hourly_text, newline=""))
    required = {"yr", "mnth", "cnt", "casual", "registered"}
    if not rows.fieldnames or not required.issubset(rows.fieldnames):
        raise ValueError(f"hour.csv is missing required fields: {sorted(required)}")

    groups: dict[tuple[int, int], dict[str, int]] = {}
    for row in rows:
        key = (int(row["yr"]), int(row["mnth"]))
        values = groups.setdefault(key, {"rides": 0, "casual_rides": 0, "registered_rides": 0, "hourly_records": 0})
        values["rides"] += int(row["cnt"])
        values["casual_rides"] += int(row["casual"])
        values["registered_rides"] += int(row["registered"])
        values["hourly_records"] += 1

    if len(groups) != 24 or sum(v["hourly_records"] for v in groups.values()) != 17379:
        raise ValueError("unexpected hourly source coverage; expected 24 year-months and 17,379 rows")

    output = StringIO(newline="")
    writer = csv.DictWriter(output, fieldnames=ROLLUP_FIELDS, lineterminator="\n")
    writer.writeheader()
    for (year, month), values in sorted(groups.items()):
        writer.writerow({"yr": year, "mnth": month, **values})

    return daily, output.getvalue().encode("utf-8")


def main() -> int:
    parser = argparse.ArgumentParser()
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--write", action="store_true", help="write the workspace data files")
    group.add_argument("--check", action="store_true", help="fail if generated files are stale")
    args = parser.parse_args()

    try:
        daily, rollup = render()
    except (OSError, ValueError, KeyError) as exc:
        print(f"uci-bike-sharing: {exc}", file=sys.stderr)
        return 1

    expected = {DAILY_OUT: daily, ROLLUP_OUT: rollup}
    if args.write:
        WORKSPACE.mkdir(parents=True, exist_ok=True)
        for path, content in expected.items():
            path.write_bytes(content)
        print(f"wrote {len(daily):,} daily bytes and {len(rollup):,} monthly-rollup bytes")
        return 0

    for path, content in expected.items():
        if not path.is_file() or path.read_bytes() != content:
            print(f"uci-bike-sharing: {path.relative_to(HERE)} is missing or stale; run with --write", file=sys.stderr)
            return 1
    print("uci-bike-sharing workspace: current (731 daily rows; 24 monthly groups from 17,379 hourly rows)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

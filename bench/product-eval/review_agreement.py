#!/usr/bin/env python3
"""Summarize agreement between blinded reviewers without seeing answer keys."""
from __future__ import annotations

import argparse
from collections import Counter, defaultdict
from itertools import combinations
import json
import math
from pathlib import Path
import sys


SCHEMA_PATH = Path(__file__).with_name("reviewer-assessment.schema.json")
REVIEW_FIELDS = (
    "answer_behavior", "correctness", "evidence_support", "unsupported_claims",
    "clarification", "chart_correctness", "forecast_quality", "failure_tags",
)


def read_assessments(path: Path) -> list[dict]:
    schema = json.loads(SCHEMA_PATH.read_text(encoding="utf-8"))
    properties = schema["properties"]
    required = schema["required"]
    by_packet: dict[str, dict[str, dict]] = defaultdict(dict)
    for line_no, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        try:
            row = json.loads(line)
        except json.JSONDecodeError as exc:
            raise ValueError(f"{path}:{line_no}: invalid JSON: {exc}") from exc
        if not isinstance(row, dict):
            raise ValueError(f"{path}:{line_no}: assessment must be an object")
        missing = [field for field in required if field not in row]
        extra = sorted(set(row) - set(properties))
        if missing or extra:
            raise ValueError(f"{path}:{line_no}: missing={missing}, disallowed={extra}")
        if type(row["schema_version"]) is not int or row["schema_version"] != 1:
            raise ValueError(f"{path}:{line_no}: unsupported schema_version")
        for field in ("packet_id", "reviewer_code"):
            if not isinstance(row[field], str) or not row[field].strip():
                raise ValueError(f"{path}:{line_no}: {field} must be a non-empty string")
        for field, value in row.items():
            spec = properties[field]
            if "enum" in spec and value not in spec["enum"]:
                raise ValueError(f"{path}:{line_no}: invalid {field} value {value!r}")
        claims = row["unsupported_claims"]
        if type(claims) is not int or claims < 0:
            raise ValueError(f"{path}:{line_no}: unsupported_claims must be a non-negative integer")
        tags = row["failure_tags"]
        if not isinstance(tags, list) or len(tags) != len(set(tags)):
            raise ValueError(f"{path}:{line_no}: failure_tags must be a unique array")
        if any(tag not in properties["failure_tags"]["items"]["enum"] for tag in tags):
            raise ValueError(f"{path}:{line_no}: failure_tags contains an unknown tag")
        for field in properties:
            if field in {"schema_version", "packet_id", "reviewer_code", *REVIEW_FIELDS} or field not in row:
                continue
            value = row[field]
            if value is not None and (
                not isinstance(value, (int, float)) or isinstance(value, bool) or not math.isfinite(value)
            ):
                raise ValueError(f"{path}:{line_no}: {field} must be a finite number or null")
        packet = by_packet[row["packet_id"]]
        reviewer = row["reviewer_code"]
        if reviewer in packet:
            raise ValueError(f"{path}:{line_no}: duplicate reviewer {reviewer!r} for packet {row['packet_id']!r}")
        packet[reviewer] = row
    if not by_packet:
        raise ValueError("review file contains no assessments")
    for packet_id, reviewers in by_packet.items():
        if len(reviewers) < 2:
            raise ValueError(f"packet {packet_id!r} has fewer than two independent reviewers")
    return [
        {"packet_id": packet_id, "reviewers": reviewers}
        for packet_id, reviewers in sorted(by_packet.items())
    ]


def agreement_report(packets: list[dict]) -> str:
    pairs = []
    for packet in packets:
        reviewers = packet["reviewers"]
        for left_code, right_code in combinations(sorted(reviewers), 2):
            pairs.append((reviewers[left_code], reviewers[right_code]))

    lines = [
        "# Blinded reviewer agreement",
        "",
        f"Packets: {len(packets)}; reviewer-packet comparisons: {len(pairs)}.",
        "Exact agreement is descriptive. Cohen’s κ is pooled pairwise for categorical fields; it is omitted for multi-label failure tags and is not a release verdict.",
        "",
        "| Field | Pair comparisons | Exact agreements | Exact agreement | Pooled pairwise Cohen’s κ |",
        "|---|---:|---:|---:|---:|",
    ]
    for field in REVIEW_FIELDS:
        left_values = []
        right_values = []
        exact = 0
        for left, right in pairs:
            left_value = tuple(sorted(left[field])) if field == "failure_tags" else left[field]
            right_value = tuple(sorted(right[field])) if field == "failure_tags" else right[field]
            left_values.append(left_value)
            right_values.append(right_value)
            exact += left_value == right_value
        total = len(pairs)
        pct = "—" if total == 0 else f"{exact / total:.1%}"
        kappa = "—"
        if total and field != "failure_tags":
            left_counts = Counter(left_values)
            right_counts = Counter(right_values)
            expected = sum(left_counts[label] * right_counts[label] for label in left_counts.keys() | right_counts.keys()) / (total * total)
            if expected < 1:
                kappa = f"{((exact / total) - expected) / (1 - expected):.3f}"
        lines.append(f"| {field} | {total} | {exact} | {pct} | {kappa} |")
    lines.append("")
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--assessments", required=True, type=Path, help="blinded raw reviewer JSONL")
    args = parser.parse_args()
    try:
        sys.stdout.write(agreement_report(read_assessments(args.assessments)))
    except (OSError, ValueError, TypeError, KeyError, json.JSONDecodeError) as exc:
        print(f"Review agreement: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

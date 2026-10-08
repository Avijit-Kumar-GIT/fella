#!/usr/bin/env python3
"""Independently check the housing clarification fixture and numeric oracle."""
from __future__ import annotations

import csv
import json
from datetime import date
from pathlib import Path
import sys


HERE = Path(__file__).resolve().parent
WORKSPACE = HERE / "workspaces" / "housing"
TASK_ID = "housing-q1-clarification-and-resume"


def read_jsonl(path: Path) -> list[dict]:
    return [
        json.loads(line)
        for line in path.read_text(encoding="utf-8").splitlines()
        if line.strip()
    ]


def main() -> int:
    try:
        with (WORKSPACE / "ledger.csv").open(encoding="utf-8", newline="") as stream:
            rows = list(csv.DictReader(stream))
        tasks = {row["id"]: row for row in read_jsonl(HERE / "tasks.jsonl")}
        keys = {row["task_id"]: row for row in read_jsonl(HERE / "answer-keys.jsonl")}
        cases = {row["id"]: row for row in read_jsonl(HERE / "cases.jsonl")}
    except (OSError, KeyError, json.JSONDecodeError, csv.Error) as exc:
        print(f"clarification-housing validation: {exc}", file=sys.stderr)
        return 1

    if set(tasks) != {TASK_ID} or set(keys) != {TASK_ID} or set(cases) != {TASK_ID}:
        print("clarification-housing validation: expected one aligned episode", file=sys.stderr)
        return 1

    included = {"rent", "utilities"}
    excluded = {"repair", "maintenance"}
    total = 0.0
    for row in rows:
        observed = date.fromisoformat(row["date"])
        category = row["category"].strip().casefold()
        if date(2024, 1, 1) <= observed <= date(2024, 3, 31):
            if category in included:
                total += float(row["amount"])
            elif category not in excluded:
                print(
                    f"clarification-housing validation: unexpected Q1 category {category!r}",
                    file=sys.stderr,
                )
                return 1

    key = keys[TASK_ID]
    task = tasks[TASK_ID]
    case = cases[TASK_ID]
    expected = key["contracts"][0]["expected_values"]
    if len(expected) != 1 or abs(float(expected[0]) - total) > 0.01:
        print(
            f"clarification-housing validation: oracle mismatch; fixture={total}, gold={expected}",
            file=sys.stderr,
        )
        return 1
    if task["labels"]["answerability"] != "clarification_needed":
        print("clarification-housing validation: task is not tagged clarification_needed", file=sys.stderr)
        return 1
    if case.get("question") != task["request"].get("clarification_reply"):
        print("clarification-housing validation: runner final turn differs from user reply", file=sys.stderr)
        return 1
    graded = case.get("graded_setup_turns", [])
    if len(graded) != 1 or graded[0].get("question") != task["request"]["prompt"]:
        print("clarification-housing validation: initial assistant turn is not graded", file=sys.stderr)
        return 1

    print(
        f"clarification-housing validation: one end-to-end episode; "
        f"fixture total ${total:,.2f} matches the frozen gold"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

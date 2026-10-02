#!/usr/bin/env python3
"""Independent data/oracle checks for this benchmark fixture, not app logic."""
from __future__ import annotations

import csv
import json
from pathlib import Path
import sys


HERE = Path(__file__).resolve().parent
WORKSPACE = HERE / "workspaces" / "capital-bikeshare"


def read_csv(name: str) -> list[dict[str, str]]:
    with (WORKSPACE / name).open(encoding="utf-8", newline="") as stream:
        return list(csv.DictReader(stream))


def read_jsonl(name: str) -> list[dict]:
    return [json.loads(line) for line in (HERE / name).read_text(encoding="utf-8").splitlines() if line.strip()]


def close_list(got: list[float], want: list[float], tolerance: float) -> bool:
    return len(got) == len(want) and all(abs(a - b) <= tolerance for a, b in zip(got, want))


def main() -> int:
    try:
        daily = read_csv("daily.csv")
        monthly_hourly = read_csv("monthly-usage-from-hourly.csv")
        tasks = {row["id"]: row for row in read_jsonl("tasks.jsonl")}
        keys = {row["task_id"]: row for row in read_jsonl("answer-keys.jsonl")}
        cases = {row["id"]: row for row in read_jsonl("cases.jsonl")}
    except (OSError, KeyError, json.JSONDecodeError, csv.Error) as exc:
        print(f"UCI suite validation: {exc}", file=sys.stderr)
        return 1

    if not (len(tasks) == len(keys) == len(cases) == 13):
        print("UCI suite validation: expected 13 aligned tasks, keys, and runner cases", file=sys.stderr)
        return 1
    if set(tasks) != set(keys) or set(tasks) != set(cases):
        print("UCI suite validation: task IDs differ across manifests", file=sys.stderr)
        return 1

    to_int = lambda row, field: int(row[field])
    by_year = {
        year: sum(to_int(row, "cnt") for row in daily if to_int(row, "yr") == year)
        for year in (0, 1)
    }
    working = [to_int(row, "cnt") for row in daily if to_int(row, "workingday") == 1]
    nonworking = [to_int(row, "cnt") for row in daily if to_int(row, "workingday") == 0]
    means = [sum(working) / len(working), sum(nonworking) / len(nonworking)]
    means.append(means[0] - means[1])
    year_2012 = [row for row in daily if to_int(row, "yr") == 1]
    registered_share = (
        sum(to_int(row, "registered") for row in year_2012)
        / sum(to_int(row, "cnt") for row in year_2012)
        * 100
    )

    daily_months = {
        (to_int(row, "yr"), to_int(row, "mnth")): 0
        for row in daily
    }
    for row in daily:
        key = (to_int(row, "yr"), to_int(row, "mnth"))
        daily_months[key] += to_int(row, "cnt")
    hourly_months = {
        (to_int(row, "yr"), to_int(row, "mnth")): to_int(row, "rides")
        for row in monthly_hourly
    }
    if set(daily_months) != set(hourly_months):
        print("UCI suite validation: daily and hourly-derived month keys differ", file=sys.stderr)
        return 1
    max_month_gap = max(abs(daily_months[key] - hourly_months[key]) for key in daily_months)

    monthly_2012 = [daily_months[(1, month)] for month in range(1, 13)]
    june_2011_mean_baseline = sum(daily_months[(0, month)] for month in range(1, 7)) / 6
    lower_2012_scenario = by_year[1] * 0.9
    all_year_months = {
        month: sum(daily_months[(year, month)] for year in (0, 1))
        for month in range(1, 13)
    }
    busiest_month = max(all_year_months, key=all_year_months.get)
    mismatched_rows = sum(
        to_int(row, "casual") + to_int(row, "registered") != to_int(row, "cnt")
        for row in daily
    )
    coverage = [len(daily), min(row["dteday"] for row in daily), max(row["dteday"] for row in daily)]

    actual_values = {
        "bike-annual-total-2012": [by_year[1]],
        "bike-working-vs-nonworking-average": means,
        "bike-registered-share-2012": [registered_share],
        "bike-daily-hourly-month-reconcile": [max_month_gap],
        "bike-monthly-chart-2012": monthly_2012,
        "bike-daily-count-integrity": [mismatched_rows],
        "bike-highest-month-overall": [busiest_month, all_year_months[busiest_month]],
        "bike-daily-coverage": [coverage[0]],
        "bike-followup-next-year": [by_year[1], by_year[1] - by_year[0]],
        "bike-forecast-july-2011-mean-baseline": [june_2011_mean_baseline],
        "bike-scenario-2012-ten-percent-lower": [lower_2012_scenario],
    }
    actual_text = {
        "bike-highest-month-overall": f"{('January', 'February', 'March', 'April', 'May', 'June', 'July', 'August', 'September', 'October', 'November', 'December')[busiest_month - 1]} {all_year_months[busiest_month]}",
        "bike-daily-coverage": f"{coverage[0]} {coverage[1]} {coverage[2]}",
    }

    for task_id, actual in actual_values.items():
        key = keys[task_id]
        contract = key["contracts"][0]
        expected = contract.get("expected_values", [])
        tolerance = contract.get("absolute_tolerance", 0)
        if not close_list([float(v) for v in actual], [float(v) for v in expected], tolerance):
            print(f"UCI suite validation: {task_id} contract mismatch: actual={actual}, expected={expected}", file=sys.stderr)
            return 1
        gold = key["runner_gold"]
        if "figures" in gold and not close_list([float(v) for v in actual], gold["figures"], 0.01):
            print(f"UCI suite validation: {task_id} runner figures mismatch", file=sys.stderr)
            return 1
        if "approx" in gold and abs(float(actual[0]) - gold["approx"][0]) > gold["approx"][1]:
            print(f"UCI suite validation: {task_id} runner approximation mismatch", file=sys.stderr)
            return 1
        for assertion in gold.get("all_of", []):
            if "approx" in assertion and abs(float(actual[0]) - assertion["approx"][0]) > assertion["approx"][1]:
                print(f"UCI suite validation: {task_id} nested runner approximation mismatch", file=sys.stderr)
                return 1
        if "chart" in gold and not close_list(actual, gold["chart"]["series"][0]["values"], 0.01):
            print(f"UCI suite validation: {task_id} chart values mismatch", file=sys.stderr)
            return 1
        if "chart" in gold and (
            gold["chart"].get("require_label_order") is not True
            or gold["chart"].get("labels") != [
                "January", "February", "March", "April", "May", "June",
                "July", "August", "September", "October", "November", "December",
            ]
        ):
            print(f"UCI suite validation: {task_id} chart ordering contract mismatch", file=sys.stderr)
            return 1
        if "contains" in gold and any(term.lower() not in actual_text.get(task_id, "").lower() for term in gold["contains"]):
            print(f"UCI suite validation: {task_id} text contract mismatch", file=sys.stderr)
            return 1

    inventory = set(tasks[next(iter(tasks))]["workspace"]["files"])
    expected_runner_files = {
        f"workspaces/capital-bikeshare/{relative}" for relative in inventory
    }
    for task_id, task in tasks.items():
        if set(task["workspace"]["files"]) != inventory or task["workspace"]["scope"] != "entire_workspace":
            print(f"UCI suite validation: {task_id} does not expose the complete workspace", file=sys.stderr)
            return 1
        if set(cases[task_id]["files"]) != expected_runner_files:
            print(f"UCI suite validation: {task_id} runner file inventory differs", file=sys.stderr)
            return 1
    if len(daily) != 731 or len(monthly_hourly) != 24 or max_month_gap != 0:
        print("UCI suite validation: source/rollup shape or reconciliation changed", file=sys.stderr)
        return 1
    if "hr" in daily[0] or "hr" in monthly_hourly[0]:
        print("UCI suite validation: hour-of-day detail unexpectedly present", file=sys.stderr)
        return 1
    workspace_bytes = sum((WORKSPACE / name).stat().st_size for name in inventory)
    if workspace_bytes >= 100 * 1024:
        print(f"UCI suite validation: workspace is {workspace_bytes} bytes, over the current 100 KiB comparison limit", file=sys.stderr)
        return 1
    unsupported = [task_id for task_id, key in keys.items() if key["expected_behavior"] == "unsupported"]
    if len(unsupported) != 2 or any(
        not isinstance(keys[task_id]["runner_gold"], dict)
        or "all_of" not in keys[task_id]["runner_gold"]
        or not keys[task_id].get("unsupported_reason")
        or not keys[task_id].get("must_not_claim")
        for task_id in unsupported
    ):
        print("UCI suite validation: source-limit cases need explanatory, non-refusal contracts", file=sys.stderr)
        return 1
    if any(
        tasks[task_id]["labels"]["primary_capability"] != "explain_evidence_limits"
        for task_id in unsupported
    ):
        print("UCI suite validation: source-limit capability labels are stale", file=sys.stderr)
        return 1

    print(
        f"UCI suite validation: {len(tasks)} tasks and answer keys; "
        f"{len(daily)} daily rows; {len(monthly_hourly)} monthly groups; "
        f"{workspace_bytes:,} mounted bytes; all numeric/chart contracts match the fixture"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

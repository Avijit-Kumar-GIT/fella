#!/usr/bin/env python3
"""Join agent_eval result JSON to canonical FQA task tags and report slices."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys
from collections import defaultdict


AXES = ("domain", "primary_capability", "analysis_family", "interaction", "answerability", "workspace_scope", "file_format", "data_condition")


def read_jsonl(path: Path) -> list[dict]:
    rows = []
    for line_no, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        try:
            rows.append(json.loads(line))
        except json.JSONDecodeError as exc:
            raise ValueError(f"{path}:{line_no}: invalid JSON: {exc}") from exc
    return rows


def slice_labels(task: dict, axis: str) -> list[str]:
    labels = task.get("labels", {})
    if axis in {"domain", "primary_capability", "interaction", "answerability", "workspace_scope"}:
        value = labels.get(axis)
        return [str(value)] if value else ["untagged"]
    field = {
        "analysis_family": "analysis_families",
        "file_format": "formats",
        "data_condition": "data_conditions",
    }[axis]
    values = labels.get(field) or []
    return sorted({str(value) for value in values}) or ["untagged"]


def summarize(rows: list[dict]) -> dict:
    valid = [row for row in rows if row.get("err") is None and isinstance(row.get("correct"), bool)]
    invalid = len(rows) - len(valid)
    intermediate_turn_rates = [
        float(rate)
        for row in valid
        for rate in row.get("graded_setup_turn_rates", [])
    ]
    if not valid:
        return {
            "cases": len(rows), "valid": 0, "invalid": invalid,
            "correct": None, "rate": None, "tokens": None,
            "intermediate_turns": len(intermediate_turn_rates),
            "intermediate_turn_rate": None,
        }
    correct = sum(row["correct"] for row in valid)
    rate = sum(float(row.get("correct_rate", row["correct"])) for row in valid) / len(valid)
    tokens = sum(int(row.get("prompt_tok", 0)) + int(row.get("completion_tok", 0)) for row in valid) / len(valid)
    return {
        "cases": len(rows),
        "valid": len(valid),
        "invalid": invalid,
        "correct": correct,
        "rate": rate,
        "tokens": tokens,
        "intermediate_turns": len(intermediate_turn_rates),
        "intermediate_turn_rate": (
            sum(intermediate_turn_rates) / len(intermediate_turn_rates)
            if intermediate_turn_rates else None
        ),
    }


def render(tasks: list[dict], results: list[dict], axes: list[str]) -> str:
    task_by_id = {task["id"]: task for task in tasks}
    if len(task_by_id) != len(tasks):
        raise ValueError("duplicate task IDs in task catalog")
    unknown = sorted({row.get("id") for row in results} - task_by_id.keys())
    if unknown:
        raise ValueError(f"result IDs are missing from task catalog: {', '.join(str(x) for x in unknown)}")

    models = sorted({str(row.get("model", "unknown")) for row in results})
    lines = ["# FQA-Bench slice report", "", "Invalid/error rows are reported separately and excluded from correctness rates. Each case row represents the evaluator's result for one episode (which may itself aggregate repeated iterations). Format and data-condition slices are multi-label, so their counts overlap; treat small slices as descriptive, not as stable rankings.", ""]
    for axis in axes:
        lines.extend([f"## By {axis.replace('_', ' ')}", "", "| Model | Slice | Cases | Valid | Invalid | Majority correct | Mean iteration correctness | Graded turns | Intermediate-turn pass rate | Mean tokens |", "|---|---|---:|---:|---:|---:|---:|---:|---:|---:|"])
        for model in models:
            selected = [row for row in results if str(row.get("model", "unknown")) == model]
            groups: dict[str, list[dict]] = defaultdict(list)
            for row in selected:
                for label in slice_labels(task_by_id[row["id"]], axis):
                    groups[label].append(row)
            for label in sorted(groups):
                metric = summarize(groups[label])
                majority = "—" if metric["correct"] is None else f"{metric['correct']}/{metric['valid']} ({metric['correct'] / metric['valid']:.0%})"
                mean_rate = "—" if metric["rate"] is None else f"{metric['rate']:.0%}"
                intermediate_rate = "—" if metric["intermediate_turn_rate"] is None else f"{metric['intermediate_turn_rate']:.0%}"
                tokens = "—" if metric["tokens"] is None else f"{metric['tokens']:.0f}"
                lines.append(
                    f"| {model} | {label} | {metric['cases']} | {metric['valid']} | {metric['invalid']} | {majority} | {mean_rate} | {metric['intermediate_turns']} | {intermediate_rate} | {tokens} |"
                )
        lines.append("")
    represented = len({row.get("id") for row in results})
    lines.extend([f"Catalog coverage: {represented}/{len(task_by_id)} task IDs appeared in the supplied run file.", ""])
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tasks", required=True, type=Path, help="canonical tasks.jsonl or tagged catalog JSONL")
    parser.add_argument("--results", required=True, type=Path, help="agent_eval --json output")
    parser.add_argument("--axis", choices=("all", *AXES), default="all")
    args = parser.parse_args()
    try:
        tasks = read_jsonl(args.tasks)
        results = json.loads(args.results.read_text(encoding="utf-8"))
        if not isinstance(results, list):
            raise ValueError("result JSON must be an array")
        axes = list(AXES) if args.axis == "all" else [args.axis]
        sys.stdout.write(render(tasks, results, axes))
    except (OSError, ValueError, KeyError, TypeError, json.JSONDecodeError) as exc:
        print(f"FQA slice report: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

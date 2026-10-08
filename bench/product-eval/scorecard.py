#!/usr/bin/env python3
"""Build an honest cross-family Fella evaluation scorecard from frozen runs.

The runner JSON contains objective grader and operational fields, not answer
text. Qualitative claims therefore come only from separate blind-review rows.
"""
from __future__ import annotations

import argparse
from datetime import date, datetime, timezone
import hashlib
import json
import math
from pathlib import Path
import random
import statistics
import sys
from collections import Counter, defaultdict
from fingerprint_suites import compute_fingerprints


FAMILY_ORDER = (
    "filesystem_analytics",
    "general_knowledge",
    "web_research",
    "forecasting",
    "chart_correctness",
    "clarification",
    "hybrid",
)

FAMILY_TITLES = {
    "filesystem_analytics": "Filesystem analytics",
    "general_knowledge": "General knowledge / no-workspace routing",
    "web_research": "Web research",
    "forecasting": "Forecasting and scenarios",
    "chart_correctness": "Chart correctness",
    "clarification": "Clarification and continuation",
    "hybrid": "Hybrid local + external research",
}

MANIFEST_FIELDS = {
    "schema_version", "run_id", "benchmark_id", "benchmark_version", "task_set_sha256",
    "workspace_set_sha256", "split", "git_commit", "started_at_utc", "condition",
    "provider", "model", "model_version", "model_settings", "evaluation_protocol_settings",
    "runner_settings", "iterations", "result_sha256", "pricing_snapshot_sha256",
}

NOT_GRADED_BY_RUNNER = (
    "false deferral",
    "unsupported-claim rate",
    "evidence/citation support",
    "chart suitability and visual semantics",
    "forecast calibration",
    "helpfulness / communication quality",
)

REVIEW_ENUMS = {
    "task_expected_behavior": {"answer", "estimate", "clarify", "unsupported", "no_analysis_needed"},
    "answer_behavior": {"answered", "clarified", "useful_limit", "unnecessary_deferral", "error_or_empty", "unresolved"},
    "correctness": {"correct", "partially_correct", "incorrect", "unresolved"},
    "evidence_support": {"supported", "partially_supported", "unsupported", "not_applicable", "unresolved"},
    "clarification": {"necessary_and_used", "necessary_but_not_asked", "unnecessary", "not_applicable", "unresolved"},
    "chart_correctness": {"faithful_and_appropriate", "data_error", "semantic_error", "inappropriate_chart", "not_applicable", "unresolved"},
    "forecast_quality": {"appropriate_and_calibrated", "method_acceptable_limits_missing", "incorrect", "unsupported", "not_applicable", "unresolved"},
    "adjudication": {"single_review", "agreement", "adjudicated"},
}
REVIEW_FAILURE_TAGS = {
    "source_missed", "intent_misread", "normalization_error", "wrong_scope_or_filter",
    "wrong_join_or_grain", "calculation_error", "unsupported_claim", "unnecessary_deferral",
    "missed_clarification", "bad_continuity", "chart_data_error", "chart_semantics_error",
    "forecast_method_or_calibration", "provenance_gap", "tool_friction",
    "privacy_or_boundary_violation", "communication_issue", "other",
}


def validate_review_annotation(review: dict) -> None:
    required = (
        "schema_version", "benchmark_version", "run_id", "task_id", "model", "profile",
        "iteration", "reviewer_code", "condition", *REVIEW_ENUMS, "unsupported_claims", "failure_tags",
    )
    missing = [field for field in required if field not in review]
    if missing:
        raise ValueError("review annotation is missing: " + ", ".join(missing))
    if type(review["schema_version"]) is not int or review["schema_version"] != 1:
        raise ValueError("unsupported review annotation schema_version")
    for field in ("benchmark_version", "run_id", "task_id", "model", "profile", "reviewer_code"):
        if not isinstance(review[field], str) or not review[field].strip():
            raise ValueError(f"review annotation {field} must be a non-empty string")
    if type(review["iteration"]) is not int or review["iteration"] < 1:
        raise ValueError("review annotation iteration must be a positive integer")
    if review["condition"] not in {"baseline", "candidate", "ablation", "diagnostic"}:
        raise ValueError("review annotation condition is invalid")
    for field, allowed in REVIEW_ENUMS.items():
        if review[field] not in allowed:
            raise ValueError(f"review annotation has invalid {field}: {review[field]!r}")
    if type(review["unsupported_claims"]) is not int or review["unsupported_claims"] < 0:
        raise ValueError("unsupported_claims must be a non-negative integer")
    if not isinstance(review["failure_tags"], list) or any(not isinstance(tag, str) for tag in review["failure_tags"]):
        raise ValueError("failure_tags must be an array of strings")
    if len(set(review["failure_tags"])) != len(review["failure_tags"]):
        raise ValueError("failure_tags must not contain duplicates")
    if any(tag not in REVIEW_FAILURE_TAGS for tag in review["failure_tags"]):
        raise ValueError("failure_tags contains an unknown tag")
    for field in (
        "forecast_value", "forecast_observed", "forecast_baseline_value",
        "forecast_interval_lower", "forecast_interval_upper",
    ):
        value = review.get(field)
        if value is not None and (
            not isinstance(value, (int, float)) or isinstance(value, bool) or not math.isfinite(value)
        ):
            raise ValueError(f"{field} must be numeric or null")
    lower = review.get("forecast_interval_lower")
    upper = review.get("forecast_interval_upper")
    if lower is not None and upper is not None and lower > upper:
        raise ValueError("forecast interval lower bound exceeds upper bound")


def read_jsonl(path: Path) -> list[dict]:
    records: list[dict] = []
    for line_no, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        try:
            record = json.loads(line)
        except json.JSONDecodeError as exc:
            raise ValueError(f"{path}:{line_no}: invalid JSON: {exc}") from exc
        if not isinstance(record, dict):
            raise ValueError(f"{path}:{line_no}: each JSONL record must be an object")
        records.append(record)
    return records


def read_results(path: Path) -> list[dict]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, list) or any(not isinstance(row, dict) for row in value):
        raise ValueError(f"{path}: result JSON must be an array of objects")
    return value


def families_for_task(task: dict, source_family: str) -> set[str]:
    """Map task metadata to product-level reporting families, not tool routes."""
    if source_family in FAMILY_ORDER and source_family != "filesystem_analytics":
        return {source_family}

    labels = task.get("labels", {})
    if not isinstance(labels, dict):
        labels = {}
    families: set[str] = set()
    if source_family == "fqa":
        families.add("filesystem_analytics")
        if labels.get("primary_capability") == "forecast_and_scenario":
            families.add("forecasting")
        if "visualization" in labels.get("analysis_families", []):
            families.add("chart_correctness")
        if labels.get("interaction") == "clarification_episode":
            families.add("clarification")
    return families


def _number(row: dict, field: str) -> float | None:
    value = row.get(field)
    if isinstance(value, (int, float)) and not isinstance(value, bool):
        return float(value)
    return None


def is_valid_result(row: dict) -> bool:
    return "err" in row and row["err"] is None and isinstance(row.get("correct"), bool)


def summarize(rows: list[dict], expected_tasks: int, prices: dict | None = None) -> dict:
    valid = [row for row in rows if is_valid_result(row)]
    invalid = len(rows) - len(valid)

    def mean_field(field: str, transform=lambda value: value) -> float | None:
        values = [transform(row[field]) for row in valid if _number(row, field) is not None]
        return statistics.fmean(values) if values else None

    cost_values = []
    rates = (prices or {}).get("rates", {})
    for row in valid:
        rate = rates.get(str(row.get("model", "")))
        prompt = _number(row, "prompt_tok")
        completion = _number(row, "completion_tok")
        if isinstance(rate, dict) and prompt is not None and completion is not None:
            cost_values.append(
                (prompt * float(rate["input_per_million"]) + completion * float(rate["output_per_million"])) / 1_000_000
            )

    return {
        "catalog_tasks": expected_tasks,
        "result_rows": len(rows),
        "valid": len(valid),
        "invalid": invalid,
        "missing": max(0, expected_tasks - len({row.get("id") for row in rows})),
        "majority_correct": sum(row["correct"] for row in valid),
        "majority_rate": (
            sum(row["correct"] for row in valid) / len(valid) if valid else None
        ),
        "iteration_accuracy": mean_field(
            "correct_rate", lambda value: float(value)
        ) if any(_number(row, "correct_rate") is not None for row in valid) else (
            statistics.fmean(float(row["correct"]) for row in valid) if valid else None
        ),
        "tokens": statistics.fmean(
            float(row.get("prompt_tok", 0) or 0) + float(row.get("completion_tok", 0) or 0)
            for row in valid
            if "prompt_tok" in row or "completion_tok" in row
        ) if any("prompt_tok" in row or "completion_tok" in row for row in valid) else None,
        "model_calls": mean_field("model_calls"),
        "tool_calls": mean_field("tool_calls"),
        "model_call_coverage": (
            sum(int(row.get("model_call_observed_iterations", 0) or 0) for row in valid)
            / sum(max(1, int(row.get("iters", 1) or 1)) for row in valid)
            if valid else None
        ),
        "tool_call_coverage": (
            sum(int(row.get("tool_call_observed_iterations", 0) or 0) for row in valid)
            / sum(max(1, int(row.get("iters", 1) or 1)) for row in valid)
            if valid else None
        ),
        "estimated_cost_usd": statistics.fmean(cost_values) if cost_values else None,
        "priced_rows": len(cost_values),
        "seconds": mean_field("total_s"),
        "steps": mean_field("steps"),
        "failed_tool_calls": sum(int(row.get("failed_tool_calls", 0) or 0) for row in valid),
        "redundant_calls": sum(int(row.get("redundant_calls", 0) or 0) for row in valid),
        "unreferenced_sql_results": sum(int(row.get("unreferenced_sql_results", 0) or 0) for row in valid),
        "verification_mismatch_rows": sum(
            1 for row in valid
            if (row.get("correct") is True and row.get("verification_status") in {"needs_review", "failed", "insufficient_data"})
            or (row.get("correct") is False and row.get("verification_status") == "verified")
        ),
    }


def result_key(row: dict, source: str) -> tuple[str, str, str]:
    task_id = row.get("id")
    if not isinstance(task_id, str) or not task_id:
        raise ValueError(f"{source}: result row has no string task id")
    model = str(row.get("model", "unknown"))
    profile = str(row.get("profile", "unknown"))
    return task_id, model, profile


def index_results(results: list[dict], source: str) -> dict[tuple[str, str, str], dict]:
    indexed: dict[tuple[str, str, str], dict] = {}
    for row in results:
        key = result_key(row, source)
        if key in indexed:
            raise ValueError(f"{source}: duplicate result key {key!r}")
        indexed[key] = row
    return indexed


def bootstrap_mean_ci(deltas: list[float], seed: int = 81027, samples: int = 5000) -> tuple[float, float] | None:
    """Deterministic paired bootstrap interval; descriptive, not a pass/fail test."""
    if not deltas:
        return None
    if len(deltas) == 1:
        return deltas[0], deltas[0]
    rng = random.Random(seed)
    means = sorted(
        statistics.fmean(rng.choices(deltas, k=len(deltas)))
        for _ in range(samples)
    )
    low = means[max(0, math.floor(0.025 * samples))]
    high = means[min(samples - 1, math.ceil(0.975 * samples) - 1)]
    return low, high


def comparison_summary(
    baseline: dict[tuple[str, str, str], dict],
    candidate: dict[tuple[str, str, str], dict],
    task_ids: set[str],
    model: str,
    profile: str,
) -> dict:
    baseline_keys = {key for key in baseline if key[0] in task_ids and key[1:] == (model, profile)}
    candidate_keys = {key for key in candidate if key[0] in task_ids and key[1:] == (model, profile)}
    common_ids = sorted(baseline_keys & candidate_keys)
    valid_pairs = [
        (baseline[key], candidate[key])
        for key in common_ids
        if is_valid_result(baseline[key]) and is_valid_result(candidate[key])
    ]
    deltas = [
        float(new.get("correct_rate", new["correct"]))
        - float(old.get("correct_rate", old["correct"]))
        for old, new in valid_pairs
    ]
    ci = bootstrap_mean_ci(deltas)
    return {
        "paired": len(valid_pairs),
        "common": len(common_ids),
        "baseline_only": len(baseline_keys - candidate_keys),
        "candidate_only": len(candidate_keys - baseline_keys),
        "invalid_pairs": len(common_ids) - len(valid_pairs),
        "improved": sum(delta > 0 for delta in deltas),
        "worsened": sum(delta < 0 for delta in deltas),
        "unchanged": sum(delta == 0 for delta in deltas),
        "mean_delta": statistics.fmean(deltas) if deltas else None,
        "ci": ci,
    }


def validate_manifest_pair(
    baseline_path: Path, candidate_path: Path, baseline_results: Path, candidate_results: Path,
    prices_path: Path | None = None,
    input_fingerprints: dict | None = None,
) -> tuple[dict, dict]:
    base = json.loads(baseline_path.read_text(encoding="utf-8"))
    cand = json.loads(candidate_path.read_text(encoding="utf-8"))
    parity_fields = (
        "benchmark_id", "benchmark_version", "task_set_sha256", "workspace_set_sha256",
        "split", "provider", "model", "model_version", "model_settings", "evaluation_protocol_settings",
        "iterations", "pricing_snapshot_sha256",
    )
    required_fields = (
        "schema_version", "run_id", "benchmark_id", "benchmark_version", "task_set_sha256",
        "workspace_set_sha256", "split", "git_commit", "started_at_utc", "condition",
        "provider", "model", "model_version", "model_settings", "evaluation_protocol_settings",
        "runner_settings", "iterations", "result_sha256", "pricing_snapshot_sha256",
    )
    for label, manifest in (("baseline", base), ("candidate", cand)):
        if not isinstance(manifest, dict):
            raise ValueError(f"{label} run manifest must be a JSON object")
        extra = sorted(set(manifest) - MANIFEST_FIELDS)
        if extra:
            raise ValueError(f"{label} run manifest has disallowed fields: " + ", ".join(extra))
        missing = [field for field in required_fields if field not in manifest]
        if missing:
            raise ValueError(f"{label} run manifest is missing: " + ", ".join(missing))
        if type(manifest["schema_version"]) is not int or manifest["schema_version"] != 1:
            raise ValueError(f"{label} run manifest has unsupported schema_version")
        if type(manifest["iterations"]) is not int:
            raise ValueError(f"{label} run manifest iterations must be an integer")
        if manifest["iterations"] < 3:
            raise ValueError(f"{label} run has fewer than three repetitions")
        for field in ("run_id", "benchmark_id", "benchmark_version", "provider", "model", "model_version"):
            if not isinstance(manifest[field], str) or not manifest[field].strip():
                raise ValueError(f"{label} run manifest field {field} must be a non-empty string")
        if not isinstance(manifest["git_commit"], str) or len(manifest["git_commit"]) < 7:
            raise ValueError(f"{label} run manifest git_commit must contain at least 7 characters")
        for field in ("task_set_sha256", "workspace_set_sha256", "result_sha256"):
            digest = manifest[field]
            if not isinstance(digest, str) or len(digest) != 64 or any(char not in "0123456789abcdef" for char in digest):
                raise ValueError(f"{label} run manifest has invalid {field}")
        price_hash = manifest["pricing_snapshot_sha256"]
        if price_hash is not None and (
            not isinstance(price_hash, str)
            or len(price_hash) != 64
            or any(char not in "0123456789abcdef" for char in price_hash)
        ):
            raise ValueError(f"{label} run manifest has invalid pricing_snapshot_sha256")
        started_at = manifest["started_at_utc"]
        try:
            parsed_time = datetime.fromisoformat(started_at.replace("Z", "+00:00"))
        except (AttributeError, TypeError, ValueError) as exc:
            raise ValueError(f"{label} run manifest started_at_utc must be an ISO-8601 timestamp") from exc
        if parsed_time.tzinfo is None or parsed_time.utcoffset() != timezone.utc.utcoffset(parsed_time):
            raise ValueError(f"{label} run manifest started_at_utc must be UTC")
        if manifest["split"] not in {"development", "anchor", "blind", "smoke"}:
            raise ValueError(f"{label} run manifest has invalid split")
        if not isinstance(manifest["model_settings"], dict):
            raise ValueError(f"{label} run manifest model_settings must be an object")
        extra_model = set(manifest["model_settings"]) - {
            "temperature", "top_p", "seed", "max_output_tokens", "reasoning_effort", "other_settings_sha256"
        }
        if extra_model:
            raise ValueError(f"{label} run manifest model_settings has disallowed fields: " + ", ".join(sorted(extra_model)))
        for field in ("temperature", "top_p"):
            value = manifest["model_settings"].get(field)
            if field in manifest["model_settings"] and (
                not isinstance(value, (int, float)) or isinstance(value, bool) or not math.isfinite(value)
            ):
                raise ValueError(f"{label} run manifest model_settings.{field} must be finite numeric")
        for field in ("seed", "max_output_tokens"):
            value = manifest["model_settings"].get(field)
            if field in manifest["model_settings"] and type(value) is not int:
                raise ValueError(f"{label} run manifest model_settings.{field} must be an integer")
        if "max_output_tokens" in manifest["model_settings"] and manifest["model_settings"]["max_output_tokens"] < 1:
            raise ValueError(f"{label} run manifest model_settings.max_output_tokens must be positive")
        reasoning = manifest["model_settings"].get("reasoning_effort")
        if "reasoning_effort" in manifest["model_settings"] and not isinstance(reasoning, str):
            raise ValueError(f"{label} run manifest model_settings.reasoning_effort must be a string")
        other_settings_hash = manifest["model_settings"].get("other_settings_sha256")
        if "other_settings_sha256" in manifest["model_settings"] and (
            not isinstance(other_settings_hash, str)
            or len(other_settings_hash) != 64
            or any(char not in "0123456789abcdef" for char in other_settings_hash)
        ):
            raise ValueError(f"{label} run manifest has invalid model_settings.other_settings_sha256")
        if not isinstance(manifest["evaluation_protocol_settings"], dict):
            raise ValueError(f"{label} run manifest evaluation_protocol_settings must be an object")
        extra_protocol = set(manifest["evaluation_protocol_settings"]) - {
            "budget_policy_sha256", "max_episode_seconds", "max_model_calls", "max_tool_calls",
            "evaluator_version", "rubric_version",
        }
        if extra_protocol:
            raise ValueError(f"{label} run manifest evaluation_protocol_settings has disallowed fields: " + ", ".join(sorted(extra_protocol)))
        protocol = manifest["evaluation_protocol_settings"]
        budget_hash = protocol.get("budget_policy_sha256")
        if "budget_policy_sha256" in protocol and (
            not isinstance(budget_hash, str)
            or len(budget_hash) != 64
            or any(char not in "0123456789abcdef" for char in budget_hash)
        ):
            raise ValueError(f"{label} run manifest has invalid evaluation protocol budget_policy_sha256")
        duration = protocol.get("max_episode_seconds")
        if "max_episode_seconds" in protocol and (
            not isinstance(duration, (int, float)) or isinstance(duration, bool)
            or not math.isfinite(duration) or duration <= 0
        ):
            raise ValueError(f"{label} run manifest max_episode_seconds must be finite and positive")
        for field, minimum in (("max_model_calls", 1), ("max_tool_calls", 0)):
            value = protocol.get(field)
            if field in protocol and (type(value) is not int or value < minimum):
                raise ValueError(f"{label} run manifest {field} is invalid")
        for field in ("evaluator_version", "rubric_version"):
            value = protocol.get(field)
            if field in protocol and (not isinstance(value, str) or not value.strip()):
                raise ValueError(f"{label} run manifest {field} must be a non-empty string")
        if not isinstance(manifest["runner_settings"], dict) or not isinstance(manifest["runner_settings"].get("harness"), str) or not manifest["runner_settings"]["harness"].strip():
            raise ValueError(f"{label} run manifest runner_settings must identify its harness")
        extra_runner = set(manifest["runner_settings"]) - {
            "harness", "toolset_sha256", "configuration_sha256", "profile"
        }
        if extra_runner:
            raise ValueError(f"{label} run manifest runner_settings has disallowed fields: " + ", ".join(sorted(extra_runner)))
        for field in ("toolset_sha256", "configuration_sha256"):
            digest = manifest["runner_settings"].get(field)
            if field in manifest["runner_settings"] and (
                not isinstance(digest, str)
                or len(digest) != 64
                or any(char not in "0123456789abcdef" for char in digest)
            ):
                raise ValueError(f"{label} run manifest has invalid runner_settings.{field}")
        profile = manifest["runner_settings"].get("profile")
        if "profile" in manifest["runner_settings"] and not isinstance(profile, str):
            raise ValueError(f"{label} run manifest runner_settings.profile must be a string")
    if base["condition"] != "baseline" or cand["condition"] != "candidate":
        raise ValueError("run manifests must identify baseline and candidate conditions")
    if prices_path:
        price_hash = hashlib.sha256(prices_path.read_bytes()).hexdigest()
        if base.get("pricing_snapshot_sha256") != price_hash or cand.get("pricing_snapshot_sha256") != price_hash:
            raise ValueError("price snapshot hash does not match both run manifests")
    differences = [field for field in parity_fields if base.get(field) != cand.get(field)]
    if differences:
        raise ValueError("baseline/candidate run manifests differ in: " + ", ".join(differences))
    if input_fingerprints is not None:
        for label, manifest in (("baseline", base), ("candidate", cand)):
            for field in ("task_set_sha256", "workspace_set_sha256"):
                if manifest[field] != input_fingerprints[field]:
                    raise ValueError(f"{label} run manifest {field} does not match supplied task/workspace files")
    for manifest, results_path, label in (
        (base, baseline_results, "baseline"), (cand, candidate_results, "candidate")
    ):
        digest = hashlib.sha256(results_path.read_bytes()).hexdigest()
        if manifest.get("result_sha256") != digest:
            raise ValueError(f"{label} result file does not match its run manifest hash")
    return base, cand


def _fmt(value: float | None, suffix: str = "") -> str:
    return "—" if value is None else f"{value:.1%}{suffix}"


def _fmt_call_metric(value: float | None, coverage: float | None) -> str:
    if coverage is None:
        return "—"
    if value is None:
        return f"— ({coverage:.0%} traced)"
    return f"{value:.1f} ({coverage:.0%} traced)"


def render(
    source_sets: list[tuple[str, list[dict], list[dict]]],
    reviews: list[dict] | None = None,
    baseline_results: list[dict] | None = None,
    candidate_results: list[dict] | None = None,
    prices: dict | None = None,
) -> str:
    task_family_members: dict[str, set[str]] = defaultdict(set)
    task_catalog: dict[str, dict] = {}
    result_rows: dict[tuple[str, str, str], dict] = {}
    family_reviews: dict[str, list[dict]] = defaultdict(list)

    for source_name, tasks, results in source_sets:
        task_ids = set()
        for task in tasks:
            task_id = task.get("id")
            if not isinstance(task_id, str) or not task_id:
                raise ValueError(f"{source_name}: task has no string id")
            if task_id in task_catalog:
                raise ValueError(f"duplicate task id across inputs: {task_id!r}")
            task_catalog[task_id] = task
            task_ids.add(task_id)
            for family in families_for_task(task, source_name):
                task_family_members[family].add(task_id)

        rows_by_key = index_results(results, source_name)
        unknown = sorted({key[0] for key in rows_by_key} - task_ids)
        if unknown:
            raise ValueError(f"{source_name}: result IDs absent from task catalog: {', '.join(unknown)}")
        for key, row in rows_by_key.items():
            if key in result_rows:
                raise ValueError(f"duplicate result key across inputs: {key!r}")
            result_rows[key] = row

    review_keys: set[tuple] = set()
    for review in reviews or []:
        validate_review_annotation(review)
        task_id = review.get("task_id")
        if task_id not in task_catalog:
            raise ValueError(f"review row refers to task absent from catalogs: {task_id!r}")
        if not isinstance(review.get("condition"), str) or not review["condition"]:
            raise ValueError(f"review row for {task_id!r} must include its unblinded condition label")
        if review["adjudication"] not in {"agreement", "adjudicated"}:
            raise ValueError(f"review row for {task_id!r} is not consensus/adjudicated")
        review_key = tuple(review[field] for field in ("run_id", "task_id", "model", "profile", "iteration", "condition"))
        if review_key in review_keys:
            raise ValueError(f"duplicate consensus review for task/run iteration: {review_key}")
        review_keys.add(review_key)
        for family in (family for family, ids in task_family_members.items() if task_id in ids):
            family_reviews[family].append(review)

    model_profiles = sorted({(key[1], key[2]) for key in result_rows})
    model_profiles.extend(
        pair for pair in sorted({(str(row.get("model", "unknown")), str(row.get("profile", "unknown"))) for row in reviews or []})
        if pair not in model_profiles
    )

    lines = [
        "# Fella capability scorecard",
        "",
        "This report separates product families. A missing result is not a pass; invalid runs are not counted as model failures. Qualitative rates come only from attached independent-review annotations. The runner’s objective grade is a screening signal, not a substitute for reading the answer and trace.",
        "",
        "| Capability family | Model | Profile | Catalog tasks | Run rows | Valid | Invalid | Missing | Correct / valid | Mean iteration accuracy | Mean tokens | Est. USD / priced row | Model calls / episode | Runtime tool calls / episode | Mean seconds | Steps | Failed tool calls | Duplicate calls | Qualitative review |",
        "|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|",
    ]
    summaries: dict[tuple[str, str, str], dict] = {}
    for family in FAMILY_ORDER:
        ids = task_family_members.get(family, set())
        profiles = model_profiles or [("—", "—")]
        for model, profile in profiles:
            rows = [result_rows[(task_id, model, profile)] for task_id in sorted(ids) if (task_id, model, profile) in result_rows]
            metrics = summarize(rows, len(ids), prices)
            summaries[(family, model, profile)] = metrics
            correct = "—" if metrics["majority_rate"] is None else f"{metrics['majority_correct']}/{metrics['valid']} ({metrics['majority_rate']:.1%})"
            reviews_for_group = [
                row for row in family_reviews.get(family, [])
                if str(row.get("model", "unknown")) == model and str(row.get("profile", "unknown")) == profile
            ]
            qualitative = f"{len(reviews_for_group)} review rows" if reviews_for_group else "not reviewed"
            values = [
                FAMILY_TITLES[family], model, profile,
                str(metrics["catalog_tasks"]), str(metrics["result_rows"]),
                str(metrics["valid"]), str(metrics["invalid"]), str(metrics["missing"]),
                correct, _fmt(metrics["iteration_accuracy"]),
                "—" if metrics["tokens"] is None else f"{metrics['tokens']:.0f}",
                "—" if metrics["estimated_cost_usd"] is None else f"${metrics['estimated_cost_usd']:.4f} ({metrics['priced_rows']}/{metrics['valid']})",
                _fmt_call_metric(metrics["model_calls"], metrics["model_call_coverage"]),
                _fmt_call_metric(metrics["tool_calls"], metrics["tool_call_coverage"]),
                "—" if metrics["seconds"] is None else f"{metrics['seconds']:.2f}",
                "—" if metrics["steps"] is None else f"{metrics['steps']:.1f}",
                str(metrics["failed_tool_calls"]), str(metrics["redundant_calls"]), qualitative,
            ]
            lines.append("| " + " | ".join(values) + " |")

    lines.extend([
        "",
        "Call counts include the full episode (setup, clarification, and final answer) when canonical traces are available. Estimated USD/episode is calculated only when the caller supplies a dated, pinned rate snapshot; no provider prices are hard-coded or fetched live. Models without a matching rate remain unpriced.",
        "",
        "## Runner diagnostics",
        "",
        "`observations` are not labeled as total tool calls here: the current runner intentionally distinguishes source-inspection operations, exact duplicate calls, failed calls, and unreferenced SQL-result signals. These are friction diagnostics, not a claim that every non-answer figure was wasted.",
        "",
        "| Capability family | Model | Profile | Verification/result mismatch rows | Failed tool calls | Duplicate calls | Unreferenced SQL-result signals |",
        "|---|---|---|---:|---:|---:|---:|",
    ])
    for family in FAMILY_ORDER:
        for model, profile in model_profiles or [("—", "—")]:
            m = summaries[(family, model, profile)]
            lines.append(f"| {FAMILY_TITLES[family]} | {model} | {profile} | {m['verification_mismatch_rows']} | {m['failed_tool_calls']} | {m['redundant_calls']} | {m['unreferenced_sql_results']} |")

    lines.extend(["", "## Qualitative dimensions", "", "The following are **not inferred** from objective result JSON:", ""])
    for dimension in NOT_GRADED_BY_RUNNER:
        lines.append(f"- {dimension}: attach blind-review annotations to report it.")

    if reviews:
        lines.extend(["", "## Blind-review summary", "", "Rates below use consensus/adjudicated annotations only; unresolved and not-applicable labels stay in their own denominators. Conditions are restored only after reviewer labels are locked.", ""])
        lines.append("| Capability family | Model | Profile | Condition | Reviewed | Answer coverage | Unnecessary deferral | Needed clarification | Useful evidence limit | Correctness | Evidence support | Unsupported claims present | Chart correctness | Forecast quality | Forecast MAE | Baseline MAE | Interval coverage |")
        lines.append("|---|---|---|---|---:|---:|---:|---:|---:|---|---|---:|---|---|---:|---:|---:|")
        for family in FAMILY_ORDER:
            rows = family_reviews.get(family, [])
            groups = sorted({
                (str(row.get("model", "unknown")), str(row.get("profile", "unknown")), str(row["condition"]))
                for row in rows
            })
            if not groups:
                lines.append(f"| {FAMILY_TITLES[family]} | — | — | — | 0 | — | — | — | — | — | — | — | — | — | — | — | — |")
                continue
            for model, profile, condition in groups:
                scoped = [
                    row for row in rows
                    if str(row.get("model", "unknown")) == model
                    and str(row.get("profile", "unknown")) == profile
                    and row["condition"] == condition
                ]
                correctness = Counter(row.get("correctness", "unresolved") for row in scoped)
                evidence = Counter(row.get("evidence_support", "unresolved") for row in scoped)
                clarification = Counter(row.get("clarification", "not_applicable") for row in scoped)
                charts = Counter(row.get("chart_correctness", "not_applicable") for row in scoped)
                forecasts = Counter(row.get("forecast_quality", "not_applicable") for row in scoped)
                unsupported = sum(int(row.get("unsupported_claims", 0) or 0) > 0 for row in scoped)
                answerable = [row for row in scoped if row["task_expected_behavior"] in {"answer", "estimate", "no_analysis_needed"}]
                clarify_expected = [row for row in scoped if row["task_expected_behavior"] == "clarify"]
                unsupported_expected = [row for row in scoped if row["task_expected_behavior"] == "unsupported"]
                answered = sum(row.get("answer_behavior") == "answered" for row in answerable)
                unnecessary_deferrals = sum(row.get("answer_behavior") == "unnecessary_deferral" for row in answerable)
                necessary_clarifications = sum(row.get("clarification") == "necessary_and_used" for row in clarify_expected)
                useful_limits = sum(row.get("answer_behavior") == "useful_limit" for row in unsupported_expected)
                forecast_rows = [
                    row for row in scoped
                    if isinstance(row.get("forecast_value"), (int, float))
                    and isinstance(row.get("forecast_observed"), (int, float))
                ]
                forecast_mae = statistics.fmean(
                    abs(row["forecast_value"] - row["forecast_observed"]) for row in forecast_rows
                ) if forecast_rows else None
                baseline_rows = [row for row in forecast_rows if isinstance(row.get("forecast_baseline_value"), (int, float))]
                baseline_mae = statistics.fmean(
                    abs(row["forecast_baseline_value"] - row["forecast_observed"]) for row in baseline_rows
                ) if baseline_rows else None
                interval_rows = [
                    row for row in forecast_rows
                    if isinstance(row.get("forecast_interval_lower"), (int, float))
                    and isinstance(row.get("forecast_interval_upper"), (int, float))
                ]
                interval_coverage = (
                    sum(row["forecast_interval_lower"] <= row["forecast_observed"] <= row["forecast_interval_upper"] for row in interval_rows)
                    / len(interval_rows)
                    if interval_rows else None
                )
                lines.append(
                    f"| {FAMILY_TITLES[family]} | {model} | {profile} | {condition} | {len(scoped)} | {answered}/{len(answerable)} ({(answered / len(answerable) if answerable else 0):.0%}) | {unnecessary_deferrals}/{len(answerable)} ({(unnecessary_deferrals / len(answerable) if answerable else 0):.0%}) | {necessary_clarifications}/{len(clarify_expected)} | {useful_limits}/{len(unsupported_expected)} | {_counter(correctness)} | {_counter(evidence)} | {unsupported}/{len(scoped)} | {_counter(charts)} | {_counter(forecasts)} | {('—' if forecast_mae is None else f'{forecast_mae:.4g}')} | {('—' if baseline_mae is None else f'{baseline_mae:.4g}')} | {('—' if interval_coverage is None else f'{interval_coverage:.0%}')} |"
                )

    if baseline_results is not None and candidate_results is not None:
        base_index = index_results(baseline_results, "baseline")
        candidate_index = index_results(candidate_results, "candidate")
        known_ids = set(task_catalog)
        for label, index in (("baseline", base_index), ("candidate", candidate_index)):
            unknown = sorted({key[0] for key in index} - known_ids)
            if unknown:
                raise ValueError(f"{label} result IDs absent from task catalogs: {', '.join(unknown)}")
        pair_profiles = sorted({key[1:] for key in (*base_index.keys(), *candidate_index.keys())}) or [("—", "—")]
        lines.extend(["", "## Paired baseline comparison", "", "Positive deltas favor the candidate. The paired bootstrap interval is descriptive and must be interpreted with sample size, task-family breadth, and validity counts; it is not an automatic release verdict.", "", "| Capability family | Model | Profile | Paired valid tasks | Baseline-only rows | Candidate-only rows | Invalid pairs | Candidate improved | Candidate worsened | Unchanged | Mean accuracy delta | Paired bootstrap 95% interval |", "|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---|"])
        for family in FAMILY_ORDER:
            ids = task_family_members.get(family, set())
            for model, profile in pair_profiles:
                comp = comparison_summary(base_index, candidate_index, ids, model, profile)
                delta = comp["mean_delta"]
                ci = comp["ci"]
                delta_text = "—" if delta is None else f"{delta:+.1%}"
                ci_text = "—" if ci is None else f"[{ci[0]:+.1%}, {ci[1]:+.1%}]"
                lines.append(f"| {FAMILY_TITLES[family]} | {model} | {profile} | {comp['paired']} | {comp['baseline_only']} | {comp['candidate_only']} | {comp['invalid_pairs']} | {comp['improved']} | {comp['worsened']} | {comp['unchanged']} | {delta_text} | {ci_text} |")
        lines.extend(["", "Rows omitted from the paired denominator because one side was absent or invalid are not hidden: inspect each run’s missing/invalid columns above. The run manifests must match model, settings, benchmark/workspace hashes, split, and iteration count before this comparison is interpreted as fair."])

    lines.extend(["", "## Interpretation limits", "", "- `Correct / valid` is the benchmark runner’s declared grader result. It can still have rubric limitations; use the frozen task version and preserve disputed results for separate adjudication.", "- Mean iteration accuracy is averaged from each task’s repeated-run rate. Majority correctness is shown separately.", "- Forecast, chart, clarification, and filesystem slices overlap by design; do not sum them into an overall score.", "- A family with zero catalog tasks or no result rows is **not evaluated**, not a 0% result and not evidence of capability.", "- A failed provider/evaluator/setup row is invalid; report it, do not score it as a model refusal or success.", ""])
    return "\n".join(lines)


def _counter(values: Counter) -> str:
    return "; ".join(f"{key}: {value}" for key, value in sorted(values.items())) or "—"


def validate_price_snapshot(prices: dict) -> None:
    if (
        not isinstance(prices, dict)
        or type(prices.get("schema_version")) is not int
        or prices["schema_version"] != 1
        or not isinstance(prices.get("rates"), dict)
    ):
        raise ValueError("price snapshot must contain a rates object")
    try:
        date.fromisoformat(prices.get("as_of", ""))
    except (TypeError, ValueError) as exc:
        raise ValueError("price snapshot as_of must be an ISO date") from exc
    if not isinstance(prices.get("source"), str) or not prices["source"].strip():
        raise ValueError("price snapshot must declare a source")
    if prices.get("currency") != "USD":
        raise ValueError("the scorecard currently accepts USD pricing snapshots only")
    for model, rate in prices["rates"].items():
        if not isinstance(model, str) or not model.strip() or not isinstance(rate, dict):
            raise ValueError(f"price snapshot has invalid rates entry for {model!r}")
        for field in ("input_per_million", "output_per_million"):
            amount = rate.get(field)
            if (
                not isinstance(amount, (int, float))
                or isinstance(amount, bool)
                or not math.isfinite(amount)
                or amount < 0
            ):
                raise ValueError(f"price snapshot has invalid token rates for {model!r}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fqa-suite", nargs=2, action="append", metavar=("TASKS_JSONL", "RESULTS_JSON"), default=[])
    parser.add_argument("--product-suite", nargs=3, action="append", metavar=("FAMILY", "TASKS_JSONL", "RESULTS_JSON"), default=[])
    parser.add_argument("--reviews", type=Path, help="blind human review annotations JSONL")
    parser.add_argument("--prices", type=Path, help="dated JSON provider-rate snapshot; never fetched live")
    parser.add_argument("--baseline", type=Path)
    parser.add_argument("--candidate", type=Path)
    parser.add_argument("--baseline-manifest", type=Path)
    parser.add_argument("--candidate-manifest", type=Path)
    args = parser.parse_args()
    try:
        if not args.fqa_suite and not args.product_suite:
            raise ValueError("supply at least one --fqa-suite or --product-suite")
        if bool(args.baseline) != bool(args.candidate):
            raise ValueError("--baseline and --candidate must be supplied together")
        if args.baseline and (not args.baseline_manifest or not args.candidate_manifest):
            raise ValueError("paired runs require both run manifests")
        sources = []
        task_catalog_paths = []
        for tasks_path, results_path in args.fqa_suite:
            catalog_path = Path(tasks_path)
            task_catalog_paths.append(catalog_path)
            sources.append(("fqa", read_jsonl(catalog_path), read_results(Path(results_path))))
        for family, tasks_path, results_path in args.product_suite:
            if family not in FAMILY_ORDER or family == "filesystem_analytics":
                raise ValueError(f"unsupported product-suite family: {family}")
            catalog_path = Path(tasks_path)
            task_catalog_paths.append(catalog_path)
            sources.append((family, read_jsonl(catalog_path), read_results(Path(results_path))))
        reviews = read_jsonl(args.reviews) if args.reviews else None
        prices = json.loads(args.prices.read_text(encoding="utf-8")) if args.prices else None
        if prices is not None:
            validate_price_snapshot(prices)
        baseline = read_results(args.baseline) if args.baseline else None
        candidate = read_results(args.candidate) if args.candidate else None
        if args.baseline:
            input_fingerprints = compute_fingerprints(task_catalog_paths)
            base_manifest, candidate_manifest = validate_manifest_pair(
                args.baseline_manifest, args.candidate_manifest, args.baseline, args.candidate,
                args.prices, input_fingerprints,
            )
            for label, rows, manifest in (
                ("baseline", baseline, base_manifest), ("candidate", candidate, candidate_manifest)
            ):
                mismatched = sorted({str(row.get("model")) for row in rows if row.get("model") != manifest["model"]})
                if mismatched:
                    raise ValueError(f"{label} result model IDs do not match its run manifest: {', '.join(mismatched)}")
        sys.stdout.write(render(sources, reviews, baseline, candidate, prices))
    except (OSError, ValueError, TypeError, KeyError, json.JSONDecodeError) as exc:
        print(f"Fella scorecard: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""Implementation-independent checks for scorecard accounting and reporting."""
from __future__ import annotations

import importlib.util
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest


MODULE_PATH = Path(__file__).with_name("scorecard.py")
sys.path.insert(0, str(MODULE_PATH.parent))
SPEC = importlib.util.spec_from_file_location("fella_product_scorecard", MODULE_PATH)
assert SPEC and SPEC.loader
scorecard = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(scorecard)


class ScorecardTests(unittest.TestCase):
    def test_product_families_are_selected_from_declared_task_metadata(self) -> None:
        task = {
            "labels": {
                "primary_capability": "forecast_and_scenario",
                "analysis_families": ["temporal_and_predictive", "visualization"],
                "interaction": "clarification_episode",
            }
        }
        self.assertEqual(
            scorecard.families_for_task(task, "fqa"),
            {"filesystem_analytics", "forecasting", "chart_correctness", "clarification"},
        )
        self.assertEqual(scorecard.families_for_task({"id": "web-1"}, "web_research"), {"web_research"})

    def test_invalid_and_missing_rows_stay_visible_and_out_of_accuracy(self) -> None:
        rows = [
            {"id": "a", "correct": True, "correct_rate": 1, "prompt_tok": 100, "completion_tok": 25, "model_calls": 3, "model_call_observed_iterations": 1, "tool_calls": 2, "tool_call_observed_iterations": 1, "err": None},
            {"id": "b", "correct": False, "correct_rate": 0, "err": None},
            {"id": "c", "correct": False, "err": "provider failed"},
        ]
        result = scorecard.summarize(rows, expected_tasks=4)
        self.assertEqual(result["valid"], 2)
        self.assertEqual(result["invalid"], 1)
        self.assertEqual(result["missing"], 1)
        self.assertEqual(result["majority_rate"], 0.5)
        self.assertEqual(result["tokens"], 125)
        self.assertEqual(result["model_calls"], 3)
        self.assertEqual(result["tool_calls"], 2)
        self.assertEqual(result["model_call_coverage"], 0.5)

    def test_row_without_explicit_error_state_is_invalid(self) -> None:
        result = scorecard.summarize([{"id": "a", "correct": True}], expected_tasks=1)
        self.assertEqual(result["valid"], 0)
        self.assertEqual(result["invalid"], 1)
        self.assertIsNone(result["majority_rate"])

    def test_cost_uses_only_supplied_pinned_rates(self) -> None:
        row = {
            "id": "a",
            "model": "provider/model",
            "correct": True,
            "prompt_tok": 1_000_000,
            "completion_tok": 1_000_000,
            "err": None,
        }
        rates = {"rates": {"provider/model": {"input_per_million": 2, "output_per_million": 3}}}
        self.assertEqual(scorecard.summarize([row], 1, rates)["estimated_cost_usd"], 5)
        self.assertIsNone(scorecard.summarize([row], 1)["estimated_cost_usd"])

    def test_empty_families_are_reported_as_not_evaluated(self) -> None:
        tasks = [{"id": "bike-1", "labels": {"primary_capability": "compute_and_compare"}}]
        results = [{"id": "bike-1", "model": "m", "correct": True, "correct_rate": 1, "err": None}]
        rendered = scorecard.render([("fqa", tasks, results)])
        self.assertIn("Web research | m | unknown | 0 | 0 | 0 | 0 | 0 | —", rendered)
        self.assertIn("Hybrid local + external research | m | unknown | 0 | 0 | 0 | 0 | 0 | —", rendered)
        self.assertIn("not evaluated", rendered)
        self.assertIn("false deferral: attach blind-review annotations", rendered)

    def test_scorecard_rows_have_aligned_columns_for_every_family(self) -> None:
        rendered = scorecard.render([("fqa", [], [])])
        rows = [line for line in rendered.splitlines() if line.startswith("| ")]
        header = rows[0]
        expected_columns = len(header.split("|"))
        family_rows = [line for line in rows[1:8]]
        self.assertEqual(len(family_rows), 7)
        self.assertTrue(all(len(line.split("|")) == expected_columns for line in family_rows))

    def test_models_and_profiles_are_never_pooled_for_the_same_task(self) -> None:
        task = {"id": "task-1", "labels": {"primary_capability": "compute_and_compare"}}
        results = [
            {"id": "task-1", "model": "m1", "profile": "fella", "correct": True, "err": None},
            {"id": "task-1", "model": "m1", "profile": "bare", "correct": False, "err": None},
            {"id": "task-1", "model": "m2", "profile": "fella", "correct": False, "err": None},
        ]
        rendered = scorecard.render([("fqa", [task], results)])
        self.assertIn("| Filesystem analytics | m1 | bare | 1 | 1 | 1 | 0 | 0 | 0/1 (0.0%) |", rendered)
        self.assertIn("| Filesystem analytics | m1 | fella | 1 | 1 | 1 | 0 | 0 | 1/1 (100.0%) |", rendered)
        self.assertIn("| Filesystem analytics | m2 | fella | 1 | 1 | 1 | 0 | 0 | 0/1 (0.0%) |", rendered)

    def test_paired_comparison_uses_common_valid_tasks_and_reports_transitions(self) -> None:
        baseline = {
            ("a", "m", "fella"): {"id": "a", "correct": False, "correct_rate": 0, "err": None},
            ("b", "m", "fella"): {"id": "b", "correct": True, "correct_rate": 1, "err": None},
            ("c", "m", "fella"): {"id": "c", "correct": False, "err": "failed"},
            ("e", "m", "fella"): {"id": "e", "correct": True, "correct_rate": 1, "err": None},
        }
        candidate = {
            ("a", "m", "fella"): {"id": "a", "correct": True, "correct_rate": 1, "err": None},
            ("b", "m", "fella"): {"id": "b", "correct": True, "correct_rate": 1, "err": None},
            ("c", "m", "fella"): {"id": "c", "correct": True, "correct_rate": 1, "err": None},
            ("d", "m", "fella"): {"id": "d", "correct": True, "correct_rate": 1, "err": None},
        }
        result = scorecard.comparison_summary(baseline, candidate, {"a", "b", "c", "d", "e"}, "m", "fella")
        self.assertEqual(result["paired"], 2)
        self.assertEqual(result["improved"], 1)
        self.assertEqual(result["unchanged"], 1)
        self.assertEqual(result["invalid_pairs"], 1)
        self.assertEqual(result["baseline_only"], 1)
        self.assertEqual(result["candidate_only"], 1)
        self.assertEqual(result["mean_delta"], 0.5)

    def test_manual_review_is_reported_without_rewriting_machine_grade(self) -> None:
        task = {"id": "bike-1", "labels": {"primary_capability": "compute_and_compare"}}
        result = {"id": "bike-1", "model": "m", "profile": "fella", "correct": False, "correct_rate": 0, "err": None}
        review = {
            "schema_version": 1,
            "benchmark_version": "0.1",
            "run_id": "run-a",
            "task_id": "bike-1",
            "model": "m",
            "profile": "fella",
            "iteration": 1,
            "reviewer_code": "consensus-r1-r2",
            "condition": "candidate",
            "task_expected_behavior": "answer",
            "answer_behavior": "unnecessary_deferral",
            "correctness": "incorrect",
            "evidence_support": "unsupported",
            "unsupported_claims": 1,
            "clarification": "not_applicable",
            "chart_correctness": "not_applicable",
            "forecast_quality": "not_applicable",
            "failure_tags": ["unnecessary_deferral"],
            "adjudication": "agreement",
        }
        rendered = scorecard.render([("fqa", [task], [result])], [review])
        self.assertIn("0/1 (0.0%)", rendered)
        self.assertIn("0/1 (0%) | 1/1 (100%)", rendered)
        self.assertIn("1/1", rendered)

    def test_useful_limit_does_not_count_as_answer_coverage_when_answer_was_expected(self) -> None:
        task = {"id": "task-1", "labels": {"primary_capability": "compute_and_compare"}}
        review = {
            "schema_version": 1, "benchmark_version": "0.1", "run_id": "r",
            "task_id": "task-1", "model": "m", "profile": "fella", "iteration": 1,
            "reviewer_code": "consensus", "condition": "candidate",
            "task_expected_behavior": "answer", "answer_behavior": "useful_limit",
            "correctness": "partially_correct", "evidence_support": "partially_supported",
            "unsupported_claims": 0, "clarification": "not_applicable",
            "chart_correctness": "not_applicable", "forecast_quality": "not_applicable",
            "failure_tags": [], "adjudication": "agreement",
        }
        rendered = scorecard.render([("fqa", [task], [])], [review])
        self.assertIn("| Filesystem analytics | m | fella | candidate | 1 | 0/1 (0%)", rendered)

    def test_review_consensus_row_is_required(self) -> None:
        task = {"id": "bike-1", "labels": {"primary_capability": "compute_and_compare"}}
        review = {
            "schema_version": 1, "benchmark_version": "0.1", "run_id": "r",
            "task_id": "bike-1", "model": "m", "profile": "fella", "iteration": 1,
            "reviewer_code": "r1", "condition": "candidate",
            "task_expected_behavior": "answer",
            "answer_behavior": "answered", "correctness": "correct",
            "evidence_support": "supported", "unsupported_claims": 0,
            "clarification": "not_applicable", "chart_correctness": "not_applicable",
            "forecast_quality": "not_applicable", "failure_tags": [],
            "adjudication": "single_review",
        }
        with self.assertRaisesRegex(ValueError, "not consensus/adjudicated"):
            scorecard.render([("fqa", [task], [])], [review])

    def test_forecast_error_and_interval_coverage_are_calculated_from_reviewed_values(self) -> None:
        task = {
            "id": "forecast-1",
            "labels": {"primary_capability": "forecast_and_scenario"},
        }
        review = {
            "schema_version": 1, "benchmark_version": "0.1", "run_id": "run-a",
            "task_id": "forecast-1", "model": "m", "profile": "fella", "iteration": 1,
            "reviewer_code": "consensus-r1-r2", "condition": "candidate",
            "task_expected_behavior": "estimate",
            "answer_behavior": "answered", "correctness": "correct",
            "evidence_support": "supported", "unsupported_claims": 0,
            "forecast_value": 120, "forecast_observed": 100,
            "forecast_baseline_value": 100,
            "forecast_interval_lower": 90, "forecast_interval_upper": 110,
            "clarification": "not_applicable", "chart_correctness": "not_applicable",
            "forecast_quality": "appropriate_and_calibrated", "failure_tags": [],
            "adjudication": "agreement",
        }
        rendered = scorecard.render([("fqa", [task], [])], [review])
        self.assertIn("Forecast MAE", rendered)
        self.assertIn("20 | 0 | 100%", rendered)

    def test_paired_manifests_require_same_protocol_and_result_hashes(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            result_bytes = b"[]"
            baseline_result = root / "baseline.json"
            candidate_result = root / "candidate.json"
            baseline_result.write_bytes(result_bytes)
            candidate_result.write_bytes(result_bytes)
            common = {
                "schema_version": 1,
                "benchmark_id": "fqa-bench",
                "benchmark_version": "0.1",
                "task_set_sha256": "a" * 64,
                "workspace_set_sha256": "b" * 64,
                "split": "anchor",
                "git_commit": "1234567890abcdef",
                "started_at_utc": "2026-10-06T12:00:00Z",
                "provider": "provider",
                "model": "model",
                "model_version": "pinned-model-version",
                "model_settings": {"temperature": 0},
                "evaluation_protocol_settings": {"max_tool_calls": 20},
                "iterations": 3,
                "pricing_snapshot_sha256": None,
                "result_sha256": hashlib.sha256(result_bytes).hexdigest(),
            }
            base = dict(common, run_id="base", condition="baseline", runner_settings={"harness": "bare"})
            candidate = dict(common, run_id="candidate", condition="candidate", runner_settings={"harness": "fella"})
            base_path = root / "base-manifest.json"
            candidate_path = root / "candidate-manifest.json"
            base_path.write_text(json.dumps(base), encoding="utf-8")
            candidate_path.write_text(json.dumps(candidate), encoding="utf-8")
            scorecard.validate_manifest_pair(base_path, candidate_path, baseline_result, candidate_result)

            # Harness/tooling is the intervention and is expected to differ.
            candidate["runner_settings"] = {"harness": "fella", "toolset_sha256": "c" * 64}
            candidate_path.write_text(json.dumps(candidate), encoding="utf-8")
            scorecard.validate_manifest_pair(base_path, candidate_path, baseline_result, candidate_result)
            with self.assertRaisesRegex(ValueError, "does not match supplied task/workspace files"):
                scorecard.validate_manifest_pair(
                    base_path,
                    candidate_path,
                    baseline_result,
                    candidate_result,
                    input_fingerprints={"task_set_sha256": "a" * 64, "workspace_set_sha256": "d" * 64},
                )

            candidate["model_settings"] = {"temperature": 0.7}
            candidate_path.write_text(json.dumps(candidate), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "model_settings"):
                scorecard.validate_manifest_pair(base_path, candidate_path, baseline_result, candidate_result)
            candidate.pop("model_settings")
            candidate["api_key"] = "test-value"
            candidate_path.write_text(json.dumps(candidate), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "disallowed fields: api_key"):
                scorecard.validate_manifest_pair(base_path, candidate_path, baseline_result, candidate_result)

    def test_paired_manifest_rejects_invalid_hashes_and_repetition_counts(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            result_bytes = b"[]"
            baseline_result = root / "baseline.json"
            candidate_result = root / "candidate.json"
            baseline_result.write_bytes(result_bytes)
            candidate_result.write_bytes(result_bytes)
            common = {
                "schema_version": 1,
                "benchmark_id": "fqa-bench",
                "benchmark_version": "0.1",
                "task_set_sha256": "bad-hash",
                "workspace_set_sha256": "b" * 64,
                "split": "anchor",
                "git_commit": "1234567890abcdef",
                "started_at_utc": "2026-10-06T12:00:00Z",
                "provider": "provider",
                "model": "model",
                "model_version": "pinned-model-version",
                "model_settings": {},
                "evaluation_protocol_settings": {},
                "iterations": 3,
                "pricing_snapshot_sha256": None,
                "result_sha256": hashlib.sha256(result_bytes).hexdigest(),
            }
            base = dict(common, run_id="base", condition="baseline", runner_settings={"harness": "bare"})
            candidate = dict(common, task_set_sha256="a" * 64, run_id="candidate", condition="candidate", runner_settings={"harness": "fella"})
            base_path = root / "base-manifest.json"
            candidate_path = root / "candidate-manifest.json"
            base_path.write_text(json.dumps(base), encoding="utf-8")
            candidate_path.write_text(json.dumps(candidate), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "task_set_sha256"):
                scorecard.validate_manifest_pair(base_path, candidate_path, baseline_result, candidate_result)

            base["task_set_sha256"] = "a" * 64
            base["iterations"] = 2
            base_path.write_text(json.dumps(base), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "fewer than three"):
                scorecard.validate_manifest_pair(base_path, candidate_path, baseline_result, candidate_result)


if __name__ == "__main__":
    unittest.main()

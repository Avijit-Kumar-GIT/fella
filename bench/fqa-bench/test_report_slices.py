#!/usr/bin/env python3
"""Unit tests for FQA-Bench result slicing and interaction metrics."""
from __future__ import annotations

import importlib.util
from pathlib import Path
import unittest


MODULE_PATH = Path(__file__).with_name("report_slices.py")
SPEC = importlib.util.spec_from_file_location("fqa_report_slices", MODULE_PATH)
assert SPEC and SPEC.loader
report = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(report)


class ReportSlicesTests(unittest.TestCase):
    def test_analysis_family_is_a_multi_label_axis(self) -> None:
        task = {"labels": {"analysis_families": ["visualization", "temporal_and_predictive"]}}
        self.assertEqual(
            report.slice_labels(task, "analysis_family"),
            ["temporal_and_predictive", "visualization"],
        )

    def test_episode_and_intermediate_turn_rates_are_separate(self) -> None:
        rows = [
            {
                "err": None,
                "correct": True,
                "correct_rate": 1.0,
                "graded_setup_turn_rates": [1.0, 0.0],
                "prompt_tok": 100,
                "completion_tok": 20,
                "model_calls": 3.0,
                "tool_calls": 2.0,
            },
            {
                "err": "provider error",
                "correct": False,
                "graded_setup_turn_rates": [0.0],
            },
        ]
        result = report.summarize(rows)
        self.assertEqual(result["valid"], 1)
        self.assertEqual(result["invalid"], 1)
        self.assertEqual(result["correct"], 1)
        self.assertEqual(result["intermediate_turns"], 2)
        self.assertEqual(result["intermediate_turn_rate"], 0.5)
        self.assertEqual(result["model_calls"], 3.0)
        self.assertEqual(result["tool_calls"], 2.0)
        self.assertEqual(result["model_call_coverage"], 1.0)
        self.assertEqual(result["tool_call_coverage"], 1.0)

    def test_slice_report_includes_analysis_families_and_turn_quality(self) -> None:
        tasks = [{
            "id": "clarify-1",
            "labels": {
                "domain": "housing_household",
                "primary_capability": "clarify_and_continue",
                "analysis_families": ["clarification_and_continuity"],
                "interaction": "clarification_episode",
                "answerability": "clarification_needed",
                "workspace_scope": "entire_workspace",
                "formats": ["csv"],
                "data_conditions": ["ambiguous_measure"],
            },
        }]
        results = [{
            "id": "clarify-1",
            "model": "test/model",
            "correct": True,
            "correct_rate": 1.0,
            "iters": 1,
            "model_call_observed_iterations": 1,
            "tool_call_observed_iterations": 1,
            "graded_setup_turn_rates": [1.0],
            "prompt_tok": 100,
            "completion_tok": 20,
            "model_calls": 2.0,
            "tool_calls": 3.0,
            "err": None,
        }]
        rendered = report.render(tasks, results, ["analysis_family"])
        self.assertIn("clarification_and_continuity", rendered)
        self.assertIn("Graded turns", rendered)
        self.assertIn("Model calls / episode", rendered)
        self.assertIn("(trace coverage)", rendered)
        self.assertIn("2.0 (100% traced)", rendered)
        self.assertIn("3.0 (100% traced)", rendered)
        self.assertIn("| 1 | 100% |", rendered)

    def test_same_model_profiles_are_reported_separately(self) -> None:
        tasks = [{"id": "t1", "labels": {"domain": "finance"}}]
        results = [
            {"id": "t1", "model": "m", "profile": "bare", "correct": False, "err": None},
            {"id": "t1", "model": "m", "profile": "fella", "correct": True, "err": None},
        ]
        rendered = report.render(tasks, results, ["domain"])
        self.assertIn("| m | bare | finance | 1 | 1 | 0 | 0/1 (0%)", rendered)
        self.assertIn("| m | fella | finance | 1 | 1 | 0 | 1/1 (100%)", rendered)

    def test_duplicate_result_for_same_task_model_profile_is_rejected(self) -> None:
        task = {"id": "t1", "labels": {}}
        row = {"id": "t1", "model": "m", "profile": "fella", "correct": True, "err": None}
        with self.assertRaisesRegex(ValueError, "duplicate result row"):
            report.render([task], [row, dict(row)], ["domain"])


if __name__ == "__main__":
    unittest.main()

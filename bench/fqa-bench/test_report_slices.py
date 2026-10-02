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
            "graded_setup_turn_rates": [1.0],
            "prompt_tok": 100,
            "completion_tok": 20,
            "err": None,
        }]
        rendered = report.render(tasks, results, ["analysis_family"])
        self.assertIn("clarification_and_continuity", rendered)
        self.assertIn("Graded turns", rendered)
        self.assertIn("| 1 | 100% |", rendered)


if __name__ == "__main__":
    unittest.main()

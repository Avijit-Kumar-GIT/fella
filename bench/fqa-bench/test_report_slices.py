"""Small tests for generic FQA tag slicing, independent of any app behavior."""
import unittest

from report_slices import render, slice_labels, summarize


class SliceReportTests(unittest.TestCase):
    def test_multi_label_axes_include_a_case_in_each_applicable_slice(self):
        task = {"labels": {"formats": ["csv", "md"], "data_conditions": ["mixed_formats"]}}
        self.assertEqual(slice_labels(task, "file_format"), ["csv", "md"])
        self.assertEqual(slice_labels(task, "data_condition"), ["mixed_formats"])

    def test_invalid_runs_do_not_count_as_wrong_or_successful(self):
        metrics = summarize([
            {"correct": True, "correct_rate": 1.0, "prompt_tok": 10, "completion_tok": 4, "err": None},
            {"correct": False, "correct_rate": 0.0, "prompt_tok": 20, "completion_tok": 3, "err": None},
            {"correct": False, "correct_rate": 0.0, "prompt_tok": 0, "completion_tok": 0, "err": "provider timeout"},
        ])
        self.assertEqual(metrics["cases"], 3)
        self.assertEqual(metrics["valid"], 2)
        self.assertEqual(metrics["invalid"], 1)
        self.assertEqual(metrics["correct"], 1)
        self.assertEqual(metrics["rate"], 0.5)
        self.assertEqual(metrics["tokens"], 18.5)

    def test_report_joins_case_results_to_capability_and_domain(self):
        tasks = [{"id": "case-1", "labels": {"domain": "public_open_data", "primary_capability": "visualize"}}]
        results = [{"id": "case-1", "model": "provider/model", "correct": True, "correct_rate": 1.0, "prompt_tok": 100, "completion_tok": 10, "err": None}]
        report = render(tasks, results, ["domain", "primary_capability"])
        self.assertIn("public_open_data", report)
        self.assertIn("visualize", report)
        self.assertIn("1/1 (100%)", report)

    def test_unknown_result_ids_fail_instead_of_silently_dropping(self):
        with self.assertRaisesRegex(ValueError, "missing from task catalog"):
            render([], [{"id": "not-in-the-catalog"}], ["domain"])


if __name__ == "__main__":
    unittest.main()

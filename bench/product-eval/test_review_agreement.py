#!/usr/bin/env python3
"""Tests for independent blinded review agreement reporting."""
from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import tempfile
import unittest


MODULE_PATH = Path(__file__).with_name("review_agreement.py")
SPEC = importlib.util.spec_from_file_location("fella_review_agreement", MODULE_PATH)
assert SPEC and SPEC.loader
agreement = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(agreement)


def assessment(reviewer: str, **overrides) -> dict:
    row = {
        "schema_version": 1,
        "packet_id": "blind-packet-1",
        "reviewer_code": reviewer,
        "answer_behavior": "answered",
        "correctness": "correct",
        "evidence_support": "supported",
        "unsupported_claims": 0,
        "clarification": "not_applicable",
        "chart_correctness": "not_applicable",
        "forecast_quality": "not_applicable",
        "failure_tags": [],
    }
    row.update(overrides)
    return row


class ReviewAgreementTests(unittest.TestCase):
    def test_blinded_independent_assessments_report_exact_agreement(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "reviews.jsonl"
            rows = [
                assessment("reviewer-a"),
                assessment("reviewer-b", correctness="incorrect", failure_tags=["calculation_error"]),
            ]
            path.write_text("\n".join(json.dumps(row) for row in rows), encoding="utf-8")
            report = agreement.agreement_report(agreement.read_assessments(path))
            self.assertIn("Packets: 1; reviewer-packet comparisons: 1.", report)
            self.assertIn("| answer_behavior | 1 | 1 | 100.0% | — |", report)
            self.assertIn("| correctness | 1 | 0 | 0.0% | 0.000 |", report)
            self.assertIn("| failure_tags | 1 | 0 | 0.0% | — |", report)

    def test_single_review_is_not_misrepresented_as_interrater_evidence(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "reviews.jsonl"
            path.write_text(json.dumps(assessment("reviewer-a")), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "fewer than two independent reviewers"):
                agreement.read_assessments(path)

    def test_blind_assessment_rejects_condition_or_answer_key_fields(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "reviews.jsonl"
            row_a = assessment("reviewer-a", condition="candidate")
            row_b = assessment("reviewer-b")
            path.write_text("\n".join(json.dumps(row) for row in (row_a, row_b)), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "disallowed"):
                agreement.read_assessments(path)


if __name__ == "__main__":
    unittest.main()

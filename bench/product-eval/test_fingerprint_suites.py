#!/usr/bin/env python3
"""Tests for stable and complete benchmark input fingerprints."""
from __future__ import annotations

import importlib.util
import hashlib
import json
from pathlib import Path
import tempfile
import unittest


MODULE_PATH = Path(__file__).with_name("fingerprint_suites.py")
SPEC = importlib.util.spec_from_file_location("fella_eval_fingerprints", MODULE_PATH)
assert SPEC and SPEC.loader
fingerprints = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(fingerprints)


def workspace_digest(files: dict[str, bytes]) -> str:
    digest = hashlib.sha256()
    for path, contents in sorted(files.items()):
        digest.update(path.encode("utf-8"))
        digest.update(b"\0")
        digest.update(hashlib.sha256(contents).digest())
        digest.update(b"\n")
    return digest.hexdigest()


class FingerprintTests(unittest.TestCase):
    def make_suite(self, parent: Path, scope: str = "entire_workspace") -> tuple[Path, Path]:
        suite = parent / "suite"
        workspace = suite / "workspaces" / "demo"
        (workspace / "notes").mkdir(parents=True)
        contents = {"data.csv": b"day,value\n1,2\n", "notes/readme.md": b"Human note\n"}
        for relative, payload in contents.items():
            target = workspace / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(payload)
        task = {
            "id": "task-1",
            "suite": "fixture-v1",
            "workspace": {
                "id": "workspace-1",
                "path": "workspaces/demo",
                "scope": scope,
                "files": sorted(contents),
                "sha256": workspace_digest(contents),
            },
        }
        catalog = suite / "tasks.jsonl"
        catalog.write_text(json.dumps(task) + "\n", encoding="utf-8")
        return catalog, workspace

    def test_fingerprint_is_stable_content_addressed_and_path_free(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            catalog, _ = self.make_suite(Path(tmp))
            first = fingerprints.compute_fingerprints([catalog])
            second = fingerprints.compute_fingerprints([catalog])
            self.assertEqual(first, second)
            self.assertEqual(first["tasks"], 1)
            self.assertEqual(first["workspace_snapshots"], 1)
            self.assertEqual(first["workspace_files"], 2)
            self.assertEqual(first["workspace_bytes"], len(b"day,value\n1,2\n") + len(b"Human note\n"))
            self.assertNotIn(tmp, json.dumps(first))

    def test_entire_workspace_requires_complete_declared_file_inventory(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            catalog, workspace = self.make_suite(Path(tmp))
            (workspace / "unlisted.csv").write_text("secret,1\n", encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "entire-workspace file list differs"):
                fingerprints.compute_fingerprints([catalog])

    def test_stale_workspace_digest_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            catalog, workspace = self.make_suite(Path(tmp))
            (workspace / "data.csv").write_text("day,value\n1,999\n", encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "workspace sha256 is stale"):
                fingerprints.compute_fingerprints([catalog])

    def test_listed_file_scope_does_not_claim_unmounted_distractors(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            catalog, workspace = self.make_suite(Path(tmp), scope="listed_files")
            (workspace / "unmounted.txt").write_text("not part of this task", encoding="utf-8")
            result = fingerprints.compute_fingerprints([catalog])
            self.assertEqual(result["workspace_files"], 2)


if __name__ == "__main__":
    unittest.main()

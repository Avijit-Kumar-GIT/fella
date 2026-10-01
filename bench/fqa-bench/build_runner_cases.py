#!/usr/bin/env python3
"""Adapt canonical FQA-Bench tasks/answer keys to agent_eval's legacy cases.jsonl."""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import sys


ROOT = Path(__file__).resolve().parents[2]


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


def safe_path(raw: str, what: str) -> PurePosixPath:
    path = PurePosixPath(raw)
    if path.is_absolute() or not path.parts or any(part in ("", ".", "..") for part in path.parts):
        raise ValueError(f"unsafe {what} path: {raw!r}")
    return path


def workspace_hash(workspace: Path, files: list[str]) -> str:
    digest = hashlib.sha256()
    for rel in sorted(files):
        path = safe_path(rel, "workspace file")
        source = workspace.joinpath(*path.parts)
        if not source.is_file():
            raise ValueError(f"missing workspace file: {source}")
        digest.update(rel.encode("utf-8"))
        digest.update(b"\0")
        digest.update(hashlib.sha256(source.read_bytes()).digest())
        digest.update(b"\n")
    return digest.hexdigest()


def render(suite: Path) -> bytes:
    tasks = read_jsonl(suite / "tasks.jsonl")
    keys = read_jsonl(suite / "answer-keys.jsonl")
    task_ids = [task.get("id") for task in tasks]
    key_ids = [key.get("task_id") for key in keys]
    if len(task_ids) != len(set(task_ids)):
        raise ValueError("duplicate task IDs in tasks.jsonl")
    if len(key_ids) != len(set(key_ids)):
        raise ValueError("duplicate task IDs in answer-keys.jsonl")
    if set(task_ids) != set(key_ids):
        raise ValueError("tasks and answer keys must have exactly the same task IDs")

    keys_by_id = {key["task_id"]: key for key in keys}
    generated = []
    for task in tasks:
        task_id = task["id"]
        ws = task["workspace"]
        rel_workspace = safe_path(ws["path"], "workspace")
        workspace = suite.joinpath(*rel_workspace.parts)
        files = ws["files"]
        if not files or len(files) != len(set(files)):
            raise ValueError(f"{task_id}: workspace files must be nonempty and unique")
        for rel in files:
            safe_path(rel, "workspace file")
        if workspace_hash(workspace, files) != ws["sha256"]:
            raise ValueError(f"{task_id}: workspace hash is stale")
        if ws["scope"] == "entire_workspace":
            actual = {
                path.relative_to(workspace).as_posix()
                for path in workspace.rglob("*")
                if path.is_file()
            }
            if actual != set(files):
                raise ValueError(
                    f"{task_id}: entire_workspace inventory mismatch; "
                    f"undeclared={sorted(actual - set(files))}, missing={sorted(set(files) - actual)}"
                )

        key = keys_by_id[task_id]
        expected = task["labels"]["answerability"]
        behavior = key["expected_behavior"]
        if (expected == "unsupported") != (behavior == "unsupported"):
            raise ValueError(f"{task_id}: task answerability disagrees with answer key")
        if "runner_gold" not in key:
            raise ValueError(f"{task_id}: answer key lacks runner_gold compatibility data")

        suite_files = [f"{rel_workspace.as_posix()}/{safe_path(rel, 'workspace file').as_posix()}" for rel in files]
        request = task["request"]
        generated.append({
            "id": task_id,
            "question": request["prompt"],
            "files": suite_files,
            "gold": key["runner_gold"],
            "category": task["labels"]["domain"],
            "tier": task["labels"]["primary_capability"],
            "setup_turns": request.get("prior_turns", []),
        })

    return ("".join(json.dumps(row, ensure_ascii=False, sort_keys=True) + "\n" for row in generated)).encode("utf-8")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--suite-dir", required=True, type=Path)
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--write", action="store_true")
    group.add_argument("--check", action="store_true")
    args = parser.parse_args()
    suite = args.suite_dir.resolve()
    output = suite / "cases.jsonl"
    try:
        rendered = render(suite)
    except (OSError, ValueError, KeyError, TypeError) as exc:
        print(f"FQA adapter: {exc}", file=sys.stderr)
        return 1

    if args.write:
        output.write_bytes(rendered)
        print(f"wrote {output.relative_to(ROOT)} ({rendered.count(bytes([10]))} tasks)")
        return 0
    if not output.is_file() or output.read_bytes() != rendered:
        print(f"FQA adapter: {output.relative_to(ROOT)} is missing or stale; run with --write", file=sys.stderr)
        return 1
    print(f"FQA adapter: current ({rendered.count(bytes([10]))} tasks)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

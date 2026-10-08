#!/usr/bin/env python3
"""Compute reproducible task/workspace fingerprints for evaluation manifests."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import sys


def read_tasks(path: Path) -> list[dict]:
    tasks = []
    for line_no, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        try:
            task = json.loads(line)
        except json.JSONDecodeError as exc:
            raise ValueError(f"{path}:{line_no}: invalid JSON: {exc}") from exc
        if not isinstance(task, dict):
            raise ValueError(f"{path}:{line_no}: task must be a JSON object")
        tasks.append(task)
    if not tasks:
        raise ValueError(f"{path}: task catalog is empty")
    ids = [task.get("id") for task in tasks]
    if any(not isinstance(task_id, str) or not task_id for task_id in ids):
        raise ValueError(f"{path}: every task needs a non-empty string id")
    if len(set(ids)) != len(ids):
        raise ValueError(f"{path}: duplicate task IDs")
    return tasks


def _safe_relative(raw: str, description: str) -> PurePosixPath:
    path = PurePosixPath(raw)
    if (
        path.is_absolute()
        or not path.parts
        or "\\" in raw
        or any(part in {"", ".", ".."} for part in path.parts)
    ):
        raise ValueError(f"unsafe {description} path: {raw!r}")
    return path


def _reject_symlink_components(root: Path, relative: PurePosixPath, description: str) -> Path:
    current = root
    for part in relative.parts:
        current = current / part
        if current.is_symlink():
            raise ValueError(f"{description} path contains a symbolic link: {relative.as_posix()}")
    return current


def _file_hash(path: Path) -> tuple[str, int]:
    before = path.stat()
    if not path.is_file() or path.is_symlink():
        raise ValueError(f"workspace entry is not a regular file: {path.name}")
    digest = hashlib.sha256()
    size = 0
    with path.open("rb") as stream:
        while chunk := stream.read(1024 * 1024):
            digest.update(chunk)
            size += len(chunk)
    after = path.stat()
    if (before.st_size, before.st_mtime_ns) != (after.st_size, after.st_mtime_ns):
        raise ValueError(f"workspace file changed while fingerprinting: {path.name}")
    return digest.hexdigest(), size


def _workspace_inventory(root: Path) -> list[str]:
    inventory = []
    for directory, dirnames, filenames in os.walk(root, followlinks=False):
        current = Path(directory)
        for name in dirnames:
            if (current / name).is_symlink():
                raise ValueError(f"workspace contains a symbolic link: {name}")
        for name in filenames:
            path = current / name
            if path.is_symlink() or not path.is_file():
                raise ValueError(f"workspace contains a non-regular file: {name}")
            inventory.append(path.relative_to(root).as_posix())
    return sorted(inventory)


def _fingerprint_workspace(suite_root: Path, suite_id: str, workspace: dict) -> tuple[dict, int, int]:
    required = ("id", "path", "scope", "files", "sha256")
    missing = [field for field in required if field not in workspace]
    if missing:
        raise ValueError("workspace metadata is missing: " + ", ".join(missing))
    if not all(isinstance(workspace[field], str) and workspace[field] for field in ("id", "path", "scope", "sha256")):
        raise ValueError("workspace id, path, scope, and sha256 must be non-empty strings")
    if workspace["scope"] not in {"entire_workspace", "listed_files"}:
        raise ValueError(f"unsupported workspace scope: {workspace['scope']!r}")
    if not isinstance(workspace["files"], list) or any(not isinstance(item, str) for item in workspace["files"]):
        raise ValueError("workspace files must be an array of relative paths")
    if workspace["scope"] == "entire_workspace" and not workspace["files"]:
        raise ValueError("an entire-workspace task must declare its complete file inventory")
    if len(set(workspace["files"])) != len(workspace["files"]):
        raise ValueError("workspace file inventory contains duplicates")

    relative_root = _safe_relative(workspace["path"], "workspace")
    root = _reject_symlink_components(suite_root, relative_root, "workspace")
    if not root.is_dir():
        raise ValueError(f"workspace directory is missing or unsafe: {workspace['id']}")
    declared_files = sorted(_safe_relative(item, "workspace file").as_posix() for item in workspace["files"])
    if workspace["scope"] == "entire_workspace":
        actual_files = _workspace_inventory(root)
        if actual_files != declared_files:
            missing_files = sorted(set(actual_files) - set(declared_files))
            absent_files = sorted(set(declared_files) - set(actual_files))
            raise ValueError(
                f"{workspace['id']}: entire-workspace file list differs from mounted folder "
                f"(unlisted={len(missing_files)}, missing={len(absent_files)})"
            )

    records = []
    expected_digest = hashlib.sha256()
    total_bytes = 0
    for relative in declared_files:
        source = _reject_symlink_components(root, PurePosixPath(relative), "workspace file")
        if not source.is_file():
            raise ValueError(f"{workspace['id']}: declared file is missing or unsafe: {relative}")
        content_hash, size = _file_hash(source)
        total_bytes += size
        expected_digest.update(relative.encode("utf-8"))
        expected_digest.update(b"\0")
        expected_digest.update(bytes.fromhex(content_hash))
        expected_digest.update(b"\n")
        records.append({"path": relative, "sha256": content_hash, "size": size})
    if expected_digest.hexdigest() != workspace["sha256"]:
        raise ValueError(f"{workspace['id']}: declared workspace sha256 is stale")
    snapshot = {
        "suite": suite_id,
        "workspace_id": workspace["id"],
        "scope": workspace["scope"],
        "files": records,
    }
    return snapshot, len(records), total_bytes


def compute_fingerprints(task_catalogs: list[Path]) -> dict:
    task_records = []
    workspace_snapshots: dict[tuple[str, str, str], dict] = {}
    workspace_cache: dict[tuple[str, str, str, str, tuple[str, ...], str], dict] = {}
    task_count = 0
    for catalog_path in task_catalogs:
        tasks = read_tasks(catalog_path)
        suite_ids = {task.get("suite") for task in tasks}
        if len(suite_ids) != 1 or not isinstance(next(iter(suite_ids)), str) or not next(iter(suite_ids)):
            raise ValueError(f"{catalog_path}: tasks must have one shared non-empty suite identifier")
        suite_id = next(iter(suite_ids))
        catalog_digest = hashlib.sha256(catalog_path.read_bytes()).hexdigest()
        task_records.append({"suite": suite_id, "catalog_sha256": catalog_digest, "task_count": len(tasks)})
        task_count += len(tasks)
        for task in tasks:
            workspace = task.get("workspace")
            if workspace is None:
                continue
            if not isinstance(workspace, dict):
                raise ValueError(f"{task['id']}: workspace metadata must be an object")
            cache_key = (
                suite_id,
                str(workspace.get("id", "")),
                str(workspace.get("path", "")),
                str(workspace.get("scope", "")),
                tuple(sorted(workspace.get("files", []))) if isinstance(workspace.get("files"), list) else (),
                str(workspace.get("sha256", "")),
            )
            snapshot = workspace_cache.get(cache_key)
            if snapshot is None:
                snapshot, _, _ = _fingerprint_workspace(catalog_path.parent, suite_id, workspace)
                workspace_cache[cache_key] = snapshot
            key = (suite_id, snapshot["workspace_id"], snapshot["scope"] + ":" + json.dumps(snapshot["files"], sort_keys=True))
            workspace_snapshots[key] = snapshot

    if len({record["suite"] for record in task_records}) != len(task_records):
        raise ValueError("each task catalog must have a distinct suite identifier")
    task_records.sort(key=lambda record: record["suite"])
    workspace_records = [workspace_snapshots[key] for key in sorted(workspace_snapshots)]
    canonical = lambda value: json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode("utf-8")
    task_hash = hashlib.sha256(canonical(task_records)).hexdigest()
    workspace_hash = hashlib.sha256(canonical(workspace_records)).hexdigest()
    files = sum(len(snapshot["files"]) for snapshot in workspace_records)
    total_bytes = sum(record["size"] for snapshot in workspace_records for record in snapshot["files"])
    return {
        "schema_version": 1,
        "task_set_sha256": task_hash,
        "workspace_set_sha256": workspace_hash,
        "task_catalogs": len(task_records),
        "tasks": task_count,
        "workspace_snapshots": len(workspace_records),
        "workspace_files": files,
        "workspace_bytes": total_bytes,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--suite", type=Path, action="append", required=True, metavar="TASKS_JSONL")
    args = parser.parse_args()
    try:
        sys.stdout.write(json.dumps(compute_fingerprints(args.suite), indent=2) + "\n")
    except (OSError, ValueError, TypeError, json.JSONDecodeError) as exc:
        print(f"Evaluation fingerprint: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

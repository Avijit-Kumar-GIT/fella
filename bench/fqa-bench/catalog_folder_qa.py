#!/usr/bin/env python3
"""Build a provisional tagged crosswalk for the legacy personal-data battery.

The catalog is metadata only: cases.jsonl remains the runner input and the
workspace fixtures remain in bench/folder-qa. These records are development
diagnostics, not the representative FQA-Bench v0.1 corpus. Tags are
provisional because they are inferred from old task tiers.

    python3 bench/fqa-bench/catalog_folder_qa.py --write
    python3 bench/fqa-bench/catalog_folder_qa.py --check
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import sys


ROOT = Path(__file__).resolve().parents[2]
SOURCE_DIR = ROOT / "bench" / "folder-qa"
SOURCE_CASES = SOURCE_DIR / "cases.jsonl"
OUTPUT = ROOT / "bench" / "fqa-bench" / "catalog" / "folder-qa.jsonl"


DOMAIN_MAP = {
    "spending": "personal_finance",
    "subscriptions": "subscriptions",
    "fitness": "health_wellness",
    "sleep": "health_wellness",
    "screen-time": "health_wellness",
    "reading": "reading_media",
    "travel": "travel",
    "housing": "housing_household",
    "contacts": "contacts_relationships",
    "goals": "goals_planning",
    "journal": "cross_domain",
    "general": "general",
}


def read_cases() -> list[dict]:
    cases = []
    for line_no, line in enumerate(SOURCE_CASES.read_text(encoding="utf-8").splitlines(), 1):
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        try:
            cases.append(json.loads(line))
        except json.JSONDecodeError as exc:
            raise ValueError(f"{SOURCE_CASES}:{line_no}: invalid JSON: {exc}") from exc
    return cases


def content_hash(case: dict) -> str:
    """Hash the exact case-scoped files as stable path/content-hash pairs."""
    digest = hashlib.sha256()
    for rel in sorted(case.get("files", [])):
        path = Path(rel)
        if path.is_absolute() or ".." in path.parts:
            raise ValueError(f"{case['id']}: unsafe fixture path {rel!r}")
        source = SOURCE_DIR / path
        if not source.is_file():
            raise ValueError(f"{case['id']}: missing fixture {rel!r}")
        digest.update(rel.encode("utf-8"))
        digest.update(b"\0")
        digest.update(hashlib.sha256(source.read_bytes()).digest())
        digest.update(b"\n")
    return digest.hexdigest()


def primary_capability(case: dict) -> str:
    tier = (case.get("tier") or case.get("category") or "").lower()
    gold = case.get("gold")
    if tier.startswith("clarification-episode"):
        return "clarify_and_continue"
    if tier.startswith("forecast-estimate"):
        return "forecast_and_scenario"
    if tier.startswith("forecast-source-limit"):
        return "explain_evidence_limits"
    if gold in ("notool", "no_tool"):
        return "tool_proportionality"
    if isinstance(gold, dict) and "chart" in gold:
        return "visualize"
    if tier.startswith("chart"):
        return "visualize"
    if tier.startswith("mf-") or case.get("multifile"):
        return "combine_sources"
    if tier.startswith(("text-", "pdf-")):
        return "inspect_and_select"
    return "compute_and_compare"


def supporting_capabilities(case: dict, primary: str) -> list[str]:
    tier = (case.get("tier") or case.get("category") or "").lower()
    support = set()
    if case.get("multifile") or case.get("cluttered"):
        support.add("inspect_and_select")
    if "nonstd-date" in tier or "normalization" in tier or "date-parsing" in tier:
        support.add("interpret_and_normalize")
    if primary == "combine_sources":
        support.add("compute_and_compare")
    if primary == "visualize":
        support.add("compute_and_compare")
    if case.get("setup_turns"):
        support.add("plan_and_decompose")
    if primary == "forecast_and_scenario":
        support.update(("inspect_and_select", "compute_and_compare"))
    if primary == "clarify_and_continue":
        support.update(("inspect_and_select", "interpret_and_normalize", "compute_and_compare"))
    support.discard(primary)
    return sorted(support)


def data_conditions(case: dict) -> list[str]:
    tier = (case.get("tier") or case.get("category") or "").lower()
    formats = set(case.get("formats", []))
    conditions = set()
    if case.get("multifile"):
        conditions.add("cross_file_keys")
    if case.get("cluttered"):
        conditions.add("distractor_files")
    if len(formats) > 1:
        conditions.add("mixed_formats")
    if "nonstd-date" in tier:
        conditions.add("nonstandard_dates")
    if "empty-filter" in tier:
        conditions.add("sparse_or_short_series")
    if tier.startswith("forecast-source-limit"):
        conditions.add("missing_time_dimension")
    if formats & {"md", "txt", "pdf"}:
        conditions.add("unstructured_notes")
    return sorted(conditions)


def answerability(case: dict) -> str:
    gold = case.get("gold")
    tier = (case.get("tier") or "").lower()
    if tier.startswith("clarification-episode"):
        return "clarification_needed"
    if tier.startswith("forecast-estimate"):
        return "answerable_with_assumptions"
    if tier.startswith("forecast-source-limit"):
        return "unsupported"
    if gold in ("notool", "no_tool"):
        return "no_analysis_needed"
    return "answerable"


def catalog_entry(case: dict) -> dict:
    domain = case.get("domain", "general")
    try:
        normalized_domain = DOMAIN_MAP[domain]
    except KeyError as exc:
        raise ValueError(f"{case.get('id')}: unmapped legacy domain {domain!r}") from exc
    setup = case.get("setup_turns", [])
    graded_setup = case.get("graded_setup_turns", [])
    interaction = (
        "clarification_episode" if graded_setup
        else "follow_up" if setup
        else "single_turn"
    )
    files = case.get("files", [])
    primary = primary_capability(case)
    return {
        "schema_version": 1,
        "id": case["id"],
        "suite": "folder-qa-development",
        "tagging_status": "provisional",
        "workspace": {
            "id": "synthetic-person-profile-v1",
            "path": "bench/folder-qa",
            "scope": "listed_files",
            "files": files,
            "sha256": content_hash(case),
            "source_type": "controlled_synthetic",
            "provenance_ref": "bench/folder-qa/gen.py",
        },
        "request": {
            "prompt": graded_setup[0]["question"] if graded_setup else case["question"],
            "prior_turns": setup,
            **({"clarification_reply": case["question"]} if graded_setup else {}),
        },
        "labels": {
            "domain": normalized_domain,
            "primary_capability": primary,
            "supporting_capabilities": supporting_capabilities(case, primary),
            "formats": sorted(set(case.get("formats", []))),
            "data_conditions": data_conditions(case),
            "interaction": interaction,
            "answerability": answerability(case),
            "workspace_scope": "listed_files",
        },
        "answer_key_ref": f"bench/folder-qa/cases.jsonl#{case['id']}",
    }


def render() -> bytes:
    cases = read_cases()
    ids = [case.get("id") for case in cases]
    if len(ids) != len(set(ids)):
        raise ValueError("duplicate task IDs in legacy folder-qa cases.jsonl")
    entries = [catalog_entry(case) for case in cases]
    return ("".join(json.dumps(entry, ensure_ascii=False, sort_keys=True) + "\n" for entry in entries)).encode()


def main() -> int:
    parser = argparse.ArgumentParser()
    group = parser.add_mutually_exclusive_group()
    group.add_argument("--write", action="store_true", help="write the generated catalog")
    group.add_argument("--check", action="store_true", help="fail if the catalog is stale")
    args = parser.parse_args()

    try:
        rendered = render()
    except (OSError, ValueError) as exc:
        print(f"fqa catalog: {exc}", file=sys.stderr)
        return 1

    if args.write:
        OUTPUT.parent.mkdir(parents=True, exist_ok=True)
        OUTPUT.write_bytes(rendered)
        print(f"wrote {OUTPUT.relative_to(ROOT)} ({rendered.count(bytes([10]))} tasks)")
        return 0
    if args.check:
        if not OUTPUT.is_file() or OUTPUT.read_bytes() != rendered:
            print(f"fqa catalog: {OUTPUT.relative_to(ROOT)} is missing or stale; run with --write", file=sys.stderr)
            return 1
        print(f"fqa catalog: current ({rendered.count(bytes([10]))} tasks)")
        return 0
    sys.stdout.buffer.write(rendered)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""Join one or more `agent_eval bench --json` result dumps against their
`cases.jsonl` metadata (question text, tier, domain) into one consolidated
JSON the dashboard artifact reads. stdlib only.

Every future axis's results plug into this same script -- add a
(suite_label, cases_dir, results_json) triple per run.

    python3 bench/rollup.py --out bench/out/results.json \
        "easy=folder-qa:folder-qa/out/gemma-fella.json" \
        "hard=folder-qa-hard:folder-qa-hard/out/gemma-fella.json"
"""
import argparse
import json
import sys
from pathlib import Path


def load_cases(cases_dir: Path) -> dict:
    out = {}
    for line in (cases_dir / "cases.jsonl").read_text().splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        c = json.loads(line)
        out[c["id"]] = c
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True)
    ap.add_argument(
        "runs",
        nargs="+",
        help='"suite_label=cases_dir:results_json" -- one per axis/tier run',
    )
    args = ap.parse_args()

    rows = []
    for spec in args.runs:
        label, rest = spec.split("=", 1)
        cases_dir, results_path = rest.split(":", 1)
        cases = load_cases(Path(cases_dir))
        results = json.loads(Path(results_path).read_text())
        for r in results:
            c = cases.get(r["id"], {})
            rows.append({
                "suite": label,
                "id": r["id"],
                "model": r["model"],
                "question": c.get("question", ""),
                "tier": c.get("tier", "unknown"),
                "domain": c.get("domain", "unknown"),
                "multifile": c.get("multifile", False),
                "cluttered": c.get("cluttered", False),
                "correct": r["correct"],
                "correct_rate": r["correct_rate"],
                "iters": r["iters"],
                "closeness_det": r["closeness_det"],
                "steps": r["steps"],
                "waste": r["waste"],
                "prompt_tok": r["prompt_tok"],
                "completion_tok": r["completion_tok"],
                "total_s": r["total_s"],
                "err": r.get("err"),
            })

    Path(args.out).parent.mkdir(parents=True, exist_ok=True)
    Path(args.out).write_text(json.dumps(rows, indent=None))
    n_ok = sum(1 for r in rows if r["correct"])
    print(f"wrote {len(rows)} rows ({n_ok}/{len(rows)} correct) -> {args.out}", file=sys.stderr)


if __name__ == "__main__":
    main()

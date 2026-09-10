#!/usr/bin/env python3
"""Roll up `agent_eval bench --json` dumps.

Harness-lift comparison (the main use):

    python3 bench/aggregate.py --lift --bare qa-bare.json --fella qa-fella.json [--prices]

  Per model: acc(bare) -> acc(fella), Δacc + a paired bootstrap 95% CI,
  consistency (all-iters-correct), tok/correct, self-catch (wrong answers the
  verifier flagged), and Δ$/correct.

Tidy CSV for plotting (one row per model x harness x case):

    python3 bench/aggregate.py --csv out/results.csv --bare qa-bare.json --fella qa-fella.json

Generic table (any set of dumps):

    python3 bench/aggregate.py "label=path[:model-filter]" ...
"""
import csv
import json
import random
import sys

random.seed(12)  # bootstrap resampling — fixed so CIs are reproducible

# $ / 1M (input, output). Verified 2026-09-09 against OpenRouter's
# /api/v1/models and per-model /endpoints APIs; representative (headline) rate —
# OpenRouter load-balances across providers, so the real spend is the dashboard.
# Models free on an ollama-cloud key are marked FREE and cost $0 here.
PRICES = {
    "gpt-5.6-luna": (0.20, 1.20),          # OpenRouter openai/gpt-5.6-luna (Pro is 1.00/6.00)
    "gpt-5-nano": (0.05, 0.40),
    "gpt-4o-mini": (0.15, 0.60),
    "grok-4.3": (1.25, 2.50),
    "gemini-3.8-flash": (0.75, 3.75),
    "muse-glimmer-30b": (0.30, 1.10),
    "inkling-small": (0.45, 1.20),
    "muse-spark-1.3": (1.25, 4.25),
    "muse-spark-1.3-contributor": (0.10, 0.20),
    "nemotron-3.5-lightning": (0.08, 0.20),
    "deepseek-v4-pro-0813": (1.0494, 3.1482),
    # free on a plain ollama-cloud key (the rest of ollama-cloud is gated):
    "gemma4:31b": (0.0, 0.0),
    "gpt-oss:120b": (0.0, 0.0),
    "gpt-oss:20b": (0.0, 0.0),
    "nemotron-3-nano:30b": (0.0, 0.0),
    # OpenRouter (not free on ollama-cloud despite being listed there). Key is
    # the slug's last "/"-segment: deepseek/deepseek-v4-flash-0731 -> this.
    "deepseek-v4-flash-0731": (0.065, 0.18),
    "glm-5.3-flash": (0.075, 0.25),
}


def stem(m):
    return m.rsplit("/", 1)[-1]


def by_model(rows):
    out = {}
    for r in rows:
        out.setdefault(r["model"], []).append(r)
    return out


def agg(rows):
    n = len(rows)
    ok = sum(1 for r in rows if r["correct"])
    allk = sum(1 for r in rows if r.get("correct_rate", 0) >= 0.999)  # correct on every iter
    caught = sum(1 for r in rows if not r["correct"] and r.get("hard_fail"))
    wrong = n - ok
    tok = sum(r["prompt_tok"] + r["completion_tok"] for r in rows)
    ptok = sum(r["prompt_tok"] for r in rows)
    ctok = sum(r["completion_tok"] for r in rows)
    waste = sum(r["waste"] for r in rows)
    return dict(n=n, ok=ok, allk=allk, caught=caught, wrong=wrong,
               tok=tok, ptok=ptok, ctok=ctok, waste=waste)


def usd_per_correct(a, model):
    p = PRICES.get(stem(model))
    if p is None:
        return None
    return (a["ptok"] * p[0] + a["ctok"] * p[1]) / 1e6 / max(a["ok"], 1)


def bootstrap_dacc_ci(bare_rows, fella_rows, b=2000):
    """Paired bootstrap 95% CI for Δacc = acc(fella) - acc(bare).
    Pairs cases by id; resamples the case list with replacement."""
    bb = {r["id"]: bool(r["correct"]) for r in bare_rows}
    ff = {r["id"]: bool(r["correct"]) for r in fella_rows}
    ids = [i for i in ff if i in bb]
    if not ids:
        return (None, None)
    diffs = []
    for _ in range(b):
        s = [random.choice(ids) for _ in ids]
        d = sum(ff[i] for i in s) / len(s) - sum(bb[i] for i in s) / len(s)
        diffs.append(d)
    diffs.sort()
    return (diffs[int(0.025 * b)], diffs[int(0.975 * b) - 1])


def load_tiers(path="bench/folder-qa/cases.jsonl"):
    out = {}
    try:
        for ln in open(path):
            ln = ln.strip()
            if ln and not ln.startswith("#"):
                o = json.loads(ln)
                out[o["id"]] = o.get("tier", "")
    except OSError:
        pass
    return out


RUNGS = [
    "deepseek-v4-flash-0731", "glm-5.3-flash", "nemotron-3.5-lightning",
    "gemma4:31b", "muse-spark-1.3-contributor", "muse-glimmer-30b",
    "inkling-small", "gemini-3.8-flash", "deepseek-v4-pro-0813",
    "gpt-5.6-luna", "grok-4.3",
]


def rung_of(model):
    s = stem(model)
    return RUNGS.index(s) + 1 if s in RUNGS else 0


def write_csv(out_path, bare_path, fella_path):
    tiers = load_tiers()
    rows = []
    for harness, path in (("bare", bare_path), ("fella", fella_path)):
        if not path:
            continue
        for r in json.load(open(path)):
            rows.append({
                "rung": rung_of(r["model"]), "model": stem(r["model"]),
                "harness": harness, "case": r["id"], "tier": tiers.get(r["id"], ""),
                "correct": int(bool(r["correct"])),
                "correct_rate": round(r.get("correct_rate", 0), 4),
                "closeness_det": round(r.get("closeness_det", 0), 4),
                "waste": r.get("waste", 0),
                "prompt_tok": r.get("prompt_tok", 0),
                "completion_tok": r.get("completion_tok", 0),
                "steps": r.get("steps", 0),
                "hard_fail": int(bool(r.get("hard_fail"))),
                "total_s": round(r.get("total_s", 0), 3),
                "err": r.get("err") or "",
            })
    rows.sort(key=lambda x: (x["rung"], x["harness"], x["case"]))
    cols = ["rung", "model", "harness", "case", "tier", "correct", "correct_rate",
            "closeness_det", "waste", "prompt_tok", "completion_tok", "steps",
            "hard_fail", "total_s", "err"]
    with open(out_path, "w", newline="") as f:
        w = csv.DictWriter(f, cols)
        w.writeheader()
        w.writerows(rows)
    print(f"wrote {len(rows)} rows to {out_path}")


def lift(bare_path, fella_path, prices):
    B = by_model(json.load(open(bare_path)))
    F = by_model(json.load(open(fella_path)))
    hdr = "| model | bare acc | fella acc | Δacc | Δacc 95% CI | fella all-iters | fella tok/correct | bare tok/correct | self-catch"
    sep = "|---|--:|--:|--:|:-:|--:|--:|--:|--:"
    if prices:
        hdr += " | fella $/100-correct | bare $/100-correct"
        sep += "|--:|--:"
    print(hdr + " |")
    print(sep + "|")
    for m in F:
        f = agg(F[m])
        b = agg(B.get(m, []))
        dacc = f["ok"] / f["n"] - (b["ok"] / b["n"] if b["n"] else 0)
        lo, hi = bootstrap_dacc_ci(B.get(m, []), F[m]) if b["n"] else (None, None)
        ci = f"[{lo:+.0%}, {hi:+.0%}]" if lo is not None else "—"
        cells = [
            stem(m),
            f"{b['ok']}/{b['n']}" if b["n"] else "—",
            f"{f['ok']}/{f['n']}",
            f"{dacc:+.0%}",
            ci,
            f"{f['allk']}/{f['n']}",
            f"{f['tok']/max(f['ok'],1):,.0f}",
            f"{b['tok']/max(b['ok'],1):,.0f}" if b["n"] else "—",
            f"{f['caught']}/{f['wrong']}" if f["wrong"] else "0/0",
        ]
        if prices:
            fu, bu = usd_per_correct(f, m), (usd_per_correct(b, m) if b["n"] else None)
            cells += [f"${fu*100:.2f}" if fu is not None else "n/a",
                      f"${bu*100:.2f}" if bu is not None else "n/a"]
        print("| " + " | ".join(cells) + " |")


def generic(specs, prices):
    print("| label | n | acc | close | waste | tok/correct" + (" | $/correct |" if prices else " |"))
    print("|---|--:|:-:|--:|--:|--:" + ("|--:|" if prices else "|"))
    for spec in specs:
        label, rest = spec.split("=", 1)
        path, _, mf = rest.partition(":")
        rows = [r for r in json.load(open(path)) if not mf or r["model"] == mf]
        if not rows:
            print(f"| {label} | 0 | — | | | |")
            continue
        a = agg(rows)
        close = sum(r["closeness_det"] for r in rows) / a["n"]
        cells = [label, str(a["n"]), f"{a['ok']}/{a['n']}", f"{close:.2f}",
                 str(a["waste"]), f"{a['tok']/max(a['ok'],1):,.0f}"]
        if prices:
            u = usd_per_correct(a, rows[0]["model"])
            cells.append(f"${u:.3f}" if u is not None else "n/a")
        print("| " + " | ".join(cells) + " |")


if __name__ == "__main__":
    args = sys.argv[1:]
    if not args:
        sys.exit(__doc__)
    prices = "--prices" in args

    def opt(name):
        return args[args.index(name) + 1] if name in args else None

    if "--csv" in args:
        write_csv(opt("--csv"), opt("--bare"), opt("--fella"))
    elif "--lift" in args:
        lift(opt("--bare"), opt("--fella"), prices)
    else:
        generic([a for a in args if "=" in a], prices)

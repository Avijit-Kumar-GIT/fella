#!/usr/bin/env python3
"""Roll up `agent_eval bench --json` dumps.

Harness-lift comparison (the main use):

    python3 bench/aggregate.py --lift --bare qa-bare.json --fella qa-fella.json [--prices]

  Per model: acc(bare) -> acc(fella), Δacc, consistency (all-iters-correct),
  tok/correct, self-catch (wrong answers the verifier flagged), and Δ$/correct.

Generic table (any set of dumps):

    python3 bench/aggregate.py "label=path[:model-filter]" ...
"""
import json
import sys

# $ / 1M (input, output), Sept-2026 list prices (OpenRouter / provider direct).
# Models free on an ollama-cloud key are marked FREE and cost $0 here.
PRICES = {
    "gpt-5.6-luna": (0.20, 1.20),
    "gpt-5-nano": (0.05, 0.40),
    "gpt-4o-mini": (0.15, 0.60),
    "grok-4.3": (1.25, 2.50),
    "gemini-3.8-flash": (0.75, 3.75),
    "muse-glimmer-30b": (0.30, 1.10),
    "inkling-small": (0.45, 1.20),
    "muse-spark-1.3": (1.25, 4.25),
    "nemotron-3.5-lightning": (0.08, 0.20),
    "deepseek-v4-flash:0731": (0.0, 0.0),  # FREE on ollama-cloud
    "glm-5.3-flash": (0.0, 0.0),           # FREE
    "gemma4:31b": (0.0, 0.0),              # FREE
    "gpt-oss:120b": (0.0, 0.0),            # FREE
    "gpt-oss:20b": (0.0, 0.0),             # FREE
    "deepseek-v4-pro:0813": (0.0, 0.0),    # FREE
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


def lift(bare_path, fella_path, prices):
    B = by_model(json.load(open(bare_path)))
    F = by_model(json.load(open(fella_path)))
    hdr = "| model | bare acc | fella acc | Δacc | fella all-iters | fella tok/correct | bare tok/correct | self-catch"
    sep = "|---|--:|--:|--:|--:|--:|--:|--:"
    if prices:
        hdr += " | fella $/correct | bare $/correct"
        sep += "|--:|--:"
    print(hdr + " |")
    print(sep + "|")
    for m in F:
        f = agg(F[m])
        b = agg(B.get(m, []))
        dacc = f["ok"] / f["n"] - (b["ok"] / b["n"] if b["n"] else 0)
        cells = [
            stem(m),
            f"{b['ok']}/{b['n']}" if b["n"] else "—",
            f"{f['ok']}/{f['n']}",
            f"{dacc:+.0%}",
            f"{f['allk']}/{f['n']}",
            f"{f['tok']/max(f['ok'],1):,.0f}",
            f"{b['tok']/max(b['ok'],1):,.0f}" if b["n"] else "—",
            f"{f['caught']}/{f['wrong']}" if f["wrong"] else "0/0",
        ]
        if prices:
            fu, bu = usd_per_correct(f, m), (usd_per_correct(b, m) if b["n"] else None)
            cells += [f"${fu:.3f}" if fu is not None else "n/a",
                      f"${bu:.3f}" if bu is not None else "n/a"]
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
    if "--lift" in args:
        bare = args[args.index("--bare") + 1]
        fella = args[args.index("--fella") + 1]
        lift(bare, fella, prices)
    else:
        generic([a for a in args if "=" in a], prices)

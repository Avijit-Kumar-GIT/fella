#!/usr/bin/env python3
"""Roll up `agent_eval bench --json` dumps into one comparison table.

    python3 bench/aggregate.py \
        "Fella / gemma4:31b=/tmp/fqa-gemma.json" \
        "Fella / grok-4.3=/tmp/fqa-paid-fella.json:xai/grok-4.3" \
        "code_interpreter / luna=/tmp/fqa-ci-luna.json"

Each arg is `LABEL=PATH` or `LABEL=PATH:MODEL_FILTER` (a bench --json can hold
several models). Prints a markdown table: accuracy, mean closeness, waste,
tokens/correct, and — with --prices — $/100 and $/correct.
"""
import json
import sys

# $ / 1M (input, output), Sept-2026 list prices; keep in sync with agent_eval.rs
PRICES = {
    "gpt-5.6-luna": (0.20, 1.20),
    "gpt-5.6-terra": (2.00, 12.00),
    "gpt-5-nano": (0.05, 0.40),
    "gpt-4o-mini": (0.15, 0.60),
    "grok-4.3": (1.25, 2.50),
}


def load(spec):
    label, rest = spec.split("=", 1)
    path, _, mfilter = rest.partition(":")
    rows = json.load(open(path))
    if mfilter:
        rows = [r for r in rows if r["model"] == mfilter]
    return label, rows


def stem(m):
    return m.rsplit("/", 1)[-1]


def main(args):
    show_prices = "--prices" in args
    specs = [a for a in args if "=" in a]
    print("| harness / model | n | acc | acc% | close | waste | tok/correct |"
          + (" $/100 | $/correct |" if show_prices else ""))
    print("|---|--:|:-:|--:|--:|--:|--:|" + ("--:|--:|" if show_prices else ""))
    for spec in specs:
        label, rows = load(spec)
        n = len(rows)
        if n == 0:
            print(f"| {label} | 0 | — | | | | |")
            continue
        ok = sum(1 for r in rows if r["correct"])
        close = sum(r["closeness_det"] for r in rows) / n
        waste = sum(r["waste"] for r in rows)
        tok = sum(r["prompt_tok"] + r["completion_tok"] for r in rows)
        tpc = tok / max(ok, 1)
        cells = [
            label, str(n), f"{ok}/{n}", f"{100*ok/n:.0f}%",
            f"{close:.2f}", str(waste), f"{tpc:,.0f}",
        ]
        if show_prices:
            m = stem(rows[0]["model"])
            if m in PRICES:
                pin, pout = PRICES[m]
                pt = sum(r["prompt_tok"] for r in rows)
                ct = sum(r["completion_tok"] for r in rows)
                usd = (pt * pin + ct * pout) / 1e6
                cells += [f"${usd/n*100:.2f}", f"${usd/max(ok,1):.3f}"]
            else:
                cells += ["n/a", "n/a"]
        print("| " + " | ".join(cells) + " |")


if __name__ == "__main__":
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    main(sys.argv[1:])

#!/usr/bin/env python3
"""Turn a folder-qa bench run into plottable artifacts. stdlib only — no
matplotlib / pandas. Run after `aggregate.py --csv`:

    python3 bench/aggregate.py --csv bench/folder-qa/out/results.csv \
            --bare bench/folder-qa/out/bare.json --fella bench/folder-qa/out/fella.json
    python3 bench/chart.py bench/folder-qa/out/results.csv [--out bench/folder-qa/out]

Writes, next to results.csv:
  summary.csv  one row per model: acc bare/fella, Δacc + 95% CI, consistency,
               tok/correct x2, $/correct x2, self-catch, avg steps / waste.
  lift.html    self-contained inline-SVG report (Δacc, tok/correct, $/correct)
               + the summary table. No JS, no external assets.
"""
import csv
import html
import sys
from collections import defaultdict

sys.path.insert(0, __file__.rsplit("/", 1)[0])
from aggregate import PRICES, RUNGS, bootstrap_dacc_ci, stem  # noqa: E402


def load(path):
    with open(path) as f:
        return list(csv.DictReader(f))


def to_pairs(rows):
    """{model: {harness: [ {id, correct, prompt_tok, ...}, ... ]}}"""
    out = defaultdict(lambda: defaultdict(list))
    for r in rows:
        out[r["model"]][r["harness"]].append({
            "id": r["case"],
            "correct": r["correct"] == "1",
            "correct_rate": float(r["correct_rate"]),
            "hard_fail": r["hard_fail"] == "1",
            "prompt_tok": int(r["prompt_tok"]),
            "completion_tok": int(r["completion_tok"]),
            "steps": int(r["steps"]),
            "waste": int(r["waste"]),
        })
    return out


def acc(rs):
    return sum(x["correct"] for x in rs) / len(rs) if (rs := rs) else 0.0


def tok_per_correct(rs):
    ok = sum(x["correct"] for x in rs) or 1
    return sum(x["prompt_tok"] + x["completion_tok"] for x in rs) / ok


def usd_per_correct(rs, model):
    p = PRICES.get(stem(model))
    if p is None:
        return None
    ok = sum(x["correct"] for x in rs) or 1
    pt = sum(x["prompt_tok"] for x in rs)
    ct = sum(x["completion_tok"] for x in rs)
    return (pt * p[0] + ct * p[1]) / 1e6 / ok


def summarize(data):
    out = []
    for model, hs in data.items():
        b, f = hs.get("bare", []), hs.get("fella", [])
        if not f:
            continue
        lo, hi = bootstrap_dacc_ci(
            [{"id": x["id"], "correct": x["correct"]} for x in b],
            [{"id": x["id"], "correct": x["correct"]} for x in f],
        ) if b else (None, None)
        row = {
            "rung": RUNGS.index(stem(model)) + 1 if stem(model) in RUNGS else 99,
            "model": stem(model),
            "n": len(f),
            "bare_acc": acc(b) if b else None,
            "fella_acc": acc(f),
            "dacc": (acc(f) - acc(b)) if b else None,
            "dacc_lo": lo, "dacc_hi": hi,
            "consistency": sum(x["correct_rate"] >= 0.999 for x in f) / len(f),
            "tokpc_bare": tok_per_correct(b) if b else None,
            "tokpc_fella": tok_per_correct(f),
            "usdpc_bare": usd_per_correct(b, model) if b else None,
            "usdpc_fella": usd_per_correct(f, model),
            "selfcatch": (sum(x["hard_fail"] and not x["correct"] for x in f)
                          / max(sum(not x["correct"] for x in f), 1)),
            "avg_steps": sum(x["steps"] for x in f) / len(f),
            "avg_waste": sum(x["waste"] for x in f) / len(f),
        }
        out.append(row)
    out.sort(key=lambda r: r["rung"])
    return out


def write_summary_csv(rows, path):
    cols = ["rung", "model", "n", "bare_acc", "fella_acc", "dacc", "dacc_lo",
            "dacc_hi", "consistency", "tokpc_bare", "tokpc_fella", "usdpc_bare",
            "usdpc_fella", "selfcatch", "avg_steps", "avg_waste"]
    with open(path, "w", newline="") as fh:
        w = csv.DictWriter(fh, cols)
        w.writeheader()
        for r in rows:
            w.writerow({k: ("" if r[k] is None else
                            round(r[k], 4) if isinstance(r[k], float) else r[k])
                        for k in cols})
    print(f"wrote {path}")


# --- inline SVG ------------------------------------------------------------

W, ROW, PAD_L, PAD_R, PAD_T = 720, 30, 190, 70, 34


def _bars(title, items, vmin, vmax, fmt, colors, zero=True):
    """items: [(label, value, colorkey, (lo,hi) or None)]. One horizontal bar each."""
    h = PAD_T + ROW * len(items) + 14
    plot_w = W - PAD_L - PAD_R
    span = (vmax - vmin) or 1.0

    def x(v):
        return PAD_L + (v - vmin) / span * plot_w

    p = [f'<svg viewBox="0 0 {W} {h}" xmlns="http://www.w3.org/2000/svg" '
         f'font-family="ui-sans-serif,system-ui,sans-serif" font-size="12">']
    p.append(f'<text x="12" y="20" font-size="13" font-weight="600" '
             f'fill="#111">{html.escape(title)}</text>')
    if zero and vmin < 0 < vmax:
        zx = x(0)
        p.append(f'<line x1="{zx:.1f}" y1="{PAD_T-6}" x2="{zx:.1f}" y2="{h-14}" '
                 f'stroke="#999" stroke-dasharray="3 3"/>')
    for i, (label, v, ck, ci) in enumerate(items):
        y = PAD_T + i * ROW
        base = x(0) if (vmin < 0 < vmax) else PAD_L
        x2 = x(v)
        bx, bw = (min(base, x2), abs(x2 - base))
        p.append(f'<text x="{PAD_L-8}" y="{y+ROW/2+4:.0f}" text-anchor="end" '
                 f'fill="#333">{html.escape(label)}</text>')
        p.append(f'<rect x="{bx:.1f}" y="{y+5:.0f}" width="{max(bw,1):.1f}" '
                 f'height="{ROW-12}" rx="2" fill="{colors[ck]}"/>')
        if ci and ci[0] is not None:
            l, r = x(ci[0]), x(ci[1])
            cy = y + ROW / 2
            p.append(f'<line x1="{l:.1f}" y1="{cy:.1f}" x2="{r:.1f}" y2="{cy:.1f}" '
                     f'stroke="#111" stroke-width="1.2"/>')
            for xx in (l, r):
                p.append(f'<line x1="{xx:.1f}" y1="{cy-4:.1f}" x2="{xx:.1f}" '
                         f'y2="{cy+4:.1f}" stroke="#111" stroke-width="1.2"/>')
        p.append(f'<text x="{x2 + (6 if x2 >= base else -6):.1f}" '
                 f'y="{y+ROW/2+4:.0f}" fill="#111" '
                 f'text-anchor="{"start" if x2 >= base else "end"}">{fmt(v)}</text>')
    p.append("</svg>")
    return "\n".join(p)


def _grouped(title, models, series, fmt, colors):
    """series: [(name, {model: value}, colorkey)]. Bars grouped per model."""
    vmax = (max((v for _, d, _ in series for v in d.values() if v is not None),
                default=0.0) or 1.0) * 1.15
    n = len(series)
    rh = 14
    h = PAD_T + ROW * len(models) + 20
    plot_w = W - PAD_L - PAD_R
    p = [f'<svg viewBox="0 0 {W} {h}" xmlns="http://www.w3.org/2000/svg" '
         f'font-family="ui-sans-serif,system-ui,sans-serif" font-size="12">']
    p.append(f'<text x="12" y="20" font-size="13" font-weight="600" fill="#111">'
             f'{html.escape(title)}</text>')
    for j, (name, _, ck) in enumerate(series):
        p.append(f'<rect x="{PAD_L + j*90}" y="6" width="10" height="10" '
                 f'fill="{colors[ck]}"/>')
        p.append(f'<text x="{PAD_L + j*90 + 14}" y="15" fill="#333">'
                 f'{html.escape(name)}</text>')
    for i, m in enumerate(models):
        y0 = PAD_T + i * ROW
        p.append(f'<text x="{PAD_L-8}" y="{y0+ROW/2+4:.0f}" text-anchor="end" '
                 f'fill="#333">{html.escape(m)}</text>')
        for j, (_, d, ck) in enumerate(series):
            v = d.get(m)
            if v is None:
                continue
            bw = v / vmax * plot_w
            yy = y0 + 3 + j * (rh + 1)
            p.append(f'<rect x="{PAD_L}" y="{yy:.0f}" width="{max(bw,1):.1f}" '
                     f'height="{rh}" rx="2" fill="{colors[ck]}"/>')
            p.append(f'<text x="{PAD_L + bw + 5:.1f}" y="{yy+rh-2:.0f}" '
                     f'fill="#111">{fmt(v)}</text>')
    p.append("</svg>")
    return "\n".join(p)


def build_html(rows):
    C = {"pos": "#2a7", "neg": "#d55", "bare": "#bb5", "fella": "#47c",
         "muted": "#9aa"}
    labels = [r["model"] for r in rows]

    dacc_items = [
        (r["model"], (r["dacc"] or 0.0) * 100, "pos" if (r["dacc"] or 0) >= 0 else "neg",
         ((r["dacc_lo"] * 100, r["dacc_hi"] * 100)
          if r["dacc_lo"] is not None else None))
        for r in rows
    ]
    dvals = [v for _, v, _, _ in dacc_items] + \
            [c for _, _, _, ci in dacc_items if ci for c in ci]
    p1 = _bars("Δaccuracy from the Fella loop (fella − bare), % of 64 cases, 95% CI",
               dacc_items, min(dvals + [0]) - 4, max(dvals + [0]) + 6,
               lambda v: f"{v:+.0f}", C)

    p2 = _grouped(
        "Tokens per correct answer",
        labels,
        [("bare", {r["model"]: r["tokpc_bare"] for r in rows}, "bare"),
         ("fella", {r["model"]: r["tokpc_fella"] for r in rows}, "fella")],
        lambda v: f"{v:,.0f}", C)

    priced = [r for r in rows if r["usdpc_fella"] is not None]
    p3 = _grouped(
        "USD per 100 correct answers (list price; $0 = free tier)",
        [r["model"] for r in priced],
        [("bare", {r["model"]: (r["usdpc_bare"] or 0) * 100 for r in priced}, "bare"),
         ("fella", {r["model"]: (r["usdpc_fella"] or 0) * 100 for r in priced}, "fella")],
        lambda v: f"${v:.2f}", C)

    def pct(v):
        return "—" if v is None else f"{v*100:.0f}%"

    def num(v, f="{:.0f}"):
        return "—" if v is None else f.format(v)

    trows = ""
    for r in rows:
        ci = ("—" if r["dacc_lo"] is None
              else f"[{r['dacc_lo']*100:+.0f}, {r['dacc_hi']*100:+.0f}]")
        trows += (
            "<tr>"
            f"<td>{r['rung']}</td><td>{html.escape(r['model'])}</td>"
            f"<td>{pct(r['bare_acc'])}</td><td>{pct(r['fella_acc'])}</td>"
            f"<td><b>{'—' if r['dacc'] is None else f'{r['dacc']*100:+.0f}%'}</b></td>"
            f"<td>{ci}</td>"
            f"<td>{pct(r['consistency'])}</td>"
            f"<td>{num(r['tokpc_bare'], '{:,.0f}')}</td>"
            f"<td>{num(r['tokpc_fella'], '{:,.0f}')}</td>"
            f"<td>{'—' if r['usdpc_fella'] is None else f'${r['usdpc_fella']*100:.2f}'}</td>"
            f"<td>{pct(r['selfcatch'])}</td>"
            f"<td>{r['avg_steps']:.1f}</td><td>{r['avg_waste']:.2f}</td>"
            "</tr>"
        )

    return f"""<!doctype html><html><head><meta charset="utf-8">
<title>Folder-QA harness lift</title><style>
body{{font:14px ui-sans-serif,system-ui,sans-serif;margin:24px;max-width:820px;color:#111}}
h1{{font-size:19px}} .sub{{color:#667;margin:-6px 0 18px}}
svg{{border:1px solid #eee;border-radius:8px;margin:10px 0;background:#fff;width:100%;height:auto}}
table{{border-collapse:collapse;width:100%;font-size:12.5px;margin-top:10px}}
th,td{{border:1px solid #e3e3e3;padding:4px 7px;text-align:right}}
th:nth-child(2),td:nth-child(2){{text-align:left}}
thead th{{background:#f6f6f6}}
</style></head><body>
<h1>Does the Fella loop lift a cheap model at reading a folder?</h1>
<div class="sub">64-case folder-QA battery · bare (files in the prompt, no tools) vs
the full Fella loop · 3 iterations/case, strict-majority correct.</div>
{p1}{p2}{p3}
<table><thead><tr>
<th>#</th><th>model</th><th>bare</th><th>fella</th><th>Δacc</th><th>95% CI</th>
<th>3/3</th><th>tok/ok bare</th><th>tok/ok fella</th><th>$/100ok fella</th>
<th>self-catch</th><th>steps</th><th>waste</th>
</tr></thead><tbody>{trows}</tbody></table>
</body></html>"""


def main():
    args = sys.argv[1:]
    src = next((a for a in args if not a.startswith("--")), None)
    out_dir = (args[args.index("--out") + 1] if "--out" in args
               else src.rsplit("/", 1)[0] if "/" in src else ".")
    rows = summarize(to_pairs(load(src)))
    write_summary_csv(rows, f"{out_dir}/summary.csv")
    with open(f"{out_dir}/lift.html", "w") as fh:
        fh.write(build_html(rows))
    print(f"wrote {out_dir}/lift.html  ({len(rows)} models)")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Difficulty & domain breakdown of a folder-QA run. stdlib only.

    python3 bench/taxonomy.py bench/folder-qa/out/results.csv [--out bench/folder-qa/out]

Joins the per-case results (from `aggregate.py --csv`) with the battery
taxonomy (`domain`, `tier`, `multifile`, `cluttered`, `formats` in
`cases.jsonl`) and writes, next to results.csv:

  by_case.csv    one row per case, hardest first — the difficulty ledger
  by_tier.csv    accuracy per task shape (bare vs fella)
  by_domain.csv  accuracy per life-data domain
  taxonomy.md    a readable report (also printed)

Difficulty is *measured* (mean accuracy across models), never assigned — so
it stays comparable as the battery gets harder and the harness improves.
"""
import csv
import json
import sys
from collections import defaultdict

CASES = "bench/folder-qa/cases.jsonl"


def load_cases(path=CASES):
    out = {}
    for ln in open(path):
        ln = ln.strip()
        if ln and not ln.startswith("#"):
            o = json.loads(ln)
            out[o["id"]] = o
    return out


def load_results(path):
    rows = list(csv.DictReader(open(path)))
    for r in rows:
        r["correct"] = r["correct"] == "1"
        r["closeness_det"] = float(r["closeness_det"])
    return rows


def mean(xs):
    xs = list(xs)
    return sum(xs) / len(xs) if xs else 0.0


def pct(x):
    return f"{x * 100:.0f}%"


def main():
    args = sys.argv[1:]
    src = next(a for a in args if not a.startswith("--"))
    out = args[args.index("--out") + 1] if "--out" in args else src.rsplit("/", 1)[0]
    cases = load_cases()
    rows = load_results(src)

    # same model set on both sides (fella side already excludes unmeasurable
    # models, e.g. nemotron); restrict bare to that set for a fair delta.
    fella_models = {r["model"] for r in rows if r["harness"] == "fella"}
    rows = [r for r in rows if r["model"] in fella_models]

    per = defaultdict(lambda: {"bare": [], "fella": []})
    for r in rows:
        per[r["case"]][r["harness"]].append(r)

    # ---- by_case ----------------------------------------------------
    bc = []
    for cid, hs in per.items():
        c = cases.get(cid, {})
        b_ok, f_ok = mean(x["correct"] for x in hs["bare"]), mean(x["correct"] for x in hs["fella"])
        bc.append({
            "case": cid,
            "domain": c.get("domain", ""),
            "tier": c.get("tier", ""),
            "multifile": int(bool(c.get("multifile"))),
            "cluttered": int(bool(c.get("cluttered"))),
            "n_files": c.get("n_files", ""),
            "formats": "|".join(c.get("formats", [])),
            "n_models": len(hs["fella"]),
            "bare_acc": round(b_ok, 3),
            "fella_acc": round(f_ok, 3),
            "delta": round(f_ok - b_ok, 3),
            "bare_close": round(mean(x["closeness_det"] for x in hs["bare"]), 3),
            "fella_close": round(mean(x["closeness_det"] for x in hs["fella"]), 3),
            "question": c.get("question", ""),
        })
    bc.sort(key=lambda r: (r["fella_acc"], r["bare_acc"]))
    _csv(f"{out}/by_case.csv", bc, drop=["question"])

    # ---- misses: one row per wrong fella answer -------------------
    miss = []
    for cid, hs in per.items():
        c = cases.get(cid, {})
        for r in hs["fella"]:
            if r["correct"]:
                continue
            if r.get("err"):
                mode = "err"
            elif c.get("tier") == "refusal":
                mode = "refusal-fabrication"
            else:
                mode = "value-error"  # valid query, wrong number (verify can't see it)
            miss.append({
                "case": cid, "model": r["model"], "domain": c.get("domain", ""),
                "tier": c.get("tier", ""), "closeness": round(r["closeness_det"], 2),
                "hard_fail": r.get("hard_fail", ""), "mode": mode,
                "question": c.get("question", ""),
            })
    miss.sort(key=lambda r: (r["mode"], r["case"]))
    _csv(f"{out}/misses.csv", miss)

    # ---- grouped cuts --------------------------------------------------
    def group(keyfn):
        g = defaultdict(lambda: {"bare": [], "fella": [], "n": 0})
        for cid, hs in per.items():
            k = keyfn(cases.get(cid, {}))
            g[k]["n"] += 1
            g[k]["bare"] += [x["correct"] for x in hs["bare"]]
            g[k]["fella"] += [x["correct"] for x in hs["fella"]]
        out_rows = []
        for k, v in g.items():
            b, f = mean(v["bare"]), mean(v["fella"])
            out_rows.append({"key": k, "n_cases": v["n"], "bare_acc": round(b, 3),
                             "fella_acc": round(f, 3), "delta": round(f - b, 3)})
        out_rows.sort(key=lambda r: r["fella_acc"])
        return out_rows

    by_tier = [{"tier": r["key"], **{k: v for k, v in r.items() if k != "key"}}
               for r in group(lambda c: c.get("tier", "?"))]
    by_domain = [{"domain": r["key"], **{k: v for k, v in r.items() if k != "key"}}
                 for r in group(lambda c: c.get("domain", "?"))]
    _csv(f"{out}/by_tier.csv", by_tier)
    _csv(f"{out}/by_domain.csv", by_domain)

    cuts = []
    for label, keyfn in [
        ("single-file", lambda c: not c.get("multifile")),
        ("multi-file", lambda c: bool(c.get("multifile"))),
        ("clean folder", lambda c: not c.get("cluttered")),
        ("cluttered folder", lambda c: bool(c.get("cluttered"))),
        ("plain csv/txt only", lambda c: set(c.get("formats", [])) <= {"csv", "txt"}),
        ("uses tsv/json/jsonl/md/xlsx/pdf", lambda c: bool(set(c.get("formats", [])) - {"csv", "txt"})),
        ("touches xlsx or pdf", lambda c: bool({"xlsx", "pdf"} & set(c.get("formats", [])))),
    ]:
        sel = [cid for cid in per if keyfn(cases.get(cid, {}))]
        b = mean(x["correct"] for cid in sel for x in per[cid]["bare"])
        f = mean(x["correct"] for cid in sel for x in per[cid]["fella"])
        cuts.append({"cut": label, "n_cases": len(sel), "bare_acc": round(b, 3),
                     "fella_acc": round(f, 3), "delta": round(f - b, 3)})
    _csv(f"{out}/by_cut.csv", cuts)

    # ---- report -----------------------------------------------------
    md = []
    md.append("## Task taxonomy & measured difficulty\n")
    md.append(f"_{len(per)} cases · {len(fella_models)} models each side · "
              "difficulty = mean accuracy across models (measured, not assigned)._\n")

    md.append("\n### By life-data domain\n")
    md.append("| domain | cases | bare | fella | Δ |\n|---|--:|--:|--:|--:|")
    for r in by_domain:
        md.append(f"| {r['domain']} | {r['n_cases']} | {pct(r['bare_acc'])} | "
                  f"{pct(r['fella_acc'])} | {r['delta'] * 100:+.0f} |")

    md.append("\n### By task shape (`tier`)\n")
    md.append("| tier | cases | bare | fella | Δ |\n|---|--:|--:|--:|--:|")
    for r in by_tier:
        md.append(f"| {r['tier']} | {r['n_cases']} | {pct(r['bare_acc'])} | "
                  f"{pct(r['fella_acc'])} | {r['delta'] * 100:+.0f} |")

    md.append("\n### Structural cuts\n")
    md.append("| cut | cases | bare | fella | Δ |\n|---|--:|--:|--:|--:|")
    for r in cuts:
        md.append(f"| {r['cut']} | {r['n_cases']} | {pct(r['bare_acc'])} | "
                  f"{pct(r['fella_acc'])} | {r['delta'] * 100:+.0f} |")

    hard = [r for r in bc if r["fella_acc"] < 0.9][:15]
    md.append(f"\n### Hardest cases (fella acc < 90%, {len(hard)} of {len(bc)})\n")
    md.append("| case | domain | tier | bare | fella | question |\n|---|---|---|--:|--:|---|")
    for r in hard:
        q = r["question"][:90] + ("…" if len(r["question"]) > 90 else "")
        md.append(f"| {r['case']} | {r['domain']} | {r['tier']} | {pct(r['bare_acc'])} "
                  f"| {pct(r['fella_acc'])} | {q} |")

    easy = [r for r in bc if r["fella_acc"] == 1.0 and r["bare_acc"] == 1.0]
    md.append(f"\n_{len(easy)} cases are saturated (bare 100% and fella 100%) — "
              "retire or harden these in the next battery._\n")

    from collections import Counter
    mc = Counter(m["mode"] for m in miss)
    vg = Counter(m["model"].rsplit("/", 1)[-1] for m in miss if m["mode"] == "value-error")
    md.append(f"\n### Wrong `fella` answers ({len(miss)} of "
              f"{sum(len(h['fella']) for h in per.values())})\n")
    md.append("| mode | n | note |\n|---|--:|---|")
    md.append(f"| refusal-fabrication | {mc['refusal-fabrication']} | computed a "
              "forecast instead of declining (`fqa-refusal`) |")
    top = vg.most_common(1)[0] if vg else ("—", 0)
    md.append(f"| value-error | {mc['value-error']} | valid query, wrong number; "
              f"`verify` blind. {top[1]} of {mc['value-error']} are one model "
              f"(`{top[0]}`) |")
    if mc["err"]:
        md.append(f"| err | {mc['err']} | endpoint / parse failure |")
    md.append("\nFull list with model + question: `misses.csv`.\n")

    report = "\n".join(md) + "\n"
    open(f"{out}/taxonomy.md", "w").write(report)
    print(report)
    print(f"wrote {out}/by_case.csv, by_tier.csv, by_domain.csv, by_cut.csv, taxonomy.md")


def _csv(path, rows, drop=()):
    if not rows:
        return
    cols = [k for k in rows[0] if k not in drop]
    with open(path, "w", newline="") as f:
        w = csv.DictWriter(f, cols, extrasaction="ignore")
        w.writeheader()
        w.writerows(rows)


if __name__ == "__main__":
    main()

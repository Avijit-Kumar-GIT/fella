#!/usr/bin/env python3
"""Analysis-depth battery: tests `depth_rule` (agent.rs) directly, not
folder-QA retrieval. `depth_rule` tells the model to check the shape of the
data before answering a change/trend/correlation/comparison question --
decompose a change instead of just stating the total, say whether a
correlation actually holds, and say plainly when a correlation has too few
points (under ~8) to trust. Before this tier, nothing in bench/ exercised
that rule at all (`grep -rl "depth_rule\\|decompos\\|correlat" bench/` found
nothing outside src/).

Files are byte-identical copies from ../folder-qa-hard/ (same precedent that
dir set copying from ../folder-qa/) -- this tier needs no new fixture data,
the existing life-domain files already have the right shapes: spend.csv for
decomposition, screen_time.tsv x sleep.jsonl for a correlation with plenty
of points, books.csv (finished-only slice) for a correlation with too few.

Correlation strength/direction convention (stated once, here, so every case
built from it is self-documenting): |r| < 0.3 weak, 0.3-0.7 moderate,
> 0.7 strong; sign gives positive/negative. `pearsonr` below is a literal
copy of the one `run_python` injects (pyexec.rs STATS_HELPERS) so gold
matches what the engine itself would compute, not a second implementation
that could quietly drift.

Every gold value is computed from the copied files, not hand-typed.

Explicit non-goal: `depth_rule` also says to *lead* with the plain-language
finding before the numbers. This tier does not grade prose order --
substring matching can't tell "led with" from "mentioned somewhere", and
GOALS.md names decomposition/correlation correctness as the hole, not
phrasing order.

    python3 gen.py
    agent_eval bench --dir bench/analysis-depth \
      --models "ollama-cloud/gemma4:31b" --iters 3
"""
import csv, json, shutil, statistics
from collections import defaultdict

R = round
SRC = "../folder-qa-hard"

for fname in ["spend.csv", "sleep.jsonl", "screen_time.tsv", "books.csv"]:
    shutil.copy(f"{SRC}/{fname}", fname)

# --- load --------------------------------------------------------------

spend = list(csv.DictReader(open("spend.csv")))
sleep = [json.loads(l) for l in open("sleep.jsonl")]
screen = list(csv.DictReader(open("screen_time.tsv"), delimiter="\t"))
books = list(csv.DictReader(open("books.csv")))


def pearsonr(x, y):
    n = len(x)
    mx, my = sum(x) / n, sum(y) / n
    cov = sum((a - mx) * (b - my) for a, b in zip(x, y))
    vx = sum((a - mx) ** 2 for a in x)
    vy = sum((b - my) ** 2 for b in y)
    denom = (vx * vy) ** 0.5
    return cov / denom if denom else 0.0


def strength(r):
    a = abs(r)
    word = "strong" if a > 0.7 else "moderate" if a >= 0.3 else "weak"
    sign = "positive" if r > 0 else "negative" if r < 0 else "no"
    return word, sign


# --- 1: decomposition, H1 vs H2 spend, broad-based or concentrated -----
# `spend.csv` spans 2023-2024 (like ../folder-qa-hard/); the question asks
# about 2024 specifically, so scope to it the same way that dir's gen.py does.

cat_h1, cat_h2 = defaultdict(float), defaultdict(float)
for r in spend:
    if not r["month"].startswith("2024"):
        continue
    m = int(r["month"][5:7])
    (cat_h1 if m <= 6 else cat_h2)[r["category"]] += float(r["amount"])
cats = sorted(cat_h1)
deltas = {c: R(cat_h2[c] - cat_h1[c], 2) for c in cats}
total_delta = R(sum(deltas.values()), 2)
abs_sum = sum(abs(v) for v in deltas.values())
top_cat, top_delta = max(deltas.items(), key=lambda kv: abs(kv[1]))
concentrated = abs(top_delta) / abs_sum > 0.6 if abs_sum else False

# --- 2: comparison, one category-month vs that category's own normal --

by_cat_month = defaultdict(dict)
for r in spend:
    by_cat_month[r["category"]][r["month"]] = float(r["amount"])
# pick the category/month with the largest z-score against its own other months
best = None
for c, months in by_cat_month.items():
    vals = list(months.items())
    for m, v in vals:
        others = [ov for om, ov in vals if om != m]
        if len(others) < 2:
            continue
        mean = statistics.mean(others)
        sd = statistics.stdev(others)
        z = (v - mean) / sd if sd else 0.0
        if best is None or abs(z) > abs(best[3]):
            best = (c, m, v, z, mean)
compare_cat, compare_month, compare_val, compare_z, compare_mean = best
compare_meaningful = abs(compare_z) > 1.0
compare_direction = "higher" if compare_z > 0 else "lower"

MONTH_NAME = {
    1: "january", 2: "february", 3: "march", 4: "april", 5: "may", 6: "june",
    7: "july", 8: "august", 9: "september", 10: "october", 11: "november", 12: "december",
}
compare_month_label = f"{MONTH_NAME[int(compare_month[5:7])].title()} {compare_month[:4]}"

# --- 3: correlation, adequate n -- daily screen time vs same-night sleep

day_total = defaultdict(float)
for r in screen:
    day_total[r["date"]] += float(r["minutes"])
paired = [(day_total[s["date"]], s["quality"]) for s in sleep if s["date"] in day_total]
n_adequate = len(paired)
r_adequate = R(pearsonr([p[0] for p in paired], [p[1] for p in paired]), 3)
word_adequate, sign_adequate = strength(r_adequate)

# --- 4: correlation, too few points -- finished books, pages vs rating -

finished = [b for b in books if b["finished"] == "yes"]
n_few = len(finished)
pages = [float(b["pages"]) for b in finished]
ratings = [float(b["rating"]) for b in finished]
r_few = R(pearsonr(pages, ratings), 3)

# --- 5/6: baselines, no decomposition needed ----------------------------

rent_total_2024 = R(
    sum(float(r["amount"]) for r in spend if r["category"] == "rent" and r["month"].startswith("2024")),
    2,
)
avg_book_rating = R(statistics.mean(float(b["rating"]) for b in books), 3)

# --- cases ---------------------------------------------------------------

cases = [
    (
        "decomp-h1h2",
        "My spending in the second half of 2024 was higher than the first half "
        "-- was that increase broad-based across categories, or mostly one "
        "category?",
        ["spend.csv"],
        {
            "contains": (
                [top_cat, "concentrated|mostly|driven by|primarily|largely"]
                if concentrated
                else ["broad|spread|across|every category|most categories"]
            )
            + [str(abs(top_delta))],
        },
        "decomp-broad-or-concentrated",
    ),
    (
        "comparison-normal-range",
        f"Was my {compare_cat} spending in {compare_month_label} meaningfully "
        "different from usual (higher or lower), or within my normal range for "
        "that category?",
        ["spend.csv"],
        {
            "contains": [
                compare_cat,
                (
                    f"{compare_direction}|above|more than usual"
                    if compare_direction == "higher"
                    else f"{compare_direction}|below|less than usual"
                )
                if compare_meaningful
                else "normal|typical|usual|within range|not unusual",
            ]
        },
        "comparison-meaningful-or-normal",
    ),
    (
        "corr-adequate-n",
        "Is there a relationship between how much screen time I have in a day "
        "and how well I sleep that night?",
        ["sleep.jsonl", "screen_time.tsv"],
        {"contains": [str(n_adequate), f"{word_adequate}|{sign_adequate}"]},
        "correlation-adequate-n",
    ),
    (
        "corr-too-few",
        "Among the books I've actually finished, is there a relationship "
        "between page count and how highly I rated it?",
        ["books.csv"],
        {
            "contains": [
                str(n_few),
                "too few|not enough|small sample|can't be confident|hard to "
                "trust|limited data|not much data|too small",
            ]
        },
        "correlation-too-few-points",
    ),
    (
        "baseline-rent",
        "What was my total spending on rent in 2024?",
        ["spend.csv"],
        {"figures": [rent_total_2024]},
        "baseline-plain-lookup",
    ),
    (
        "baseline-books-avg",
        "What's the average rating across all the books in my reading list?",
        ["books.csv"],
        {"approx": [avg_book_rating, 0.05]},
        "baseline-plain-lookup",
    ),
]

DOMAIN_BY_ID = {
    "decomp-h1h2": "spending",
    "comparison-normal-range": "spending",
    "corr-adequate-n": "sleep",
    "corr-too-few": "reading",
    "baseline-rent": "spending",
    "baseline-books-avg": "reading",
}


def _ext(name):
    return name.rsplit(".", 1)[-1].lower() if "." in name else ""


with open("cases.jsonl", "w") as f:
    f.write(
        "# analysis-depth battery: tests depth_rule's decomposition/comparison/"
        "correlation behavior directly, not folder-QA retrieval. Gold computed "
        "by bench/analysis-depth/gen.py from the copied files, not hand-typed.\n"
    )
    for cid, q, files, gold, tier in cases:
        fid = f"da-{cid}"
        rec = {
            "id": fid,
            "question": q,
            "files": files,
            "gold": gold,
            "tier": tier,
            "domain": DOMAIN_BY_ID[cid],
            "multifile": len(files) > 1,
            "n_files": len(files),
            "formats": sorted({_ext(x) for x in files if _ext(x)}),
        }
        f.write(json.dumps(rec) + "\n")

print(f"wrote {len(cases)} analysis-depth cases")
print(f"  deltas={deltas} top_cat={top_cat} top_delta={top_delta} concentrated={concentrated}")
print(f"  compare={compare_cat}/{compare_month} val={compare_val:.2f} mean={compare_mean:.2f} "
      f"z={compare_z:.2f} meaningful={compare_meaningful}")
print(f"  n_adequate={n_adequate} r_adequate={r_adequate} ({word_adequate} {sign_adequate})")
print(f"  n_few={n_few} r_few={r_few}")
print(f"  rent_total_2024={rent_total_2024}  avg_book_rating={avg_book_rating}")

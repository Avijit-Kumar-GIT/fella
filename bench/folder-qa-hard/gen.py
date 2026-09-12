#!/usr/bin/env python3
"""Regenerate the *hard* folder-QA battery: same life-domain data as
../folder-qa/ (the files here are byte-identical copies -- deterministic,
seed=7 in the original gen.py, so copying is safe and avoids re-deriving the
xlsx/pdf writers for files this battery doesn't need) but harder questions:
rank-2/3 with an exact gap, weighted composites, a genuine 3-condition join,
unit conversion the DB can't do for you (km -> miles), a non-standard-date
half-year split, two "does it correctly say no / zero" traps, and two
`make_chart`-graded cases. Deliberately scoped so accuracy has real headroom
-- see docs/HARNESS-COMPARISON.md's "Scoped to the everyday-question tier"
note; this is the next tier up, not a general data-science benchmark.

Every gold value below is *computed from the copied files*, not hand-typed,
so a data edit here can't silently drift from the actual grader.

    python3 gen.py
    agent_eval bench --dir bench/folder-qa-hard \
      --models "ollama-cloud/gemma4:31b,openrouter/openai/gpt-5.6-luna" --iters 3
"""
import csv, json, datetime
from collections import defaultdict

R = round

# --- load ------------------------------------------------------------------

spend = list(csv.DictReader(open("spend.csv")))
budget = {r["category"]: float(r["monthly_budget"]) for r in csv.DictReader(open("budget.csv"))}
workouts = list(csv.DictReader(open("workouts.csv")))
trips = list(csv.DictReader(open("trips.csv")))
contacts = json.load(open("contacts.json"))
rent_ledger = list(csv.DictReader(open("rent_ledger.csv")))
books = list(csv.DictReader(open("books.csv")))
screen = list(csv.DictReader(open("screen_time.tsv"), delimiter="\t"))
sleep = [json.loads(l) for l in open("sleep.jsonl")]

cat_year, cat_month, month_total = defaultdict(float), defaultdict(float), defaultdict(float)
for r in spend:
    y, m = r["month"][:4], r["month"][:7]
    cat_year[(r["category"], y)] += float(r["amount"])
    cat_month[(r["category"], m)] += float(r["amount"])
    month_total[m] += float(r["amount"])
cats = sorted(budget)
by_cat_2024 = {c: R(cat_year[(c, "2024")], 2) for c in cats}
total_2024 = R(sum(by_cat_2024.values()), 2)
over_by = {c: R(by_cat_2024[c] - 12 * budget[c], 2) for c in cats}
over_ranked = sorted(over_by.items(), key=lambda kv: -kv[1])  # biggest over first

months_2024 = sorted(m for m in month_total if m.startswith("2024"))
month_ranked = sorted(((m, R(month_total[m], 2)) for m in months_2024), key=lambda kv: -kv[1])

run_km = R(sum(float(r["distance_km"]) for r in workouts if r["activity"] == "run"), 2)
run_miles = R(run_km * 0.621371, 1)

def parse_ledger_date(s):
    return datetime.datetime.strptime(s, "%b %d, %Y")

h1_rent = R(sum(float(r["rent"]) for r in rent_ledger if parse_ledger_date(r["date"]).month <= 6), 2)
h2_rent = R(sum(float(r["rent"]) for r in rent_ledger if parse_ledger_date(r["date"]).month >= 7), 2)
rent_half_diff = R(h2_rent - h1_rent, 2)

n_leisure_trips = sum(1 for t in trips if t["purpose"] == "leisure")
leisure_countries = {t["country"] for t in trips if t["purpose"] == "leisure"}
work_only_countries = {t["country"] for t in trips if t["purpose"] == "work"} - leisure_countries
leisure_nights_by_country = {t["country"]: int(t["nights"]) for t in trips if t["purpose"] == "leisure"}
contacts_in_leisure = [c["name"] for c in contacts if c["country"] in leisure_countries]
leisure_contact_nights = sum(
    leisure_nights_by_country[c["country"]] for c in contacts if c["country"] in leisure_countries
)
contacts_work_only = sorted(c["name"] for c in contacts if c["country"] in work_only_countries)

genre_ratings = defaultdict(list)
for b in books:
    genre_ratings[b["genre"]].append(float(b["rating"]))
qualified_genres = {g: sum(rs) / len(rs) for g, rs in genre_ratings.items() if len(rs) >= 2}
best_genre = max(qualified_genres.items(), key=lambda kv: kv[1])[0]

day_total, day_app = defaultdict(float), defaultdict(float)
apps = sorted({r["app"] for r in screen})
for r in screen:
    day_total[r["date"]] += float(r["minutes"])
    day_app[(r["date"], r["app"])] += float(r["minutes"])
heavy_days = [d for d, t in day_total.items() if t > 150]
app_avg_on_heavy = {
    a: sum(day_app[(d, a)] for d in heavy_days) / len(heavy_days) for a in apps
}
top_app_heavy = max(app_avg_on_heavy.items(), key=lambda kv: kv[1])[0]

q1_dates = {s["date"] for s in sleep if s["quality"] == 1}
q3_dates = {s["date"] for s in sleep if s["quality"] == 3}
avg_screen_q1 = sum(day_total[d] for d in q1_dates if d in day_total) / len(
    [d for d in q1_dates if d in day_total]
)
avg_screen_q3 = sum(day_total[d] for d in q3_dates if d in day_total) / len(
    [d for d in q3_dates if d in day_total]
)
sleep_screen_gap = R(avg_screen_q1 - avg_screen_q3, 1)

rent_pct_of_total = R(100 * by_cat_2024["rent"] / total_2024, 1)

MONTH_NAME = {
    1: "january", 2: "february", 3: "march", 4: "april", 5: "may", 6: "june",
    7: "july", 8: "august", 9: "september", 10: "october", 11: "november", 12: "december",
}
h2_months = [f"2024-{m:02d}" for m in range(7, 13)]
# "name|iso": a model may label a chart's x-axis with the month name or the
# raw YYYY-MM it read from the column -- both are a correct chart.
h2_labels = [f"{MONTH_NAME[m][:3]}|{h2_months[m - 7]}" for m in range(7, 13)]
h2_values = [R(month_total[m], 2) for m in h2_months]

# --- cases -------------------------------------------------------------
# (id, question, files, gold, tier)
cases = [
    (
        "budget-second-over",
        "Comparing my actual spending against my budget (actual minus 12x the "
        "monthly budget), which category was the SECOND-most over its annual "
        "budget in 2024?",
        ["spend.csv", "budget.csv"],
        {"contains": [over_ranked[1][0]]},
        "mf-join-rank2",
    ),
    (
        "budget-smallest-over",
        "Of the categories that went over their annual budget in 2024, which one "
        "went over by the SMALLEST dollar amount?",
        ["spend.csv", "budget.csv"],
        {"contains": [min(over_by.items(), key=lambda kv: kv[1])[0]]},
        "mf-join-rank-min",
    ),
    (
        "third-month",
        "Which month of 2024 was my THIRD-highest in total spending (all "
        "categories combined), and by how much did it trail the highest month?",
        ["spend.csv"],
        {
            "contains": [
                f"{MONTH_NAME[int(month_ranked[2][0][5:])][:3]}|{month_ranked[2][0]}",
                str(R(month_ranked[0][1] - month_ranked[2][1], 2)),
            ]
        },
        "num-rank3-gap",
    ),
    (
        "run-miles",
        "How many MILES (not km) did I run in total?",
        ["workouts.csv"],
        {"approx": [run_miles, 1.0]},
        "num-unit-convert",
    ),
    (
        "rent-half-diff",
        "Did I pay more rent in the second half of 2024 (Jul-Dec) or the first "
        "half (Jan-Jun), and by how much?",
        ["rent_ledger.csv"],
        {"approx": [abs(rent_half_diff), 3.0]},
        "num-date-boundary-nonstd",
    ),
    (
        "contacts-leisure-nights",
        "For the contacts who live in a country I've visited for LEISURE (not "
        "work), how many total trip-nights did I spend in those countries?",
        ["contacts.json", "trips.csv"],
        {"figures": [leisure_contact_nights]},
        "mf-join-compound-filter",
    ),
    (
        "contacts-work-only",
        "Which of my contacts live in a country I've only ever visited for work, "
        "never for leisure?",
        ["contacts.json", "trips.csv"],
        {"contains": contacts_work_only},
        "mf-join-set-diff",
    ),
    (
        "goal-ontrack",
        "I've set myself five goals for the year. Checking my actual data, which "
        "ONE goal am I currently on track to meet?",
        ["goals.md", "books.csv", "workouts.csv", "trips.csv", "sleep.jsonl", "spend.csv"],
        # "travel" alone is too weak here -- a wrong answer that enumerates
        # all five goals before picking a different one still mentions the
        # word "travel" in passing. Require the actual trip count too, since
        # only a genuinely correct conclusion states it.
        {"contains": ["travel", str(n_leisure_trips)]},
        "mf-5way-synthesis",
    ),
    (
        "genre-best-rated",
        "Among genres with at least 2 books in my reading list, which genre has "
        "the highest average rating?",
        ["books.csv"],
        {"contains": [best_genre]},
        "cat-groupby-filtered",
    ),
    (
        "screen-top-app-heavy-days",
        "On the days my total screen time exceeded 150 minutes, which single app "
        "did I use the most, on average?",
        ["screen_time.tsv"],
        {"contains": [top_app_heavy]},
        "cat-filter-groupby",
    ),
    (
        "sleep-screen-gap",
        "On nights I rated my sleep quality 1 versus nights I rated it 3, what's "
        "the difference in my average daily screen time, in minutes?",
        ["sleep.jsonl", "screen_time.tsv"],
        {"approx": [sleep_screen_gap, 3.0]},
        "mf-join-two-group-compare",
    ),
    (
        "rent-pct-of-total",
        "What percentage of my total 2024 spending went to rent?",
        ["spend.csv"],
        {"approx": [rent_pct_of_total, 1.5]},
        "num-pct-composite",
    ),
    (
        "trap-nonexistent-category",
        "How much did I spend on 'restaurants' in 2024?",
        ["spend.csv"],
        {"approx": [0.0, 0.5]},
        "trap-near-miss-label",
    ),
    (
        "trap-2025",
        "What was my rent in January 2025?",
        ["spend.csv"],
        {"approx": [0.0, 0.5]},
        "trap-out-of-range-date",
    ),
    (
        "chart-cat-bar",
        "Make a bar chart of my 2024 spending by category.",
        ["spend.csv"],
        {
            "chart": {
                "labels": cats,
                "series": [{"name": "spending", "values": [by_cat_2024[c] for c in cats]}],
            }
        },
        "chart-bar-graded",
    ),
    (
        "chart-h2-line",
        "Make a line chart of my total monthly spending for the second half of "
        "2024 (Jul through Dec).",
        ["spend.csv"],
        {"chart": {"labels": h2_labels, "series": [{"name": "spending", "values": h2_values}]}},
        "chart-line-graded",
    ),
]

DOMAIN_BY_PREFIX = {
    "budget": "spending", "third": "spending", "run": "fitness", "rent": "housing",
    "contacts": "contacts", "goal": "general", "genre": "reading", "screen": "screen-time",
    "sleep": "sleep", "trap": "spending", "chart": "spending",
}


def _ext(name):
    return name.rsplit(".", 1)[-1].lower() if "." in name else ""


with open("cases.jsonl", "w") as f:
    f.write(
        "# harder folder-QA battery: rank/composite/unit-convert/trap/chart "
        "questions against the same data as ../folder-qa/. Gold computed by "
        "bench/folder-qa-hard/gen.py from the copied files, not hand-typed.\n"
    )
    for cid, q, files, gold, tier in cases:
        fid = f"fqah-{cid}"
        rec = {
            "id": fid,
            "question": q,
            "files": files,
            "gold": gold,
            "tier": tier,
            "domain": DOMAIN_BY_PREFIX[cid.split("-")[0]],
            "multifile": len(files) > 1,
            "n_files": len(files),
            "cluttered": False,
            "formats": sorted({_ext(x) for x in files if _ext(x)}),
        }
        f.write(json.dumps(rec) + "\n")

print(f"wrote {len(cases)} hard cases")
print(f"  by_cat_2024={by_cat_2024}")
print(f"  over_by={over_by}  ranked={over_ranked}")
print(f"  month_ranked(top3)={month_ranked[:3]}")
print(f"  run_km={run_km} run_miles={run_miles}")
print(f"  h1_rent={h1_rent} h2_rent={h2_rent} diff={rent_half_diff}")
print(f"  leisure_countries={leisure_countries} contacts_in_leisure={contacts_in_leisure} "
      f"leisure_contact_nights={leisure_contact_nights}")
print(f"  work_only_countries={work_only_countries} contacts_work_only={contacts_work_only}")
print(f"  qualified_genres={qualified_genres} best_genre={best_genre}")
print(f"  heavy_days={len(heavy_days)} app_avg_on_heavy={app_avg_on_heavy} top={top_app_heavy}")
print(f"  avg_screen_q1={avg_screen_q1:.1f}({len(q1_dates)}) avg_screen_q3={avg_screen_q3:.1f}"
      f"({len(q3_dates)}) gap={sleep_screen_gap}")
print(f"  rent_pct_of_total={rent_pct_of_total}")
print(f"  h2_labels={h2_labels} h2_values={h2_values}")

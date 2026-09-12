#!/usr/bin/env python3
"""Axis: step judgment. Six single-step/multi-step TWIN pairs on the same
underlying data. The single-step twin is answerable directly; its twin has a
genuine dependency -- the second lookup's specifics (which category, which
book, which week, ...) are only known after the first lookup's result comes
back, so it cannot be pre-written as one static query.

Grading is plain answer correctness (the existing Gold shapes) -- a model
cannot get the dependent twin right without actually taking the second,
dependent step, so correctness itself is the step-understanding signal. No
"fewer steps is better" scoring: extra genuinely useful steps are never
penalized here. stdlib only.

    python3 gen.py
    agent_eval bench --dir bench/step-judgment --models "ollama-cloud/gemma4:31b" --iters 3
"""
import csv
import json
from collections import defaultdict

R = round

# --- pair 1: spend.csv -----------------------------------------------------
SPEND = [
    ("2024-08-03", "dining", 120.0), ("2024-08-11", "travel", 800.0),
    ("2024-08-19", "utilities", 150.0), ("2024-08-22", "dining", 40.0),
    ("2024-03-14", "travel", 1450.0),  # the true largest travel purchase (outside August)
    ("2024-05-02", "travel", 300.0), ("2024-08-27", "travel", 200.0),
]
with open("spend.csv", "w", newline="") as f:
    w = csv.writer(f)
    w.writerow(["date", "category", "amount"])
    w.writerows(SPEND)

aug_by_cat = defaultdict(float)
for d, c, a in SPEND:
    if d.startswith("2024-08"):
        aug_by_cat[c] += a
top_aug_category = max(aug_by_cat.items(), key=lambda kv: kv[1])[0]
largest_in_top_category = max(a for d, c, a in SPEND if c == top_aug_category)

# --- pair 2: books.csv -----------------------------------------------------
BOOKS = [
    ("The Dispossessed", "read", 341, "2024-02-11"),
    ("Klara and the Sun", "read", 303, "2024-03-20"),
    ("Half of a Yellow Sun", "read", 433, "2024-05-02"),
    ("Invisible Cities", "read", 165, "2024-06-30"),  # the most recently finished
    ("Piranesi", "reading", 245, ""),                  # currently reading
    ("Braiding Sweetgrass", "unread", 391, ""),
]
with open("books.csv", "w", newline="") as f:
    w = csv.writer(f)
    w.writerow(["title", "status", "pages", "finish_date"])
    w.writerows(BOOKS)

last_finished = max((b for b in BOOKS if b[1] == "read"), key=lambda b: b[3])
currently_reading = next(b for b in BOOKS if b[1] == "reading")
# magnitude only -- a correct answer says "80 pages shorter", stating the
# plain number 80, never a literal negative sign
page_diff = abs(last_finished[2] - currently_reading[2])

# --- pair 3: workouts.csv --------------------------------------------------
WORKOUTS = [
    ("2024-05-01", "bench", 170), ("2024-05-01", "squat", 255), ("2024-05-01", "deadlift", 320),
    ("2024-05-22", "bench", 175), ("2024-05-22", "squat", 260),
    ("2024-06-10", "bench", 185),   # the all-time bench PR, week of Jun 10-16
    ("2024-06-12", "squat", 275),   # same week, ALSO a new all-time squat PR
    ("2024-06-13", "deadlift", 315),  # same week, NOT a PR (below the 320 set May 1)
    ("2024-07-01", "bench", 180), ("2024-07-01", "squat", 270),
]
with open("workouts.csv", "w", newline="") as f:
    w = csv.writer(f)
    w.writerow(["date", "exercise", "weight_lbs"])
    w.writerows(WORKOUTS)

bench_rows = [(d, w_) for d, e, w_ in WORKOUTS if e == "bench"]
bench_pr_date, bench_pr = max(bench_rows, key=lambda x: x[1])


def iso_week(date_str):
    import datetime
    return datetime.date.fromisoformat(date_str).isocalendar()[:2]


pr_week = iso_week(bench_pr_date)
exercises = sorted({e for _, e, _ in WORKOUTS} - {"bench"})
same_week_prs = []
for ex in exercises:
    rows = [(d, w_) for d, e, w_ in WORKOUTS if e == ex]
    all_time_max = max(w_ for _, w_ in rows)
    for d, w_ in rows:
        if iso_week(d) == pr_week and w_ == all_time_max:
            same_week_prs.append(ex)

# --- pair 4: subscriptions.csv + lease.md ----------------------------------
SUBS = [("cloud storage", 2.99), ("music streaming", 10.99), ("video streaming", 15.49), ("vpn", 3.33)]
with open("subscriptions.csv", "w", newline="") as f:
    w = csv.writer(f)
    w.writerow(["service", "monthly_cost"])
    w.writerows(SUBS)
with open("lease.md", "w") as f:
    f.write("# Lease\n\nTerm: 2024-01-01 to 2025-12-31 (24 months).\nMonthly rent: $1,450.\n")

priciest_sub, priciest_cost = max(SUBS, key=lambda x: x[1])
lease_months = 24
savings_over_lease = R(priciest_cost * lease_months, 2)

# --- pair 5: trips.csv ------------------------------------------------------
TRIPS = [
    ("Lisbon", "2024-02-10", 950.0), ("Kyoto", "2024-04-05", 2400.0),
    ("Berlin", "2024-06-20", 1100.0), ("Oaxaca", "2024-09-14", 1300.0),  # most recent
]
with open("trips.csv", "w", newline="") as f:
    w = csv.writer(f)
    w.writerow(["place", "date", "cost_usd"])
    w.writerows(TRIPS)

most_recent_trip = max(TRIPS, key=lambda t: t[1])
avg_trip_cost = R(sum(c for _, _, c in TRIPS) / len(TRIPS), 2)
recent_vs_avg = R(most_recent_trip[2] - avg_trip_cost, 2)

# --- pair 6: sleep.csv + journal.md -----------------------------------------
SLEEP = [
    ("2024-09-01", 7.5), ("2024-09-08", 6.2), ("2024-09-15", 5.1),  # worst night
    ("2024-09-22", 7.8), ("2024-09-29", 7.1),
]
with open("sleep.csv", "w", newline="") as f:
    w = csv.writer(f)
    w.writerow(["date", "hours"])
    w.writerows(SLEEP)

sept_avg_sleep = R(sum(h for _, h in SLEEP) / len(SLEEP), 2)
worst_night = min(SLEEP, key=lambda s: s[1])

with open("journal.md", "w") as f:
    f.write("# Journal\n\n")
    f.write("2024-09-01  Good day, felt rested.\n")
    f.write("2024-09-08  Busy week, a bit tired.\n")
    f.write("2024-09-15  Rough night before, huge deadline stress at work.\n")
    f.write("2024-09-22  Weekend hike, felt great.\n")

# --- cases -------------------------------------------------------------
# each "-b" (dependent step) carries setup_turns=[the matching "-a" question]
# so it runs in the SAME conversation, in order -- "that category" / "it" /
# "that same week" need the prior turn's answer to actually be in context.
# Without this they're unanswerable by construction, not a capability gap.
pair1_a_q = "What category did I spend the most on in August 2024?"
pair2_a_q = "What was the last book I finished?"
pair3_a_q = "What was my max bench press weight this year?"
pair4_a_q = "Which subscription costs the most per month?"
pair5_a_q = "Which place did I visit most recently?"

cases = [
    ("pair1-a", pair1_a_q, ["spend.csv"],
     {"contains": [top_aug_category]}, "single-step", []),
    ("pair1-b", "What was the single largest purchase in that category?", ["spend.csv"],
     {"figures": [largest_in_top_category]}, "dependent-step", [pair1_a_q]),

    ("pair2-a", pair2_a_q, ["books.csv"],
     {"contains": [last_finished[0].lower()]}, "single-step", []),
    ("pair2-b", "How does its page count compare to the book I'm currently reading -- "
     "by how many pages?", ["books.csv"],
     {"figures": [float(page_diff)]}, "dependent-step", [pair2_a_q]),

    ("pair3-a", pair3_a_q, ["workouts.csv"],
     {"figures": [float(bench_pr)]}, "single-step", []),
    ("pair3-b", "What other exercise did I hit a personal record on that same week?", ["workouts.csv"],
     {"contains": same_week_prs}, "dependent-step", [pair3_a_q]),

    ("pair4-a", pair4_a_q, ["subscriptions.csv"],
     {"contains": [priciest_sub]}, "single-step", []),
    ("pair4-b", "If I cancelled it, how much would I save over the full length of my lease?",
     ["subscriptions.csv", "lease.md"],
     {"approx": [savings_over_lease, 1.0]}, "dependent-step", [pair4_a_q]),

    ("pair5-a", pair5_a_q, ["trips.csv"],
     {"contains": [most_recent_trip[0]]}, "single-step", []),
    ("pair5-b", "How did that trip's cost compare to my average trip cost?", ["trips.csv"],
     {"approx": [abs(recent_vs_avg), 5.0]}, "dependent-step", [pair5_a_q]),

    ("pair6-a", "What was my average sleep duration in September 2024?", ["sleep.csv"],
     {"approx": [sept_avg_sleep, 0.05]}, "single-step", []),
    ("pair6-b", "Which night in September 2024 did I sleep the least, and what does my "
     "journal say about that day?", ["sleep.csv", "journal.md"],
     {"contains": ["deadline|stress|rough"]}, "dependent-step", []),
]

DOMAIN = "general"

with open("cases.jsonl", "w") as f:
    f.write(
        "# Axis: step judgment. Twin pairs (single-step vs genuinely dependent "
        "multi-step) on the same data. Gold computed by bench/step-judgment/gen.py.\n"
    )
    for cid, q, files, gold, tier, turns in cases:
        rec = {
            "id": f"stp-{cid}",
            "question": q,
            "files": files,
            "gold": gold,
            "tier": tier,
            "domain": DOMAIN,
            "multifile": len(files) > 1,
            "n_files": len(files),
            "cluttered": False,
            "formats": sorted({f.rsplit(".", 1)[-1] for f in files}),
        }
        if turns:
            rec["setup_turns"] = turns
        f.write(json.dumps(rec) + "\n")

print(f"wrote {len(cases)} step-judgment cases")
print(f"  top_aug_category={top_aug_category} largest_in_cat={largest_in_top_category}")
print(f"  last_finished={last_finished} currently_reading={currently_reading} page_diff={page_diff}")
print(f"  bench_pr={bench_pr}@{bench_pr_date} week={pr_week} same_week_prs={same_week_prs}")
print(f"  priciest_sub={priciest_sub}@{priciest_cost} savings_over_lease={savings_over_lease}")
print(f"  most_recent_trip={most_recent_trip} avg_trip_cost={avg_trip_cost} recent_vs_avg={recent_vs_avg}")
print(f"  sept_avg_sleep={sept_avg_sleep} worst_night={worst_night}")

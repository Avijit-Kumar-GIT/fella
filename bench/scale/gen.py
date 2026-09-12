#!/usr/bin/env python3
"""Axis: folder scale/diversity + document size. Does the right file (or the
right part of one big file) still get found as clutter, folder size, and
single-document size grow -- distinct from the messiness axis (bad data) and
the step-judgment axis (bad reasoning). Every distractor file is EXPLICITLY
listed in the case's `files` (that's what actually lands in the staged
workspace the model sees; a scale test that forgets this tests nothing).
stdlib only.

    python3 gen.py
    agent_eval bench --dir bench/scale --models "ollama-cloud/gemma4:31b" --iters 3
"""
import csv
import json
import random

random.seed(11)
R = round

# --- case 1: tiny folder, sanity floor -------------------------------------
with open("tiny_workout.csv", "w", newline="") as f:
    w = csv.writer(f)
    w.writerow(["date", "activity"])
    w.writerows([("2024-08-01", "gym"), ("2024-08-05", "gym"), ("2024-08-14", "run")])
tiny_gym_count = 2

# --- case 2: budget.csv + 15 irrelevant client-invoice distractors --------
with open("budget.csv", "w", newline="") as f:
    w = csv.writer(f)
    w.writerow(["category", "monthly_amount"])
    w.writerows([("rent", 1450), ("groceries", 400), ("transport", 90)])

CLUTTER_15 = []
for i in range(1, 16):
    name = f"client_invoice_{i:02d}.md"
    with open(name, "w") as f:
        f.write(f"# Invoice #{1000+i}\n\nClient: Acme Corp\nAmount due: ${200 + i*17}.00\n")
    CLUTTER_15.append(name)

# --- case 3: 30-file diverse folder, only subscriptions.csv relevant ------
with open("subscriptions.csv", "w", newline="") as f:
    w = csv.writer(f)
    w.writerow(["service", "monthly_cost", "billing_cycle"])
    w.writerows([
        ("cloud storage", 2.99, "monthly"), ("music streaming", 10.99, "monthly"),
        ("video streaming", 15.49, "monthly"), ("annual news", 60.00, "annual"),
    ])
sub_annualized = R(2.99 * 12 + 10.99 * 12 + 15.49 * 12 + 60.00, 2)

DIVERSE_TOPICS = [
    ("hiking_notes", "md", "Trail notes: {} miles, great views at the summit."),
    ("recipe", "md", "Recipe idea: {} minute prep, serves 4."),
    ("article_clip", "txt", "Saved article excerpt, paragraph {} of the piece."),
    ("meeting_notes", "md", "Notes from meeting #{}: discussed roadmap."),
    ("movie_list", "txt", "Watchlist entry {}: a film to watch this month."),
]
DIVERSE_30 = []
for i in range(26):
    stem, ext, tmpl = DIVERSE_TOPICS[i % len(DIVERSE_TOPICS)]
    name = f"{stem}_{i:02d}.{ext}"
    with open(name, "w") as f:
        f.write(tmpl.format(i + 1) + "\n")
    DIVERSE_30.append(name)
DIVERSE_30.append("subscriptions.csv")  # the one relevant file, 27th of 27

# --- case 4: stale vs current expense export, distinguishable by content --
with open("expenses_2025.csv", "w", newline="") as f:
    w = csv.writer(f)
    w.writerow(["date", "category", "amount"])
    w.writerows([("2025-01-05", "rent", 1400.0), ("2025-02-05", "rent", 1400.0)])
with open("expenses_2026.csv", "w", newline="") as f:
    w = csv.writer(f)
    w.writerow(["date", "category", "amount"])
    rows_2026 = [(f"2026-{m:02d}-05", "rent", 1450.0) for m in range(1, 9)]
    w.writerows(rows_2026)
rent_2026 = R(sum(a for _, _, a in rows_2026), 2)

# --- case 5: 40 small daily notes, 6 mention "anxious" ---------------------
random.seed(11)
DAILY_MOODS = ["content", "tired", "focused", "anxious", "restless", "good", "calm"]
anxious_days = set(random.sample(range(1, 41), 6))
DAILY_40 = []
for day in range(1, 41):
    name = f"note_day{day:02d}.md"
    mood = "anxious" if day in anxious_days else random.choice(
        [m for m in DAILY_MOODS if m != "anxious"]
    )
    with open(name, "w") as f:
        f.write(f"Day {day} note. Feeling {mood} today.\n")
    DAILY_40.append(name)
n_anxious = len(anxious_days)

# --- case 6: wide table, near-identical column names -----------------------
WIDE_COLS = [
    "date", "steps", "resting_hr", "active_hr", "max_hr", "sleep_hours",
    "sleep_deep_min", "sleep_light_min", "sleep_rem_min", "calories",
    "calories_active", "protein_g", "protein_target_g", "carbs_g",
    "carbs_target_g", "fat_g", "fat_target_g", "water_ml", "weight_kg",
    "vo2max", "hrv_ms", "stress_score", "floors_climbed", "distance_km",
    "elevation_gain_m", "screen_time_min", "mood_score", "readiness_score",
    "recovery_min", "workout_min",
]
WIDE_ROWS = []
for d in range(1, 8):
    row = {"date": f"2024-08-{d:02d}"}
    for c in WIDE_COLS[1:]:
        # protein_g stays below the forced day-4 peak so the max is
        # unambiguous and doesn't depend on the random draw
        row[c] = R(random.uniform(10, 180 if c == "protein_g" else 300), 1)
    WIDE_ROWS.append(row)
WIDE_ROWS[3]["protein_g"] = 210.0  # day 4: the clean, findable max
with open("health_wide.csv", "w", newline="") as f:
    w = csv.DictWriter(f, fieldnames=WIDE_COLS)
    w.writeheader()
    w.writerows(WIDE_ROWS)
# computed from the actual rows, not assumed -- can't silently drift
max_protein_day = max(WIDE_ROWS, key=lambda r: r["protein_g"])["date"]
assert max_protein_day == "2024-08-04", max_protein_day

# --- case 7: a genuinely large single CSV, exact aggregate required -------
BIG_ROWS = []
for i in range(1500):
    m = 1 + (i % 12)
    d = 1 + (i % 27)
    desc = "Amazon order" if i % 9 == 0 else random.choice(
        ["Coffee shop", "Grocery run", "Gas station", "Pharmacy", "Restaurant"]
    )
    amt = R(random.uniform(4, 180), 2)
    BIG_ROWS.append((f"2024-{m:02d}-{d:02d}", desc, amt))
with open("bigledger.csv", "w", newline="") as f:
    w = csv.writer(f)
    w.writerow(["date", "description", "amount"])
    w.writerows(BIG_ROWS)
amazon_rows = [a for d, desc, a in BIG_ROWS if desc == "Amazon order"]
amazon_count = len(amazon_rows)
amazon_total = R(sum(amazon_rows), 2)

cases = [
    ("tiny-sanity-floor", "How many times did I go to the gym in August 2024?",
     ["tiny_workout.csv"], {"figures": [float(tiny_gym_count)]}, "scale-tiny"),
    ("clutter-few-relevant", "What's my monthly rent budget?",
     ["budget.csv"] + CLUTTER_15, {"figures": [1450.0]}, "scale-clutter-15"),
    ("large-diverse-folder", "Annualized, what do all my active subscriptions cost me "
     "in total per year (including the annual one)?",
     DIVERSE_30, {"approx": [sub_annualized, 1.0]}, "scale-diverse-30"),
    ("stale-vs-current-file", "How much did I spend on rent in 2026, based on whichever "
     "expense file actually covers that year?",
     ["expenses_2025.csv", "expenses_2026.csv"], {"approx": [rent_2026, 1.0]},
     "scale-file-disambiguation"),
    ("exhaustive-small-files", "Across all my daily notes, how many days did I describe "
     "feeling anxious?",
     DAILY_40, {"figures": [float(n_anxious)]}, "scale-many-small-files"),
    ("wide-table-column-disambiguation", "On which date did I eat the most protein "
     "(actual intake, not my target)?",
     ["health_wide.csv"], {"contains": [max_protein_day]}, "docsize-wide-columns"),
    ("large-csv-exact-aggregate", "How many Amazon orders do I have on record, and what's "
     "the total amount across them?",
     ["bigledger.csv"], {"contains": [str(amazon_count), str(amazon_total)]},
     "docsize-large-row-count"),
]

DOMAIN = "general"

with open("cases.jsonl", "w") as f:
    f.write(
        "# Axis: folder scale/diversity + document size. Gold computed by "
        "bench/scale/gen.py from the exact fixtures written here.\n"
    )
    for cid, q, files, gold, tier in cases:
        rec = {
            "id": f"scl-{cid}",
            "question": q,
            "files": files,
            "gold": gold,
            "tier": tier,
            "domain": DOMAIN,
            "multifile": len(files) > 1,
            "n_files": len(files),
            "cluttered": len(files) > 3,
            "formats": sorted({f.rsplit(".", 1)[-1] for f in files}),
        }
        f.write(json.dumps(rec) + "\n")

print(f"wrote {len(cases)} scale cases")
print(f"  sub_annualized={sub_annualized} rent_2026={rent_2026} n_anxious={n_anxious} "
      f"max_protein_day={max_protein_day} amazon_count={amazon_count} amazon_total={amazon_total}")

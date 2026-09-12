#!/usr/bin/env python3
"""Axis: tool selection / irrelevance. Does the right capability get used for
the job -- including using NO tool when none is needed, reusing a fact
already stated in conversation instead of re-deriving it, routing text search
vs tabular query correctly, and declining honestly when the folder has no
relevant file at all. stdlib only.

    python3 gen.py
    agent_eval bench --dir bench/tool-selection --models "ollama-cloud/gemma4:31b" --iters 3
"""
import csv
import json

R = round

# --- books.csv: for the "reuse a stated fact" case ------------------------
BOOKS = [
    ("The Dispossessed", "read", 341), ("Klara and the Sun", "read", 303),
    ("Half of a Yellow Sun", "read", 433), ("Invisible Cities", "read", 165),
    ("How to Do Nothing", "read", 232), ("The Left Hand of Darkness", "read", 304),
    ("Intimations", "read", 97), ("Cosmicomics", "read", 153),
    ("Exhalation", "read", 336), ("The Overstory", "read", 502),
    ("Piranesi", "unread", 245), ("Braiding Sweetgrass", "unread", 391),
    ("The Employees", "unread", 136), ("Sea of Tranquility", "unread", 255),
    ("Annihilation", "unread", 195),
]
n_finished = sum(1 for _, s, _ in BOOKS if s == "read")

with open("books.csv", "w", newline="") as f:
    w = csv.writer(f)
    w.writerow(["title", "status", "pages"])
    w.writerows(BOOKS)

# --- journal.md: for the grep-vs-sql routing case --------------------------
JOURNAL_ENTRIES = [
    ("2024-01-08", "Finished a book on the train. mood: content."),
    ("2024-02-14", "Rough day, deploy slipped, total burnout by evening."),
    ("2024-03-02", "Morning run by the river. mood: good."),
    ("2024-04-19", "Dentist, then cooked dinner. mood: fine."),
    ("2024-05-02", "Long week, feeling burnout creeping back in."),
    ("2024-06-21", "Longest day of the year. mood: restless."),
    ("2024-07-30", "Vacation starts tomorrow, can't wait."),
    ("2024-08-11", "Back from vacation, straight into another burnout spiral."),
    ("2024-09-05", "Quiet weekend, read outside."),
    ("2024-10-17", "Good day at work for once."),
]
n_burnout = sum(1 for _, text in JOURNAL_ENTRIES if "burnout" in text.lower())

with open("journal.md", "w") as f:
    f.write("# Journal 2024\n\n")
    for d, t in JOURNAL_ENTRIES:
        f.write(f"{d}  {t}\n")

# --- subscriptions.csv: for the sql-vs-python-detour case -----------------
SUBS = [
    ("cloud storage", 2.99), ("music streaming", 10.99), ("news site", 5.00),
    ("vpn", 3.33), ("video streaming", 15.49),
]
avg_sub_cost = R(sum(c for _, c in SUBS) / len(SUBS), 2)

with open("subscriptions.csv", "w", newline="") as f:
    w = csv.writer(f)
    w.writerow(["service", "monthly_cost"])
    w.writerows(SUBS)

# --- workouts.csv + sleep.csv: folder with NO financial file at all -------
WORKOUTS = [("2024-01-01", "yoga", 45), ("2024-01-03", "run", 50), ("2024-01-05", "yoga", 40)]
with open("workouts.csv", "w", newline="") as f:
    w = csv.writer(f)
    w.writerow(["date", "activity", "minutes"])
    w.writerows(WORKOUTS)

SLEEP = [("2024-01-01", 7.2), ("2024-01-02", 6.8), ("2024-01-03", 7.9)]
with open("sleep.csv", "w", newline="") as f:
    w = csv.writer(f)
    w.writerow(["date", "hours"])
    w.writerows(SLEEP)

# --- app_usage.csv: for the chart-over-applied case ------------------------
APP_USAGE = [
    ("browser", 320), ("browser", 280), ("music", 190), ("music", 150),
    ("messages", 90), ("messages", 60), ("maps", 40),
]
usage_totals = {}
for app, mins in APP_USAGE:
    usage_totals[app] = usage_totals.get(app, 0) + mins
top_app = max(usage_totals.items(), key=lambda kv: kv[1])[0]

with open("app_usage.csv", "w", newline="") as f:
    w = csv.writer(f)
    w.writerow(["app", "minutes"])
    w.writerows(APP_USAGE)

# --- cases -------------------------------------------------------------
cases = [
    (
        "general-knowledge-no-tool",
        "Roughly how many kilometers are in a mile?",
        [],
        "notool",
        "irrelevance-general-knowledge",
    ),
    (
        "reuse-conversational-fact",
        "How many more books do I need to read to hit that goal?",
        ["books.csv"],
        {"figures": [float(24 - n_finished)]},
        "context-reuse",
    ),
    (
        "grep-vs-sql-routing",
        "How many times did I mention 'burnout' in my journal?",
        ["journal.md"],
        {"figures": [float(n_burnout)]},
        "tool-routing-text-search",
    ),
    (
        "sql-native-aggregate",
        "What's my average monthly subscription cost?",
        ["subscriptions.csv"],
        {"approx": [avg_sub_cost, 0.05]},
        "tool-routing-sql-native",
    ),
    (
        "no-relevant-file",
        "How much did I spend on my gym membership last month?",
        ["workouts.csv", "sleep.csv"],
        "refusal",
        "irrelevance-no-data",
    ),
    (
        "single-fact-not-a-chart",
        "Which app did I use the most, in total?",
        ["app_usage.csv"],
        {"contains": [top_app]},
        "tool-routing-proportionate",
    ),
]

DOMAIN = "general"

with open("cases.jsonl", "w") as f:
    f.write(
        "# Axis: tool selection / irrelevance. Gold computed by "
        "bench/tool-selection/gen.py from the exact fixtures written here.\n"
    )
    for cid, q, files, gold, tier in cases:
        rec = {
            "id": f"tls-{cid}",
            "question": q,
            "files": files,
            "gold": gold,
            "tier": tier,
            "domain": DOMAIN,
            "multifile": len(files) > 1,
            "n_files": len(files),
            "cluttered": False,
            "formats": sorted({f.rsplit(".", 1)[-1] for f in files}) if files else [],
        }
        if cid == "reuse-conversational-fact":
            rec["setup_turns"] = ["My goal this year is to read 24 books."]
        f.write(json.dumps(rec) + "\n")

print(f"wrote {len(cases)} tool-selection cases")
print(f"  n_finished={n_finished} n_burnout={n_burnout} avg_sub_cost={avg_sub_cost} top_app={top_app}")

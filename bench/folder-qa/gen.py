#!/usr/bin/env python3
"""Regenerate the folder-QA battery: deterministic CSVs + notes.txt + cases.jsonl
with gold answers computed here. stdlib only. Run from this directory:

    python3 gen.py

The battery is a stand-in for "a folder of a person's own files" — spending,
workouts, a lease note — and the questions are the everyday shape Fella targets
(totals, filters, group-by, top-N, a trend, doc lookups, one refusal, one
no-tool). Run it with:

    agent_eval bench --dir bench/folder-qa --models "ollama-cloud/gemma4:31b" --iters 3
"""
import csv, json, random

random.seed(7)

cats = ["rent", "groceries", "transport", "utilities", "dining"]
base = {"rent": 1200, "groceries": 420, "transport": 95, "utilities": 140, "dining": 180}
rows = []
for y in (2023, 2024):
    for mo in range(1, 13):
        for c in cats:
            amt = round(
                base[c] * (1 + random.uniform(-0.12, 0.18))
                + (60 if (c == "rent" and y == 2024) else 0),
                2,
            )
            rows.append({"month": f"{y}-{mo:02d}", "category": c, "amount": amt})
with open("spend.csv", "w", newline="") as f:
    w = csv.DictWriter(f, ["month", "category", "amount"])
    w.writeheader()
    w.writerows(rows)


def s(pred):
    return round(sum(r["amount"] for r in rows if pred(r)), 2)


total_all = s(lambda r: True)
total_2024 = s(lambda r: r["month"].startswith("2024"))
total_2023 = s(lambda r: r["month"].startswith("2023"))
rent_2024 = s(lambda r: r["category"] == "rent" and r["month"].startswith("2024"))
groceries_all = s(lambda r: r["category"] == "groceries")
by_cat = {c: s(lambda r, c=c: r["category"] == c) for c in cats}
top_cat = max(by_cat, key=by_cat.get)
mtot = {}
for r in rows:
    mtot[r["month"]] = round(mtot.get(r["month"], 0) + r["amount"], 2)
peak_month = max(mtot, key=mtot.get)

wk = []
for d in range(1, 61):
    wk.append(
        {
            "date": f"2024-{(d - 1) // 28 + 1:02d}-{(d - 1) % 28 + 1:02d}",
            "activity": random.choice(["run", "bike", "swim", "lift"]),
            "minutes": random.randint(20, 75),
        }
    )
with open("workouts.csv", "w", newline="") as f:
    w = csv.DictWriter(f, ["date", "activity", "minutes"])
    w.writeheader()
    w.writerows(wk)
wk_total_min = sum(r["minutes"] for r in wk)
run_min = sum(r["minutes"] for r in wk if r["activity"] == "run")
wk_by_act = {}
for r in wk:
    wk_by_act[r["activity"]] = wk_by_act.get(r["activity"], 0) + r["minutes"]
top_act = max(wk_by_act, key=wk_by_act.get)

with open("notes.txt", "w") as f:
    f.write(
        "Apartment lease\n\nTenant: A. Kumar\n"
        "Monthly rent: $1,260 starting Jan 2024 (was $1,200).\n"
        "Security deposit: $2,400, refundable.\n"
        "Lease term: 12 months, renews Jan 2025.\n"
        "Landlord contact: Rivera Property Mgmt, (555) 010-8842.\n"
    )

cases = [
    ("total-all", "What is the total amount across all of spend.csv?", ["spend.csv"], {"figures": [total_all]}, "aggregate"),
    ("total-2024", "How much did I spend in 2024?", ["spend.csv"], {"figures": [total_2024]}, "aggregate"),
    ("rent-2024", "What was my total rent in 2024?", ["spend.csv"], {"figures": [rent_2024]}, "filter"),
    ("groceries", "How much did I spend on groceries in total?", ["spend.csv"], {"figures": [groceries_all]}, "filter"),
    ("top-cat", "Which category did I spend the most on overall?", ["spend.csv"], {"contains": [top_cat]}, "groupby"),
    ("yoy", "Did my spending go up or down from 2023 to 2024, and by how much?", ["spend.csv"], {"figures": [round(total_2024 - total_2023, 2)]}, "multi-step"),
    ("peak-month", "Which month had the highest total spend?", ["spend.csv"], {"contains": [peak_month]}, "groupby"),
    ("wk-total", "How many minutes did I work out in total?", ["workouts.csv"], {"figures": [wk_total_min]}, "aggregate"),
    ("wk-run", "How many minutes of running did I do?", ["workouts.csv"], {"figures": [run_min]}, "filter"),
    ("wk-top", "What activity did I spend the most time on?", ["workouts.csv"], {"contains": [top_act]}, "groupby"),
    ("doc-rent", "According to notes.txt, what is the monthly rent starting Jan 2024?", ["notes.txt"], {"contains": ["1,260"]}, "doc-lookup"),
    ("doc-deposit", "What is the security deposit on the lease?", ["notes.txt"], {"contains": ["2,400"]}, "doc-lookup"),
    ("cross", "Combine spend.csv and the lease in notes.txt: does my recorded 2024 rent match 12 months at the lease rate?", ["spend.csv", "notes.txt"], {"contains": ["no"]}, "multi-step"),
    ("refusal", "Based on spend.csv, how much will I spend next year?", ["spend.csv"], "refusal", "refusal"),
    ("notool", "What does the word 'ledger' mean?", [], "notool", "no-tool"),
]
with open("cases.jsonl", "w") as f:
    f.write("# Hand-built folder-QA battery. Gold computed by bench/folder-qa/gen.py.\n")
    for cid, q, files, gold, tier in cases:
        f.write(json.dumps({"id": f"fqa-{cid}", "question": q, "files": files, "gold": gold, "tier": tier}) + "\n")

print(f"wrote {len(cases)} cases")
print(f"  total_all={total_all} total_2024={total_2024} rent_2024={rent_2024}")
print(f"  top_cat={top_cat} peak_month={peak_month} wk_total={wk_total_min} top_act={top_act}")

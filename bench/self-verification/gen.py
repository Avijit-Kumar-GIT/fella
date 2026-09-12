#!/usr/bin/env python3
"""Axis: self-verification. A natural, easy-to-make mistake causes a lookup
to come back empty, partial, or wrong -- does a careful system notice and
re-check with a corrected approach, or confidently report the bad result (or
an unrelated excuse) as if it were fine? Each case isolates a DIFFERENT
mechanism from the messiness axis (which tests upfront normalization, not
noticing-and-recovering): a date/time boundary trap, a column whose own name
states a unit conversion is needed, a free-text near-miss causing an empty
result, and a join-key type mismatch across two files. stdlib only.

    python3 gen.py
    agent_eval bench --dir bench/self-verification --models "ollama-cloud/gemma4:31b" --iters 3
"""
import csv
import json

R = round

# --- expenses_ts.csv: date has a time component; a naive `<=` string cutoff
#     silently excludes a late-timestamp last-day-of-month row -----------
EXPENSES = [
    ("2024-08-02 09:15:00", "groceries", 42.10),
    ("2024-08-10 12:00:00", "dining", 28.50),
    ("2024-08-18 08:30:00", "transport", 15.00),
    ("2024-08-31 21:14:00", "dining", 210.00),  # the late-timestamp trap row
    ("2024-09-03 10:00:00", "groceries", 38.00),
]
with open("expenses_ts.csv", "w", newline="") as f:
    w = csv.writer(f)
    w.writerow(["date", "category", "amount"])
    w.writerows(EXPENSES)

august_total = R(sum(a for d, c, a in EXPENSES if d.startswith("2024-08")), 2)

# --- bodyweight.csv: the column name itself states the unit -- weight_lbs --
BODYWEIGHT = [("2024-01-01", 165), ("2024-02-01", 168), ("2024-03-01", 172)]
with open("bodyweight.csv", "w", newline="") as f:
    w = csv.writer(f)
    w.writerow(["date", "weight_lbs"])
    w.writerows(BODYWEIGHT)

max_lbs = max(w_ for _, w_ in BODYWEIGHT)
max_kg = R(max_lbs * 0.453592, 1)

# --- spend_merchant.csv: real merchant spellings never literally "Amazon" --
MERCHANTS = [
    ("2024-05-01", "AMAZON.COM", 42.10), ("2024-05-14", "Amazon Prime", 14.99),
    ("2024-06-02", "AMAZON MKTPLACE", 29.99), ("2024-06-20", "Corner Market", 18.00),
]
with open("spend_merchant.csv", "w", newline="") as f:
    w = csv.writer(f)
    w.writerow(["date", "merchant", "amount"])
    w.writerows(MERCHANTS)

amazon_total = R(sum(a for d, m, a in MERCHANTS if "amazon" in m.lower()), 2)

# --- orders.csv + order_notes.csv: same order_id, different representation
#     ("007" zero-padded text vs 7 plain integer) -- a naive equality join
#     silently matches zero rows -------------------------------------------
ORDERS = [("007", "desk lamp", 34.99), ("012", "headphones", 89.00), ("019", "notebook set", 12.50)]
with open("orders.csv", "w", newline="") as f:
    w = csv.writer(f)
    w.writerow(["order_id", "item", "amount"])
    w.writerows(ORDERS)

NOTES = [(7, "gift for mom"), (12, "replaced under warranty"), (19, "back to school")]
with open("order_notes.csv", "w", newline="") as f:
    w = csv.writer(f)
    w.writerow(["order_id", "note"])
    w.writerows(NOTES)

largest_order = max(ORDERS, key=lambda o: o[2])
largest_order_note = next(n for oid, n in NOTES if str(oid) == largest_order[0].lstrip("0"))

cases = [
    (
        "date-boundary-off-by-one",
        "How much did I spend in total during August 2024?",
        ["expenses_ts.csv"],
        {"approx": [august_total, 0.5]},
        "date-time-boundary",
    ),
    (
        "unit-mislabeled-column",
        "What's the heaviest I've weighed, in kilograms?",
        ["bodyweight.csv"],
        {"approx": [max_kg, 0.3]},
        "column-name-unit-metadata",
    ),
    (
        "merchant-spelling-empty-result",
        "How much have I spent at Amazon in total?",
        ["spend_merchant.csv"],
        {"approx": [amazon_total, 0.5]},
        "near-miss-text-recovery",
    ),
    (
        "join-key-type-mismatch",
        "What note is attached to my largest order?",
        ["orders.csv", "order_notes.csv"],
        {"contains": [largest_order_note]},
        "join-key-representation",
    ),
]

DOMAIN = "spending"

with open("cases.jsonl", "w") as f:
    f.write(
        "# Axis: self-verification. Each case's naive/careless query returns an "
        "empty or wrong result; a careful system notices and recovers. Gold "
        "computed by bench/self-verification/gen.py.\n"
    )
    for cid, q, files, gold, tier in cases:
        rec = {
            "id": f"svf-{cid}",
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
        f.write(json.dumps(rec) + "\n")

print(f"wrote {len(cases)} self-verification cases")
print(f"  august_total={august_total} max_kg={max_kg} amazon_total={amazon_total} "
      f"largest_order={largest_order} note={largest_order_note!r}")

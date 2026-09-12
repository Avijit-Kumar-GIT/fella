#!/usr/bin/env python3
"""Axis: data messiness. Real personal-data traps in one column at a time --
inconsistent category casing/spelling, currency-as-text, mixed date formats,
a stray total row mixed into detail rows, unit inconsistency, mismatched
negative-number conventions, and an encoding oddity defeating exact match.

Each case isolates ONE trap so a miss is diagnostic. Gold is computed here
from the exact rows written below, not hand-typed. stdlib only.

    python3 gen.py
    agent_eval bench --dir bench/messiness --models "ollama-cloud/gemma4:31b" --iters 3
"""
import json

R = round

# --- messy_spend.csv: casing/near-dup categories, currency-as-text, mixed
#     dates, a stray TOTAL row, negative-number conventions, an encoding
#     oddity in the merchant field ------------------------------------------
# columns: date, amount, category, merchant
SPEND_ROWS = [
    ("2024-01-03", "$1,200.00", "Rent", "Landlord LLC"),
    ("2024-01-14", "45.20", "groceries", "Corner Market"),
    ("03/04/2024", "1,200", "HOUSING", "Landlord LLC"),          # slash date + casing
    ("2024-02-19", "88.10", "Grocery", "Corner Market"),         # near-dup label
    ("4 Mar 2024", "1200", "mortgage", "Landlord LLC"),          # prose date
    ("2024-03-11", "12.00", "transport", "Metro Card"),
    ("2024-04-03", "$1,250.00", "rent", "Landlord LLC"),
    ("2024-04-22", "52.00", "grocery store", "Corner Market"),   # near-dup label
    ("2024-05-03", "$1,250.00", "Rent", "Landlord LLC"),
    ("2024-05-18", "9.50", "transport", "Metro Card"),
    ("06/03/2024", "1250", "housing", "Landlord LLC"),
    ("2024-06-12", "-18.00", "groceries", "Corner Market"),      # refund, minus sign
    ("2024-06-20", "(22.50)", "groceries", "Corner Market"),     # refund, parens
    ("2024-07-03", "$1,250.00", "RENT", "Landlord LLC"),
    ("2024-07-15", "60.00", "Groceries", "Corner Market"),
    ("2024-07-09", "14.40", "dining", "Bob’s Diner"),       # curly apostrophe
    ("07/25/2024", "22.00", "transport", "Metro Card"),          # day=25 anchors MM/DD
    ("", "__TOTAL__", "TOTAL", ""),                               # stray summary row (filled below)
]

# --- weight_log.csv: unit inconsistency in one numeric column -------------
# columns: date, weight (mixed kg / lb, unitless assumed kg by convention)
WEIGHT_ROWS = [
    ("2024-01-01", "72"), ("2024-01-08", "71.5"), ("2024-01-15", "71.8"),
    ("2024-01-22", "160lb"), ("2024-01-29", "70.9"),   # 160lb ~= 72.57kg
    ("2024-02-05", "70.6"), ("2024-02-12", "155lb"),   # 155lb ~= 70.31kg
]


def dump_csv(name, header, rows):
    with open(name, "w", newline="") as f:
        f.write(",".join(header) + "\n")
        for r in rows:
            f.write(",".join(f'"{c}"' if "," in c or '"' in c else c for c in r) + "\n")


def parse_amount(a):
    a = a.strip().strip('"')
    neg = False
    if a.startswith("(") and a.endswith(")"):
        neg, a = True, a[1:-1]
    a = a.replace("$", "").replace(",", "")
    v = float(a)
    return -v if neg else v


def norm_cat(c):
    c = c.strip().lower()
    if c in ("groceries", "grocery", "grocery store"):
        return "groceries"
    if c in ("rent", "housing", "mortgage"):
        return "rent"
    return c


detail_rows = [r for r in SPEND_ROWS if r[2] != "TOTAL"]
rent_total = R(sum(parse_amount(a) for d, a, c, m in detail_rows if norm_cat(c) == "rent"), 2)
groceries_total = R(
    sum(parse_amount(a) for d, a, c, m in detail_rows if norm_cat(c) == "groceries"), 2
)
grand_total = R(sum(parse_amount(a) for d, a, c, m in detail_rows), 2)
# only literal "Rent" (not the near-dup/casing variants) -- the wrong-answer trap
rent_literal_only = R(sum(parse_amount(a) for d, a, c, m in detail_rows if c == "Rent"), 2)

# The TOTAL row states the real sum (a realistic ledger's total is accurate) --
# tests exclusion of a summary row from detail-row aggregates, not contradiction
# detection (that belongs to the self-verification axis, not this one).
SPEND_ROWS[-1] = ("", str(grand_total), "TOTAL", "")

dump_csv("messy_spend.csv", ["date", "amount", "category", "merchant"], SPEND_ROWS)
dump_csv("weight_log.csv", ["date", "weight"], WEIGHT_ROWS)

# isolates sign-parsing alone (not entangled with category normalization):
# one refund written with a leading minus, one with parens -- both must parse
# as negative.
refund_total = R(
    -sum(parse_amount(a) for d, a, c, m in detail_rows if parse_amount(a) < 0), 2
)

diner_count = sum(1 for d, a, c, m in detail_rows if "diner" in m.lower())

kg_per_lb = 0.453592


def parse_weight(w):
    w = w.strip()
    if w.endswith("lb"):
        return float(w[:-2]) * kg_per_lb
    return float(w)


avg_weight_kg = R(sum(parse_weight(w) for d, w in WEIGHT_ROWS) / len(WEIGHT_ROWS), 2)

cases = [
    (
        "categorical-casing",
        "How much did I spend on rent in total, according to messy_spend.csv?",
        ["messy_spend.csv"],
        {"approx": [rent_total, 0.5]},
        "categorical-normalization",
    ),
    (
        "near-duplicate-labels",
        "How much did I spend on groceries in total, according to messy_spend.csv?",
        ["messy_spend.csv"],
        {"approx": [groceries_total, 0.5]},
        "categorical-normalization",
    ),
    (
        "currency-as-text",
        "What's the largest single transaction amount in messy_spend.csv?",
        ["messy_spend.csv"],
        {"figures": [1250.0]},
        "numeric-coercion",
    ),
    (
        "mixed-date-formats",
        "How much did I spend in total during March 2024, according to messy_spend.csv?",
        ["messy_spend.csv"],
        # both the slash-date and prose-date March rows: 1200 (housing) + 1200 (mortgage) + 12 (transport)
        {"approx": [2412.0, 0.5]},
        "date-parsing",
    ),
    (
        "stray-total-row",
        "What's the single largest transaction in messy_spend.csv, and what's the grand total across all real transactions?",
        ["messy_spend.csv"],
        {"contains": ["1,250|1250", str(grand_total)]},
        "summary-row-exclusion",
    ),
    (
        "negative-conventions",
        "How much have I been refunded in total, according to messy_spend.csv?",
        ["messy_spend.csv"],
        {"approx": [refund_total, 0.5]},
        "numeric-sign-convention",
    ),
    (
        "encoding-oddity",
        "How many transactions were at Bob's Diner, according to messy_spend.csv?",
        ["messy_spend.csv"],
        {"figures": [float(diner_count)]},
        "text-normalization",
    ),
    (
        "unit-inconsistency",
        "What's my average weight this period, in kilograms, according to weight_log.csv?",
        ["weight_log.csv"],
        {"approx": [avg_weight_kg, 0.3]},
        "unit-normalization",
    ),
    (
        "wrong-answer-trap",
        "How much did I spend on exactly the category labeled 'Rent' (capital R, lowercase rest) in messy_spend.csv?",
        ["messy_spend.csv"],
        {"approx": [rent_literal_only, 0.5]},
        "literal-vs-normalized-control",
    ),
]

DOMAIN = "spending"

with open("cases.jsonl", "w") as f:
    f.write(
        "# Axis: messiness. Each case isolates one real-world data-quality trap. "
        "Gold computed by bench/messiness/gen.py from the exact rows written here.\n"
    )
    for cid, q, files, gold, tier in cases:
        rec = {
            "id": f"msy-{cid}",
            "question": q,
            "files": files,
            "gold": gold,
            "tier": tier,
            "domain": DOMAIN,
            "multifile": False,
            "n_files": len(files),
            "cluttered": False,
            "formats": ["csv"],
        }
        f.write(json.dumps(rec) + "\n")

print(f"wrote {len(cases)} messiness cases")
print(f"  rent_total={rent_total} rent_literal_only={rent_literal_only} "
      f"groceries_total={groceries_total} grand_total={grand_total} "
      f"diner_count={diner_count} avg_weight_kg={avg_weight_kg}")

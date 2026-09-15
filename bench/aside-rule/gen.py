#!/usr/bin/env python3
"""Aside-rule battery: tests `aside_rule` (agent.rs) directly. `aside_rule`
is `depth_rule`'s complement -- depth_rule handles a change/trend/
correlation/comparison question, aside_rule handles the plain single-figure
lookup depth_rule explicitly skips. It lets the model add ONE short second
sentence, but only when the same result already shows something genuinely
notable, and never at the cost of an extra query.

Two cases only, on purpose:
- `ar-worthy-rent`: a lookup where a real, checkable, notable fact sits
  right next to the asked figure (rent is ~59% of total 2024 spend --
  should trigger the aside).
- `ar-flat-utilities`: a lookup on an unremarkable middling category --
  nothing should trigger. `Gold::Figures` can't check for *absence* of
  added text (it only checks presence), so this case's gold is a floor,
  not the real check -- the real check is the A/B run against
  FELLA_PROMPT_DROP=aside_rule plus a manual read of the transcripts
  (EVAL_SHOW_ANSWERS=1). Broader regression coverage comes from re-running
  bench/analysis-depth/ and the in-code accuracy battery, not from growing
  this tier -- depth not breadth.

File copied from ../folder-qa-hard/spend.csv (same precedent
analysis-depth/gen.py set). Gold computed from the file, not hand-typed.

    python3 gen.py
    agent_eval bench --dir bench/aside-rule \
      --models "ollama-cloud/gemma4:31b" --iters 3
    FELLA_PROMPT_DROP=aside_rule agent_eval bench --dir bench/aside-rule \
      --models "ollama-cloud/gemma4:31b" --iters 3   # OFF, for the A/B
"""
import csv, json, shutil
from collections import defaultdict

R = round
SRC = "../folder-qa-hard"

shutil.copy(f"{SRC}/spend.csv", "spend.csv")

spend = list(csv.DictReader(open("spend.csv")))

by_cat_2024 = defaultdict(float)
for r in spend:
    if r["month"].startswith("2024"):
        by_cat_2024[r["category"]] += float(r["amount"])
by_cat_2024 = {c: R(v, 2) for c, v in by_cat_2024.items()}
total_2024 = R(sum(by_cat_2024.values()), 2)

rent_total = by_cat_2024["rent"]
rent_pct = R(100 * rent_total / total_2024, 1)
utilities_total = by_cat_2024["utilities"]

cases = [
    (
        "worthy-rent",
        "What was my total spending on rent in 2024?",
        {"contains": [str(rent_total), f"{rent_pct:.0f}%|dominant|largest|most of|majority"]},
        "aside-worthy",
    ),
    (
        "flat-utilities",
        "What was my total spending on utilities in 2024?",
        {"figures": [utilities_total]},
        "aside-flat",
    ),
]

with open("cases.jsonl", "w") as f:
    f.write(
        "# aside-rule battery: tests aside_rule's plain-lookup behavior directly "
        "-- one worthy case (a genuinely notable fact should trigger one extra "
        "sentence), one flat case (nothing should trigger). Real check is the "
        "A/B run against FELLA_PROMPT_DROP=aside_rule, not this gold alone. Gold "
        "computed by bench/aside-rule/gen.py from the copied file, not hand-typed.\n"
    )
    for cid, q, gold, tier in cases:
        rec = {
            "id": f"ar-{cid}",
            "question": q,
            "files": ["spend.csv"],
            "gold": gold,
            "tier": tier,
            "domain": "spending",
            "multifile": False,
            "n_files": 1,
            "formats": ["csv"],
        }
        f.write(json.dumps(rec) + "\n")

print(f"wrote {len(cases)} aside-rule cases")
print(f"  by_cat_2024={by_cat_2024}")
print(f"  total_2024={total_2024}  rent_total={rent_total}  rent_pct={rent_pct}")
print(f"  utilities_total={utilities_total}")

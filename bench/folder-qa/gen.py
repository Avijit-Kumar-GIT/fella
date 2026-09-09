#!/usr/bin/env python3
"""Regenerate the folder-QA battery: deterministic files + cases.jsonl with gold
answers computed here. stdlib only. Run from this directory:

    python3 gen.py

A stand-in for "a folder of a person's own files" — deliberately spread across
life domains, not just finances: spending, workouts, a reading log, trips, a
plain-text journal, plus a budget and an authors table for cross-file joins.
The questions cover numeric aggregates, categorical group-by / top-N, boolean
filters, ratings/averages, date ranges, free-text lookup and search, one
refusal (no forecasting), one that needs no tool, and a multi-file tier where
each case needs a JOIN or a table + a document.

    agent_eval bench --dir bench/folder-qa --models "ollama-cloud/gemma4:31b" --iters 3
"""
import csv, json, random

random.seed(7)
R = round


def dump(name, header, records):
    with open(name, "w", newline="") as f:
        w = csv.DictWriter(f, header)
        w.writeheader()
        w.writerows(records)


# --- spending -------------------------------------------------------------
cats = ["rent", "groceries", "transport", "utilities", "dining"]
base = {"rent": 1200, "groceries": 420, "transport": 95, "utilities": 140, "dining": 180}
spend = []
for y in (2023, 2024):
    for mo in range(1, 13):
        for c in cats:
            amt = R(base[c] * (1 + random.uniform(-0.12, 0.18)) + (60 if (c == "rent" and y == 2024) else 0), 2)
            spend.append({"month": f"{y}-{mo:02d}-01", "category": c, "amount": amt})
dump("spend.csv", ["month", "category", "amount"], spend)


def sp(pred):
    return R(sum(r["amount"] for r in spend if pred(r)), 2)


total_2024 = sp(lambda r: r["month"][:4] == "2024")
total_2023 = sp(lambda r: r["month"][:4] == "2023")
rent_2024 = sp(lambda r: r["category"] == "rent" and r["month"][:4] == "2024")
by_cat = {c: sp(lambda r, c=c: r["category"] == c) for c in cats}
top_cat = max(by_cat, key=by_cat.get)

# --- budget (shares `category` with spend — a same-named join key) -------
monthly_budget = {"rent": 1200, "groceries": 400, "transport": 90, "utilities": 130, "dining": 150}
dump("budget.csv", ["category", "monthly_budget"], [{"category": c, "monthly_budget": monthly_budget[c]} for c in cats])
actual_2024 = {c: sp(lambda r, c=c: r["category"] == c and r["month"][:4] == "2024") for c in cats}
most_over_cat = max(cats, key=lambda c: actual_2024[c] - 12 * monthly_budget[c])

# --- workouts ----------------------------------------------------------
acts = ["run", "bike", "swim", "yoga", "lift"]
workouts = []
for d in range(90):
    day = f"2024-{(d // 28) + 1:02d}-{(d % 28) + 1:02d}"
    a = random.choice(acts)
    workouts.append({"date": day, "activity": a, "minutes": random.randint(20, 75),
                     "distance_km": R(random.uniform(3, 12), 1) if a in ("run", "bike", "swim") else 0})
dump("workouts.csv", ["date", "activity", "minutes", "distance_km"], workouts)
wk_total_min = sum(r["minutes"] for r in workouts)
run_km = R(sum(r["distance_km"] for r in workouts if r["activity"] == "run"), 1)
wk_by_act = {a: sum(r["minutes"] for r in workouts if r["activity"] == a) for a in acts}
top_act = max(wk_by_act, key=wk_by_act.get)

# --- reading log -----------------------------------------------------
authors_tbl = [
    ("Ursula K. Le Guin", "American"), ("Kazuo Ishiguro", "British"),
    ("Chimamanda Ngozi Adichie", "Nigerian"), ("Italo Calvino", "Italian"),
    ("Jenny Odell", "American"), ("Zadie Smith", "British"),
]
dump("authors.csv", ["author", "nationality"], [{"author": a, "nationality": n} for a, n in authors_tbl])
titles = [
    ("The Dispossessed", "Ursula K. Le Guin", "scifi", 341, 5, "yes", "2024-02-11"),
    ("Klara and the Sun", "Kazuo Ishiguro", "scifi", 303, 4, "yes", "2024-03-20"),
    ("Half of a Yellow Sun", "Chimamanda Ngozi Adichie", "literary", 433, 5, "yes", "2024-05-02"),
    ("Invisible Cities", "Italo Calvino", "literary", 165, 4, "yes", "2024-01-08"),
    ("How to Do Nothing", "Jenny Odell", "nonfiction", 232, 3, "yes", "2024-06-19"),
    ("The Left Hand of Darkness", "Ursula K. Le Guin", "scifi", 304, 5, "no", ""),
    ("Intimations", "Zadie Smith", "essays", 97, 4, "no", ""),
    ("Cosmicomics", "Italo Calvino", "scifi", 153, 3, "no", ""),
]
books = [{"title": t, "author": a, "genre": g, "pages": p, "rating": r, "finished": fin, "finished_date": fd}
         for (t, a, g, p, r, fin, fd) in titles]
dump("books.csv", ["title", "author", "genre", "pages", "rating", "finished", "finished_date"], books)
finished_books = [b for b in books if b["finished"] == "yes"]
n_finished = len(finished_books)
avg_rating_finished = R(sum(b["rating"] for b in finished_books) / n_finished, 2)
pages_by_genre_fin = {}
for b in finished_books:
    pages_by_genre_fin[b["genre"]] = pages_by_genre_fin.get(b["genre"], 0) + b["pages"]
top_genre_pages = max(pages_by_genre_fin, key=pages_by_genre_fin.get)
longest_book = max(books, key=lambda b: b["pages"])
unfinished_titles = [b["title"] for b in books if b["finished"] == "no"]
nat_of = dict(authors_tbl)
pages_british = sum(b["pages"] for b in books if nat_of.get(b["author"]) == "British")

# --- trips -----------------------------------------------------------
trips = [
    ("Lisbon", "Portugal", 4, "leisure"), ("Berlin", "Germany", 3, "work"),
    ("Kyoto", "Japan", 6, "leisure"), ("Austin", "USA", 2, "work"),
    ("Oaxaca", "Mexico", 5, "leisure"), ("Toronto", "Canada", 3, "work"),
    ("Reykjavik", "Iceland", 4, "leisure"),
]
dump("trips.csv", ["place", "country", "nights", "purpose"], [
    {"place": p, "country": c, "nights": n, "purpose": pu} for (p, c, n, pu) in trips])
trip_nights_total = sum(t[2] for t in trips)
leisure_nights = sum(t[2] for t in trips if t[3] == "leisure")
n_countries = len({t[1] for t in trips})

# --- journal (free text) -------------------------------------------
journal_lines = [
    "2024-01-08  Finished Invisible Cities on the train. mood: content. Long walk after.",
    "2024-02-14  Rough day at work, deploy slipped. mood: stressed. Skipped the gym.",
    "2024-03-02  Morning run by the river, 8k. mood: good. Started planning the Kyoto trip.",
    "2024-04-19  Dentist. mood: fine. Cooked a big pot of dal. Booked flights to Kyoto.",
    "2024-05-02  Cried at the end of Half of a Yellow Sun. mood: moved. Called mum.",
    "2024-06-21  Longest day. mood: restless. Another run, legs heavy. Bought a new notebook.",
]
with open("journal.txt", "w") as f:
    f.write("Journal 2024\n\n" + "\n".join(journal_lines) + "\n")
stressed_days = sum(1 for ln in journal_lines if "mood: stressed" in ln)
run_journal_dates = {ln[:10] for ln in journal_lines if " run" in ln.lower()}
mins_on_journal_run_days = sum(r["minutes"] for r in workouts if r["date"] in run_journal_dates)

# -----------------------------------------------------------------------
cases = [
    # finance
    ("fin-total-2024", "How much did I spend in 2024?", ["spend.csv"], {"figures": [total_2024]}, "num-aggregate"),
    ("fin-rent-2024", "What was my total rent in 2024?", ["spend.csv"], {"figures": [rent_2024]}, "num-filter"),
    ("fin-top-cat", "Which category did I spend the most on overall?", ["spend.csv"], {"contains": [top_cat]}, "cat-groupby"),
    ("fin-yoy", "Did my spending go up or down from 2023 to 2024, and by how much?", ["spend.csv"], {"figures": [R(total_2024 - total_2023, 2)]}, "num-multistep"),
    # fitness
    ("fit-total-min", "How many minutes did I work out in total?", ["workouts.csv"], {"figures": [wk_total_min]}, "num-aggregate"),
    ("fit-run-km", "How many kilometres did I run in total?", ["workouts.csv"], {"figures": [run_km]}, "num-filter"),
    ("fit-top-activity", "Which activity did I spend the most time on?", ["workouts.csv"], {"contains": [top_act]}, "cat-groupby"),
    # reading
    ("read-finished-count", "How many books have I finished?", ["books.csv"], {"figures": [n_finished]}, "bool-filter"),
    ("read-avg-rating", "What is my average rating for the books I've finished?", ["books.csv"], {"approx": [avg_rating_finished, 0.05]}, "num-avg"),
    ("read-top-genre-pages", "Across the books I finished, which genre did I read the most pages of?", ["books.csv"], {"contains": [top_genre_pages]}, "cat-groupby"),
    ("read-longest", "What is the longest book in my list, by page count?", ["books.csv"], {"contains": [longest_book["title"].lower()]}, "text-max"),
    ("read-unfinished", "Which books have I not finished? List the titles.", ["books.csv"], {"contains": [t.lower() for t in unfinished_titles]}, "text-list"),
    # trips
    ("trip-total-nights", "How many nights did I spend travelling in total?", ["trips.csv"], {"figures": [trip_nights_total]}, "num-aggregate"),
    ("trip-leisure-nights", "How many of my travel nights were for leisure rather than work?", ["trips.csv"], {"figures": [leisure_nights]}, "cat-filter"),
    ("trip-countries", "How many different countries have I visited?", ["trips.csv"], {"figures": [n_countries]}, "distinct-count"),
    # journal / free text
    ("jrnl-stressed", "How many days in the journal do I describe my mood as stressed?", ["journal.txt"], {"figures": [stressed_days]}, "text-count"),
    ("jrnl-apr19", "What did I do on 2024-04-19 according to the journal?", ["journal.txt"], {"contains": ["dentist|dal|kyoto|flight"]}, "text-lookup"),
    ("jrnl-kyoto", "Does the journal mention planning or booking a trip to Kyoto?", ["journal.txt"], {"contains": ["yes|does|mentions|plann|book"]}, "text-search"),
    # multi-file: JOIN or table + document
    ("mf-most-over-budget", "Compare spend.csv against budget.csv (a monthly budget per category). In 2024, which single category was the most over its annual budget (actual minus 12x the monthly budget)?", ["spend.csv", "budget.csv"], {"contains": [most_over_cat]}, "mf-join-compare"),
    ("mf-pages-british", "Using books.csv and authors.csv, how many pages of books by British authors are in my list (finished or not)?", ["books.csv", "authors.csv"], {"figures": [pages_british]}, "mf-join-aggregate"),
    ("mf-run-days-minutes", "The journal notes some days I went for a run. On those dates, how many workout minutes does workouts.csv record in total?", ["journal.txt", "workouts.csv"], {"figures": [mins_on_journal_run_days]}, "mf-doc-join"),
    ("mf-rent-vs-budget", "Using spend.csv and budget.csv, was my 2024 rent over or under the yearly rent budget, and by how much?", ["spend.csv", "budget.csv"], {"figures": [R(rent_2024 - 12 * monthly_budget["rent"], 2)]}, "mf-join-compare"),
    # refusal + no-tool
    ("refusal", "Based on my reading log, how many books will I finish next year?", ["books.csv"], "refusal", "refusal"),
    ("notool", "What does the word 'anthology' mean?", [], "notool", "no-tool"),
]

with open("cases.jsonl", "w") as f:
    f.write("# folder-QA battery across life domains. Gold computed by bench/folder-qa/gen.py.\n")
    for cid, q, files, gold, tier in cases:
        f.write(json.dumps({"id": f"fqa-{cid}", "question": q, "files": files, "gold": gold, "tier": tier}) + "\n")

print(f"wrote {len(cases)} cases across "
      f"{len({t.split('-')[0] for _, _, _, _, t in cases})} metric shapes")
print(f"  fin: total_2024={total_2024} rent_2024={rent_2024} top_cat={top_cat}")
print(f"  fit: min={wk_total_min} run_km={run_km} top_act={top_act}")
print(f"  read: finished={n_finished} avg_rating={avg_rating_finished} top_genre={top_genre_pages} "
      f"longest={longest_book['title']!r} unfinished={unfinished_titles}")
print(f"  trip: nights={trip_nights_total} leisure={leisure_nights} countries={n_countries}")
print(f"  jrnl: stressed_days={stressed_days} run_dates={sorted(run_journal_dates)}")
print(f"  mf: most_over={most_over_cat} pages_british={pages_british} "
      f"run_day_min={mins_on_journal_run_days} rent_over={R(rent_2024 - 12*monthly_budget['rent'], 2)}")

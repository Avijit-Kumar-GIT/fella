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

It also exercises every file type Fella ingests, one file per format:
CSV, TSV (screen_time.tsv), JSON array (contacts.json), NDJSON (sleep.jsonl),
Markdown (goals.md), XLSX (subscriptions.xlsx), PDF (lease.pdf) — with
single-file cases per format and cross-format joins (JSON x CSV, MD + CSV,
NDJSON x TSV, PDF + CSV).

    agent_eval bench --dir bench/folder-qa --models "ollama-cloud/gemma4:31b" --iters 3
"""
import csv, json, random

random.seed(7)
R = round


def dump(name, header, records, delim=","):
    with open(name, "w", newline="") as f:
        w = csv.DictWriter(f, header, delimiter=delim)
        w.writeheader()
        w.writerows(records)


def dump_xlsx(name, sheet, header, rows):
    """One-sheet .xlsx via hand-written OOXML (stdlib zipfile, no openpyxl).
    Same approach as src-tauri/tests/fixtures/make_messy_ledger.py."""
    import zipfile

    def col(i):
        return chr(ord("A") + i)

    def cell(r, c, v):
        ref = f"{col(c)}{r + 1}"
        if isinstance(v, (int, float)):
            return f'<c r="{ref}"><v>{v}</v></c>'
        esc = str(v).replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")
        return f'<c r="{ref}" t="inlineStr"><is><t xml:space="preserve">{esc}</t></is></c>'

    data = [header] + [list(r) for r in rows]
    sheet_xml = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>'
        + "".join(
            f'<row r="{i + 1}">' + "".join(cell(i, c, v) for c, v in enumerate(row)) + "</row>"
            for i, row in enumerate(data)
        )
        + "</sheetData></worksheet>"
    )
    parts = {
        "[Content_Types].xml": (
            '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
            '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
            '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
            '<Default Extension="xml" ContentType="application/xml"/>'
            '<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>'
            '<Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>'
            "</Types>"
        ),
        "_rels/.rels": (
            '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
            '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
            '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>'
        ),
        "xl/workbook.xml": (
            '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
            '<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" '
            'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">'
            f'<sheets><sheet name="{sheet}" sheetId="1" r:id="rId1"/></sheets></workbook>'
        ),
        "xl/_rels/workbook.xml.rels": (
            '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
            '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
            '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>'
        ),
        "xl/worksheets/sheet1.xml": sheet_xml,
    }
    with zipfile.ZipFile(name, "w", zipfile.ZIP_DEFLATED) as z:
        for p, content in parts.items():
            z.writestr(p, content)


def dump_pdf(name, lines):
    """Minimal single-page PDF: uncompressed Helvetica text, correct xref.
    pdf_extract (Fella's PDF reader) pulls the text back out line by line."""
    def esc(s):
        return s.replace("\\", "\\\\").replace("(", "\\(").replace(")", "\\)")

    body = ["BT", "/F1 12 Tf", "14 TL", "72 720 Td"]
    for i, ln in enumerate(lines):
        if i:
            body.append("T*")
        body.append(f"({esc(ln)}) Tj")
    body.append("ET")
    stream = "\n".join(body)
    objs = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] "
        "/Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
        f"<< /Length {len(stream.encode())} >>\nstream\n{stream}\nendstream",
    ]
    out = b"%PDF-1.4\n"
    offs = []
    for n, o in enumerate(objs, 1):
        offs.append(len(out))
        out += f"{n} 0 obj\n{o}\nendobj\n".encode()
    xref_pos = len(out)
    out += f"xref\n0 {len(objs) + 1}\n".encode() + b"0000000000 65535 f \n"
    for off in offs:
        out += f"{off:010d} 00000 n \n".encode()
    out += f"trailer\n<< /Size {len(objs) + 1} /Root 1 0 R >>\nstartxref\n{xref_pos}\n%%EOF\n".encode()
    with open(name, "wb") as f:
        f.write(out)


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

# --- screen time (TSV) -------------------------------------------------
apps = ["browser", "messages", "maps", "news", "music", "podcasts"]
screen = []
for d in range(60):
    day = f"2024-{(d // 28) + 1:02d}-{(d % 28) + 1:02d}"
    for ap in apps:
        screen.append({"date": day, "app": ap, "minutes": random.randint(0, 55)})
dump("screen_time.tsv", ["date", "app", "minutes"], screen, delim="\t")
screen_total_min = sum(r["minutes"] for r in screen)
screen_by_app = {a: sum(r["minutes"] for r in screen if r["app"] == a) for a in apps}
screen_top_app = max(screen_by_app, key=screen_by_app.get)
screen_by_date = {}
for r in screen:
    screen_by_date[r["date"]] = screen_by_date.get(r["date"], 0) + r["minutes"]

# --- sleep (NDJSON) --------------------------------------------------
sleep = []
for d in range(60):
    day = f"2024-{(d // 28) + 1:02d}-{(d % 28) + 1:02d}"
    sleep.append({"date": day, "hours": R(random.uniform(5.2, 8.4), 1), "quality": random.randint(1, 5)})
with open("sleep.jsonl", "w") as f:
    for row in sleep:
        f.write(json.dumps(row) + "\n")
sleep_avg_hours = R(sum(r["hours"] for r in sleep) / len(sleep), 2)
worst_nights = [r["date"] for r in sleep if r["quality"] == 1]
avg_screen_worst = R(sum(screen_by_date[d] for d in worst_nights) / len(worst_nights), 1)

# --- contacts (JSON array of objects) -----------------------------
contacts = [
    {"name": "Ada", "country": "Portugal", "tier": "close", "last_seen_days": 12},
    {"name": "Bruno", "country": "Germany", "tier": "work", "last_seen_days": 40},
    {"name": "Chen", "country": "Japan", "tier": "close", "last_seen_days": 95},
    {"name": "Dara", "country": "USA", "tier": "family", "last_seen_days": 6},
    {"name": "Eze", "country": "Nigeria", "tier": "close", "last_seen_days": 210},
    {"name": "Fen", "country": "Canada", "tier": "work", "last_seen_days": 33},
    {"name": "Gita", "country": "India", "tier": "family", "last_seen_days": 18},
]
with open("contacts.json", "w") as f:
    json.dump(contacts, f, indent=2)
out_of_touch = max(contacts, key=lambda c: c["last_seen_days"])["name"]
visited_countries = {t[1] for t in trips}
contacts_in_visited = sorted(c["name"] for c in contacts if c["country"] in visited_countries)

# --- goals (Markdown document) -----------------------------------
run_goal_km = 500
with open("goals.md", "w") as f:
    f.write(
        "# Goals for this year\n\n"
        "- **Reading** - finish 20 books. Tracked in books.csv.\n"
        f"- **Running** - run {run_goal_km} km total over the year. Log is workouts.csv.\n"
        "- **Travel** - take at least 3 leisure trips.\n"
        "- **Sleep** - average 7.5 hours a night.\n"
        "- **Dining out** - keep it under $150 in any single month.\n\n"
        "Of all of these, running is the one I am furthest behind on.\n"
    )
run_short_km = R(run_goal_km - run_km, 1)

# --- subscriptions (XLSX) -------------------------------------------
subs = [
    ("cloud storage", 2.99, "utilities", "yes"),
    ("music streaming", 10.99, "entertainment", "yes"),
    ("news site", 5.00, "news", "no"),
    ("vpn", 3.33, "utilities", "yes"),
    ("video streaming", 15.49, "entertainment", "yes"),
    ("password manager", 2.50, "utilities", "yes"),
    ("magazine", 6.00, "news", "no"),
]
dump_xlsx("subscriptions.xlsx", "Subs", ["service", "monthly_cost", "category", "active"], subs)
subs_active = [s for s in subs if s[3] == "yes"]
subs_active_cost = R(sum(s[1] for s in subs_active), 2)
subs_by_cat = {}
for _svc, cost, cat, _a in subs_active:
    subs_by_cat[cat] = R(subs_by_cat.get(cat, 0) + cost, 2)
subs_top_cat = max(subs_by_cat, key=subs_by_cat.get)

# --- lease (PDF document) ---------------------------------------
lease_rent, lease_deposit = 1260, 1890
dump_pdf("lease.pdf", [
    "Residential Lease Summary",
    "",
    "Tenant: A. Kumar",
    "Property: 14 Maple Court, Unit 3",
    f"Monthly rent: ${lease_rent}",
    "Lease term: 12 months, starting 2024-01-01",
    f"Security deposit: ${lease_deposit}",
    "Landlord contact: property@example.com",
    "Pets: allowed, one cat (pet fee waived)",
    "Parking: one assigned space, number 17",
    "Renewal: month-to-month after the initial term",
])
rent_paid_avg_month = R(rent_2024 / 12, 2)
lease_vs_paid = R(rent_paid_avg_month - lease_rent, 2)

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
    # screen time (TSV)
    ("screen-total", "How many minutes of screen time do I have logged in total?", ["screen_time.tsv"], {"figures": [screen_total_min]}, "num-aggregate"),
    ("screen-top-app", "Which app do I spend the most screen time in?", ["screen_time.tsv"], {"contains": [screen_top_app]}, "cat-groupby"),
    # sleep (NDJSON)
    ("sleep-avg-hours", "What is my average nightly sleep, in hours?", ["sleep.jsonl"], {"approx": [sleep_avg_hours, 0.1]}, "num-avg"),
    # contacts (JSON array)
    ("contacts-out-of-touch", "Which contact have I gone the longest without seeing?", ["contacts.json"], {"contains": [out_of_touch.lower()]}, "text-max"),
    # goals (Markdown)
    ("goals-most-behind", "According to my goals note, which goal am I furthest behind on?", ["goals.md"], {"contains": ["run"]}, "text-lookup"),
    # subscriptions (XLSX)
    ("subs-active-cost", "What do my active subscriptions cost me per month in total?", ["subscriptions.xlsx"], {"approx": [subs_active_cost, 0.02]}, "num-filter"),
    ("subs-top-category", "Which category of subscription costs me the most per month, counting only active ones?", ["subscriptions.xlsx"], {"contains": [subs_top_cat]}, "cat-groupby"),
    # lease (PDF)
    ("pdf-rent", "What is the monthly rent on my lease?", ["lease.pdf"], {"figures": [lease_rent]}, "text-lookup"),
    # cross-format multi-file
    ("xf-contacts-visited", "Which of my contacts live in a country I have travelled to? Use contacts.json and trips.csv.", ["contacts.json", "trips.csv"], {"contains": [n.lower() for n in contacts_in_visited]}, "mf-join-filter"),
    ("xf-run-goal", "My goals note sets a running target for the year. Based on workouts.csv, how many kilometres short of it am I?", ["goals.md", "workouts.csv"], {"figures": [run_short_km]}, "mf-doc-join"),
    ("xf-sleep-screen", "On the nights I rated sleep quality 1, what was my average total screen time that day? Use sleep.jsonl and screen_time.tsv.", ["sleep.jsonl", "screen_time.tsv"], {"approx": [avg_screen_worst, 2.0]}, "mf-join-aggregate"),
    ("xf-lease-vs-spend", "My lease PDF states a monthly rent. Across 2024, spend.csv records what I actually paid for rent each month. On average per month, did I pay more or less than the lease amount, and by how much?", ["lease.pdf", "spend.csv"], {"approx": [abs(lease_vs_paid), 0.6]}, "mf-doc-join"),
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
print(f"  screen(tsv): total_min={screen_total_min} top_app={screen_top_app} "
      f"worst_night_screen_avg={avg_screen_worst}")
print(f"  sleep(jsonl): avg_hours={sleep_avg_hours} n_worst_nights={len(worst_nights)}")
print(f"  contacts(json): out_of_touch={out_of_touch} in_visited={contacts_in_visited}")
print(f"  goals(md): run_goal={run_goal_km} run_short={run_short_km}")
print(f"  subs(xlsx): active_cost={subs_active_cost} top_cat={subs_top_cat} by_cat={subs_by_cat}")
print(f"  lease(pdf): rent={lease_rent} deposit={lease_deposit} "
      f"paid_avg_month={rent_paid_avg_month} vs_lease={lease_vs_paid}")

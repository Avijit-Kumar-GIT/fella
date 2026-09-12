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
            # fixed timestamp so regenerating the battery is byte-reproducible
            zi = zipfile.ZipInfo(p, date_time=(1980, 1, 1, 0, 0, 0))
            zi.compress_type = zipfile.ZIP_DEFLATED
            z.writestr(zi, content)


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
# per-app ranges so the top-N by minutes is well separated (not a coin flip):
# browser > music > messages > news > maps > podcasts by a clear margin.
app_range = {"browser": (30, 65), "music": (25, 55), "messages": (15, 45),
             "news": (10, 35), "maps": (2, 22), "podcasts": (0, 18)}
apps = list(app_range)
screen = []
for d in range(60):
    day = f"2024-{(d // 28) + 1:02d}-{(d % 28) + 1:02d}"
    for ap in apps:
        lo, hi = app_range[ap]
        screen.append({"date": day, "app": ap, "minutes": random.randint(lo, hi)})
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

import calendar

# --- rent ledger: named-month dates, not ISO — the exact shape of the
# reported bug (strftime('%Y-%m', 'Aug 1, 2026') is NULL; ingest must
# normalize this to ISO at sniff time so GROUP BY month actually groups) ---
rent_ledger = []
rent_ledger_by_month = {}
for mo in range(1, 13):
    amt = R(1450 + (55 if mo >= 7 else 0) + random.uniform(-20, 20), 2)
    rent_ledger.append({"date": f"{calendar.month_abbr[mo]} 1, 2024", "rent": amt})
    rent_ledger_by_month[f"2024-{mo:02d}"] = amt
dump("rent_ledger.csv", ["date", "rent"], rent_ledger)
rent_ledger_total = R(sum(r["rent"] for r in rent_ledger), 2)
rent_ledger_avg = R(rent_ledger_total / 12, 2)
_rl_sorted = sorted(rent_ledger_by_month.items(), key=lambda kv: -kv[1])
rent_ledger_peak_ym, rent_ledger_peak_amt = _rl_sorted[0]
rent_ledger_peak_mn = calendar.month_name[int(rent_ledger_peak_ym[5:7])].lower()
rent_ledger_peak_alts = (
    f"{rent_ledger_peak_ym}|{rent_ledger_peak_mn}|{calendar.month_abbr[int(rent_ledger_peak_ym[5:7])].lower()}"
)

# --- extra golds (no new files, no new random draws) -----------------

# spending
spend_2024_by_month = {}
for r in spend:
    if r["month"][:4] == "2024":
        spend_2024_by_month[r["month"]] = R(spend_2024_by_month.get(r["month"], 0) + r["amount"], 2)
_mo_sorted = sorted(spend_2024_by_month.items(), key=lambda kv: -kv[1])
peak_month_2024, peak_month_2024_amt = _mo_sorted[0]
peak_month_gap = R(peak_month_2024_amt - _mo_sorted[1][1], 2)
peak_mn = calendar.month_name[int(peak_month_2024[5:7])].lower()
peak_month_alts = f"{peak_month_2024[:7]}|{peak_month_2024}|{peak_mn}"
avg_month_2024 = R(total_2024 / 12, 2)
cat_rank = sorted(by_cat, key=by_cat.get, reverse=True)
second_cat = cat_rank[1]
groceries_2023 = sp(lambda r: r["category"] == "groceries" and r["month"][:4] == "2023")
groceries_2024 = sp(lambda r: r["category"] == "groceries" and r["month"][:4] == "2024")
groceries_delta = R(groceries_2024 - groceries_2023, 2)
q1_2024 = sp(lambda r: r["month"][:4] == "2024" and r["month"][5:7] in ("01", "02", "03"))
dining_2024_rows = [r for r in spend if r["category"] == "dining" and r["month"][:4] == "2024"]
dining_over_cap = sum(1 for r in dining_2024_rows if r["amount"] > 150)

# workouts
n_activities = len({r["activity"] for r in workouts})
avg_session_min = R(wk_total_min / len(workouts), 1)
total_distance = R(sum(r["distance_km"] for r in workouts), 1)
longest_workout_min = max(r["minutes"] for r in workouts)
n_runs = sum(1 for r in workouts if r["activity"] == "run")

# reading
finished_pages = sum(b["pages"] for b in finished_books)
n_genres = len({b["genre"] for b in books})
n_rated5 = sum(1 for b in books if b["rating"] == 5)
pages_by_nat = {}
for b in books:
    pages_by_nat[nat_of.get(b["author"], "?")] = pages_by_nat.get(nat_of.get(b["author"], "?"), 0) + b["pages"]
top_nat_pages = max(pages_by_nat, key=pages_by_nat.get)

# trips
avg_nights = R(trip_nights_total / len(trips), 2)
longest_trip_place = max(trips, key=lambda t: t[2])[0]
visited_contact_countries = {c["country"] for c in contacts if c["country"] in visited_countries}
nights_visited_contacts = sum(t[2] for t in trips if t[1] in visited_contact_countries)

# journal
n_journal_entries = len(journal_lines)

# screen / sleep
screen_days_over_150 = sum(1 for v in screen_by_date.values() if v > 150)
screen_second_app = sorted(screen_by_app, key=screen_by_app.get, reverse=True)[1]
sleep_at_least_8 = sum(1 for r in sleep if r["hours"] >= 8.0)

# contacts
n_contact_countries = len({c["country"] for c in contacts})
n_close = sum(1 for c in contacts if c["tier"] == "close")

# subscriptions
n_subs_inactive = len(subs) - len(subs_active)
cheapest_active = min(subs_active, key=lambda s: s[1])[0]

# lease
lease_parking = 17

# --- batch 3 golds: hard/diverse edge cases, no new files ---------------
# empty-result filters (category/period genuinely absent from the data —
# the correct answer is zero/none, not a hallucinated figure)
entertainment_spend = sp(lambda r: r["category"] == "entertainment")
transport_2022 = sp(lambda r: r["category"] == "transport" and r["month"][:4] == "2022")
# boolean + categorical combined filter
scifi_unfinished = sum(1 for b in books if b["genre"] == "scifi" and b["finished"] == "no")
rated5_titles = sorted(b["title"].lower() for b in books if b["rating"] == 5)
# multi-year aggregate
total_2023_2024 = R(total_2023 + total_2024, 2)
# signed over/under budget (can land either side of zero)
utilities_over_budget = R(actual_2024["utilities"] - 12 * monthly_budget["utilities"], 2)
# two files sharing a column name ("minutes") with unrelated meanings —
# stresses the join not silently summing across both
screen_vs_workout_gap = R(abs(screen_total_min - wk_total_min), 1)

# --- distractor files (real-looking, no question needs them) --------
receipts_old = [
    {"date": f"2019-{random.randint(1, 12):02d}-{random.randint(1, 28):02d}",
     "vendor": random.choice(["hardware store", "cafe", "pharmacy", "bookshop", "petrol"]),
     "total": R(random.uniform(4, 90), 2)}
    for _ in range(40)
]
dump("receipts_2019.csv", ["date", "vendor", "total"], receipts_old)
with open("old_notes.md", "w") as f:
    f.write(
        "# Scratch notes (archive)\n\n"
        "Old apartment checklist, mostly done. Call the internet company. Return "
        "the library books. Cancel the gym trial before the 30th.\n\n"
        "Ideas parking lot: learn to make bread, repot the plants, digitise the "
        "photo box.\n"
    )
with open("playlist.json", "w") as f:
    json.dump([
        {"track": "Teardrop", "artist": "Massive Attack", "plays": 42},
        {"track": "Redbone", "artist": "Childish Gambino", "plays": 31},
        {"track": "Nightcall", "artist": "Kavinsky", "plays": 27},
        {"track": "Roygbiv", "artist": "Boards of Canada", "plays": 55},
        {"track": "Svefn-g-englar", "artist": "Sigur Ros", "plays": 19},
    ], f, indent=2)

# extra files that no case names, dropped into ~1/6 of cases as clutter so
# the model has to pick the right files rather than being handed only them.
DISTRACT = ["receipts_2019.csv", "old_notes.md", "playlist.json"]


def clut(files, *extra):
    return list(files) + list(extra) + DISTRACT


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

    # --- batch 2: more shapes, ~1/6 with a cluttered folder ---
    # spending
    ("fin-avg-month-2024", "On average, how much did I spend per month in 2024?", clut(["spend.csv"]), {"approx": [avg_month_2024, 1.0]}, "num-avg"),
    ("fin-peak-month-2024", "Which month of 2024 did I spend the most in?", clut(["spend.csv"], "budget.csv"), {"contains": [peak_month_alts]}, "temporal-max"),
    ("fin-second-cat", "Which category is my second-biggest area of spending overall?", ["spend.csv"], {"contains": [second_cat]}, "cat-rank"),
    ("fin-groceries-delta", "How did my grocery spending change from 2023 to 2024, in dollars?", ["spend.csv"], {"figures": [groceries_delta]}, "num-multistep"),
    ("fin-q1-2024", "How much did I spend in the first quarter of 2024 (January through March)?", ["spend.csv"], {"figures": [q1_2024]}, "num-filter"),
    # workouts
    ("fit-distinct-activities", "How many different types of activity are in my workout log?", ["workouts.csv"], {"figures": [n_activities]}, "distinct-count"),
    ("fit-avg-session", "On average, how many minutes is one of my workouts?", ["workouts.csv"], {"approx": [avg_session_min, 0.5]}, "num-avg"),
    ("fit-total-distance", "What is the total distance, in km, across every workout that logged one?", clut(["workouts.csv"]), {"approx": [total_distance, 1.0]}, "num-aggregate"),
    ("fit-longest", "What was my single longest workout, in minutes?", ["workouts.csv"], {"figures": [longest_workout_min]}, "num-max"),
    ("fit-run-count", "How many separate runs are in the log?", ["workouts.csv"], {"figures": [n_runs]}, "num-filter"),
    # reading
    ("read-finished-pages", "How many pages have I read in total across the books I finished?", ["books.csv"], {"figures": [finished_pages]}, "num-aggregate"),
    ("read-genres", "How many distinct genres are in my reading list?", clut(["books.csv"], "authors.csv"), {"figures": [n_genres]}, "distinct-count"),
    ("read-rated5", "How many books did I rate 5?", ["books.csv"], {"figures": [n_rated5]}, "num-filter"),
    # trips
    ("trip-avg-nights", "On average, how many nights was each of my trips?", ["trips.csv"], {"approx": [avg_nights, 0.1]}, "num-avg"),
    ("trip-longest", "Which trip was the longest?", clut(["trips.csv"]), {"contains": [longest_trip_place.lower()]}, "text-max"),
    # journal
    ("jrnl-entries", "How many dated entries are in the journal?", clut(["journal.txt"]), {"figures": [n_journal_entries]}, "text-count"),
    # screen / sleep
    ("screen-days-over-150", "On how many days did my total screen time (all apps combined) exceed 150 minutes?", ["screen_time.tsv"], {"figures": [screen_days_over_150]}, "num-filter"),
    ("screen-second-app", "Which app is my second-heaviest by total screen time?", clut(["screen_time.tsv"]), {"contains": [screen_second_app]}, "cat-rank"),
    ("sleep-at-least-8", "How many nights did I sleep at least 8 hours?", ["sleep.jsonl"], {"figures": [sleep_at_least_8]}, "num-filter"),
    # contacts
    ("contacts-countries", "How many different countries do my contacts live in?", clut(["contacts.json"]), {"figures": [n_contact_countries]}, "distinct-count"),
    ("contacts-close-count", "How many of my contacts are in the 'close' tier?", ["contacts.json"], {"figures": [n_close]}, "cat-filter"),
    # subscriptions
    ("subs-inactive-count", "How many of my subscriptions are inactive?", ["subscriptions.xlsx"], {"figures": [n_subs_inactive]}, "num-filter"),
    ("subs-cheapest", "What is my cheapest active subscription?", clut(["subscriptions.xlsx"]), {"contains": [cheapest_active]}, "text-min"),
    # lease
    ("pdf-deposit", "How much was the security deposit on my lease?", ["lease.pdf"], {"figures": [lease_deposit]}, "text-lookup"),
    ("pdf-parking", "Which parking space number is assigned to me in the lease?", clut(["lease.pdf"]), {"figures": [lease_parking]}, "text-lookup"),
    # cross-format multi-file
    ("xf-nationality-pages", "Using books.csv and authors.csv, which author nationality accounts for the most pages in my reading list?", ["books.csv", "authors.csv"], {"contains": [top_nat_pages.lower()]}, "mf-join-groupby"),
    ("xf-contacts-nights", "For the contacts who live in a country I have visited, how many trip-nights did I spend in those countries in total? Use contacts.json and trips.csv.", clut(["contacts.json", "trips.csv"]), {"figures": [nights_visited_contacts]}, "mf-join-aggregate"),
    ("xf-dining-cap-months", "My goals note sets a monthly dining-out cap. In how many months of 2024 did my dining spend break it? Use goals.md and spend.csv.", ["goals.md", "spend.csv"], {"figures": [dining_over_cap]}, "mf-doc-join"),
    # --- batch 3: date-normalization, charting, and hard/diverse edge cases ---
    # rent ledger: named-month dates (the exact reported "Month: (blank)" bug)
    ("rentl-total", "How much rent have I paid in total this year, according to rent_ledger.csv?", ["rent_ledger.csv"], {"figures": [rent_ledger_total]}, "num-aggregate-nonstd-date"),
    ("rentl-avg", "What's the average monthly rent from rent_ledger.csv?", ["rent_ledger.csv"], {"approx": [rent_ledger_avg, 1.0]}, "num-avg-nonstd-date"),
    ("rentl-peak-month", "Break down my rent by month from rent_ledger.csv — which month was the most expensive?", ["rent_ledger.csv"], {"contains": [rent_ledger_peak_alts]}, "temporal-groupby-nonstd-date"),
    ("rentl-chart", "Chart my rent by month using rent_ledger.csv, and tell me the total for the year.", ["rent_ledger.csv"], {"figures": [rent_ledger_total]}, "chart-trend"),
    # charting: category breakdown
    ("fin-chart-cat", "Show me a chart of my spending by category in 2024, and tell me which category was the biggest.", ["spend.csv"], {"contains": [top_cat]}, "chart-breakdown"),
    # empty results — the data genuinely has none, answer must say so not invent one
    ("fin-empty-category", "How much did I spend on 'entertainment', according to spend.csv?", ["spend.csv"], {"approx": [entertainment_spend, 0.01]}, "empty-filter"),
    ("fin-empty-2022", "How much did I spend on transport in 2022?", ["spend.csv"], {"approx": [transport_2022, 0.01]}, "empty-filter-date"),
    # combined boolean + categorical filter
    ("read-scifi-unfinished", "How many science fiction books on my list have I not finished?", ["books.csv"], {"figures": [scifi_unfinished]}, "bool-cat-filter"),
    ("read-rated5-list", "Which books did I rate 5 out of 5? List their titles.", ["books.csv"], {"contains": rated5_titles}, "text-list-rating"),
    # multi-year aggregate
    ("fin-multiyear-total", "How much have I spent in total across 2023 and 2024 combined?", ["spend.csv"], {"figures": [total_2023_2024]}, "num-aggregate-multiyear"),
    # signed join result (can land on either side of zero)
    ("mf-utilities-signed", "Using spend.csv and budget.csv, was I over or under my annual utilities budget (actual minus 12x the monthly budget) in 2024, and by how much?", ["spend.csv", "budget.csv"], {"figures": [utilities_over_budget]}, "mf-join-compare-signed"),
    # join across two files that share a column name ("minutes") with unrelated meanings
    ("mf-screen-vs-workout", "Which did I spend more total time on — screen time or working out — and by how many minutes? Use screen_time.tsv and workouts.csv.", ["screen_time.tsv", "workouts.csv"], {"figures": [screen_vs_workout_gap]}, "mf-join-colliding-cols"),
    # case-insensitive text search
    ("subs-video-ci", "Do I have a subscription called 'VIDEO STREAMING'? If so, what does it cost per month?", ["subscriptions.xlsx"], {"contains": ["15.49"]}, "text-search-ci"),

    # refusal (no forecasting — the loop must decline, not compute an estimate)
    ("refusal", "Based on my reading log, how many books will I finish next year?", ["books.csv"], "refusal", "refusal"),
    ("refusal-spend", "Given my 2024 spending, what will my total grocery bill be next month?", ["spend.csv"], "refusal", "refusal"),
    ("refusal-trips", "Based on my travel history, how many trips will I take in 2025?", clut(["trips.csv"]), "refusal", "refusal"),
    # no-tool
    ("notool", "What does the word 'anthology' mean?", [], "notool", "no-tool"),
]

# --- taxonomy: primary life-data domain per case ----------------------
# so a future, harder battery slots into the same schema and difficulty
# (which is *measured* from results, not assigned here) stays comparable.
DOMAIN_BY_PREFIX = {
    "fin": "spending", "fit": "fitness", "read": "reading", "trip": "travel",
    "jrnl": "journal", "screen": "screen-time", "sleep": "sleep",
    "contacts": "contacts", "goals": "goals", "subs": "subscriptions",
    "pdf": "housing", "rentl": "housing",
}
DOMAIN_BY_ID = {  # multi-file / cross-format cases get their primary domain
    "fqa-mf-most-over-budget": "spending", "fqa-mf-pages-british": "reading",
    "fqa-mf-run-days-minutes": "fitness", "fqa-mf-rent-vs-budget": "spending",
    "fqa-xf-contacts-visited": "contacts", "fqa-xf-run-goal": "fitness",
    "fqa-xf-sleep-screen": "sleep", "fqa-xf-lease-vs-spend": "housing",
    "fqa-xf-nationality-pages": "reading", "fqa-xf-contacts-nights": "contacts",
    "fqa-xf-dining-cap-months": "spending",
    "fqa-mf-utilities-signed": "spending", "fqa-mf-screen-vs-workout": "fitness",
    "fqa-refusal": "reading", "fqa-refusal-spend": "spending",
    "fqa-refusal-trips": "travel", "fqa-notool": "general",
}


def _ext(name):
    return name.rsplit(".", 1)[-1].lower() if "." in name else ""


with open("cases.jsonl", "w") as f:
    f.write("# folder-QA battery across life domains. Gold + taxonomy from bench/folder-qa/gen.py.\n")
    for cid, q, files, gold, tier in cases:
        fid = f"fqa-{cid}"
        real = [x for x in files if x not in DISTRACT]
        rec = {
            "id": fid, "question": q, "files": files, "gold": gold, "tier": tier,
            "domain": DOMAIN_BY_ID.get(fid) or DOMAIN_BY_PREFIX[cid.split("-")[0]],
            "multifile": tier.startswith("mf"),
            "n_files": len(real),
            "cluttered": any(d in files for d in DISTRACT),
            "formats": sorted({_ext(x) for x in real if _ext(x)}),
        }
        f.write(json.dumps(rec) + "\n")

n_clut = sum(1 for _, _, files, _, _ in cases if any(d in files for d in DISTRACT))
n_mf = sum(1 for _, _, _, _, t in cases if t.startswith("mf"))
_doms = {DOMAIN_BY_ID.get(f"fqa-{c}") or DOMAIN_BY_PREFIX[c.split("-")[0]] for c, *_ in cases}
print(f"wrote {len(cases)} cases · {len({t for _, _, _, _, t in cases})} tier labels · "
      f"{len(_doms)} domains · {n_mf} multi-file · {n_clut} cluttered · 3 distractor files")
print(f"  b2: peak_month={peak_month_2024}(${peak_month_2024_amt}, gap to #2 ${peak_month_gap}) "
      f"avg_month={avg_month_2024} 2nd_cat={second_cat}(${by_cat[second_cat]} vs top ${by_cat[cat_rank[0]]}) "
      f"groc_delta={groceries_delta} q1={q1_2024} dining_over_cap={dining_over_cap}")
print(f"  b2: n_acts={n_activities} avg_session={avg_session_min} total_dist={total_distance} "
      f"longest_wk={longest_workout_min} n_runs={n_runs}")
print(f"  b2: fin_pages={finished_pages} n_genres={n_genres} n_rated5={n_rated5} "
      f"top_nat={top_nat_pages}({pages_by_nat})")
print(f"  b2: avg_nights={avg_nights} longest_trip={longest_trip_place} n_entries={n_journal_entries}")
print(f"  b2: screen_over150={screen_days_over_150} 2nd_app={screen_second_app}"
      f"({screen_by_app[screen_second_app]} vs top {screen_by_app[sorted(screen_by_app, key=screen_by_app.get, reverse=True)[0]]}) "
      f"sleep_ge8={sleep_at_least_8}")
print(f"  b2: n_countries={n_contact_countries} n_close={n_close} subs_inactive={n_subs_inactive} "
      f"cheapest={cheapest_active!r} nights_visited_contacts={nights_visited_contacts}")
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

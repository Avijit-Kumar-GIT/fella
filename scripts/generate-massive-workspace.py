#!/usr/bin/env python3
"""Create a large, independent, human-shaped Fella test workspace.

This fixture is intentionally not derived from Fella's source code or its
internal field names. It is a small personal archive: useful files are mixed
with stale exports, prose, notes, malformed cells, and unrelated material.
The seed keeps reruns reproducible while leaving the generated folder free of
answers, gold labels, or harness instructions.
"""

import csv
import json
import random
import sys
from datetime import date, timedelta
from pathlib import Path


SEED = 20260928
START = date(2024, 1, 1)
RNG = random.Random(SEED)


def day(offset: int) -> date:
    return START + timedelta(days=offset)


def money(value: float, messy: bool = False) -> str:
    if not messy:
        return f"${value:,.2f}"
    style = RNG.randrange(7)
    if style == 0:
        return f"{value:.2f}"
    if style == 1:
        return f"${value:,.2f}"
    if style == 2:
        return f"({abs(value):,.2f})" if value < 0 else f"{value:,.2f}"
    if style == 3:
        return f"USD {value:,.2f}"
    if style == 4:
        return f" ${value:,.2f} "
    return f"{value:.2f}"


def write_csv(root: Path, relative: str, headers, rows, delimiter=",") -> None:
    path = root / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.writer(handle, delimiter=delimiter)
        writer.writerow(headers)
        writer.writerows(rows)


def write_json(root: Path, relative: str, value) -> None:
    path = root / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


def write_jsonl(root: Path, relative: str, rows) -> None:
    path = root / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("".join(json.dumps(row) + "\n" for row in rows), encoding="utf-8")


def write_text(root: Path, relative: str, text: str) -> None:
    path = root / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text.rstrip() + "\n", encoding="utf-8")


def finance(root: Path) -> None:
    areas = {
        "home": [("landlord transfer", 1450, 1550), ("water bill", 35, 90), ("hardware shop", 20, 240)],
        "food": [("corner market", 25, 180), ("coffee and bagel", 5, 18), ("takeaway", 18, 75)],
        "transport": [("metro reload", 20, 90), ("train ticket", 35, 180), ("fuel station", 35, 80)],
        "leisure": [("cinema", 18, 55), ("bookshop", 12, 90), ("concert hall", 35, 160)],
        "health": [("pharmacy", 8, 120), ("clinic copay", 20, 80)],
    }
    rows = []
    for offset in range(0, 1096):
        if RNG.random() > 0.74:
            continue
        when = day(offset)
        if RNG.random() < 0.035:
            rows.append([when.isoformat(), "Payroll", money(RNG.choice([2800, 3200, 3500])), "income", "checking", "cleared", "regular"])
            continue
        area = RNG.choice(list(areas))
        description, low, high = RNG.choice(areas[area])
        amount = round(RNG.uniform(low, high), 2)
        if description == "landlord transfer":
            amount = round(RNG.uniform(1450, 1600), 2)
        if RNG.random() < 0.018:
            value = "N/A"
        elif RNG.random() < 0.012:
            value = money(-amount, messy=True)
        else:
            value = money(amount, messy=True)
        rows.append([
            when.strftime("%Y/%m/%d") if RNG.random() < 0.08 else when.isoformat(),
            description.title() if RNG.random() < 0.3 else description,
            value,
            area.title() if RNG.random() < 0.2 else area,
            RNG.choice(["checking", "visa", "joint account"]),
            RNG.choice(["cleared", "cleared", "pending"]),
            "manual note" if RNG.random() < 0.025 else "",
        ])
    rows += [["", "Account total (do not treat as a transaction)", money(0), "", "", "", ""]]
    write_csv(root, "finance/transactions_current.csv", ["posted", "description", "value", "area", "account", "status", "memo"], rows)

    card_rows = []
    merchants = ["Northwind Market", "Metro", "Cloud storage", "Corner Cafe", "Harbor Books", "Green Pharmacy", "Studio 14"]
    for offset in range(0, 760):
        if RNG.random() < 0.13:
            continue
        when = day(offset + 150)
        amount = round(RNG.uniform(4, 220), 2)
        card_rows.append([
            when.strftime("%d-%b-%Y"),
            RNG.choice(merchants),
            money(amount, messy=True) if RNG.random() > 0.03 else "--",
            RNG.choice(["personal", "shared", "work?", ""]),
            RNG.choice(["posted", "posted", "pending"]),
        ])
    write_csv(root, "finance/card_activity.tsv", ["date", "merchant", "amount_usd", "tag", "state"], card_rows, "\t")

    write_csv(root, "finance/monthly_plan.csv", ["area", "monthly_limit", "reviewed"], [
        ["home", "1550", "2025-01-04"], ["food", "500", "2025-01-04"],
        ["transport", "260", "2025-01-04"], ["leisure", "300", "2025-01-04"],
        ["health", "180", "2025-01-04"],
    ])
    write_csv(root, "finance/refunds.csv", ["received", "merchant", "amount", "reason"], [
        ["2024-03-14", "Studio 14", money(48.00), "duplicate charge"],
        ["2024-08-22", "Harbor Books", money(19.95), "returned item"],
        ["2025-01-08", "Northwind Market", money(11.20), "damaged goods"],
    ])
    write_text(root, "finance/money-notes.md", """
# Money notes

The landlord amount changed a little in July. The old card export is not a
second account; it is an archive from before the account was renamed.

Some grocery purchases appear as `Market`, `market`, or `Northwind`. I usually
keep refunds as negative values when I reconcile a month, but I do not always
remember to mark them.
""")

    old = [[f"2022-{month:02d}-03", "old landlord", money(1200), "home", "closed", "cleared"] for month in range(1, 13)]
    write_csv(root, "archive/transactions_2022.csv", ["date", "description", "value", "area", "account", "status"], old)
    write_csv(root, "archive/monthly_plan_old.csv", ["area", "limit"], [["home", "1100"], ["food", "350"], ["transport", "180"]])


def health(root: Path) -> None:
    sleep = []
    for offset in range(0, 1096):
        if RNG.random() < 0.04:
            hours = None
        else:
            hours = round(RNG.uniform(5.1, 8.8), 1)
        sleep.append({
            "when": day(offset).strftime("%m/%d/%Y") if offset % 61 == 0 else day(offset).isoformat(),
            "rest_hours": hours if hours is not None else RNG.choice(["", "n/a", None]),
            "quality_score": RNG.choice([1, 2, 3, 3, 4, 5]),
            "note": RNG.choice(["woke once", "late screen time", "", "travel", "felt rested"]),
        })
    write_jsonl(root, "health/nightly-rest.jsonl", sleep)

    activities = []
    kinds = [("run", 4, 12), ("walk", 2, 8), ("cycle", 8, 40), ("yoga", 0, 0), ("swim", 0.5, 3)]
    for offset in range(0, 1000):
        if RNG.random() > 0.64:
            continue
        kind, low, high = RNG.choice(kinds)
        distance = round(RNG.uniform(low, high), 1) if high else 0
        activities.append([
            day(offset).isoformat(), kind, RNG.randint(20, 95),
            f"{distance} km" if RNG.random() < 0.18 else distance,
            RNG.choice(["outdoors", "gym", "home", "lunch break"]),
        ])
    write_csv(root, "health/activity_log.csv", ["date", "activity", "minutes", "distance", "setting"], activities)
    write_csv(root, "health/body-measurements.csv", ["recorded", "body mass", "unit", "comment"], [
        [day(i * 21).isoformat(), round(RNG.uniform(68, 76), 1), "kg", "morning"] for i in range(52)
    ])
    write_text(root, "health/doctor-note.md", """
## Notes from annual check-in

The watch occasionally records a blank night when it was charging. A quality
score is subjective and is not a duration. Activity distances are exported in
kilometres unless the row says otherwise.
""")

    for month in range(1, 13):
        rows = [[f"2025-{month:02d}-{day_num:02d}", RNG.randint(40, 140), RNG.choice(["good", "okay", "rough"])] for day_num in range(1, 25)]
        write_csv(root, f"health/device_export_{month:02d}.csv", ["day", "active minutes", "label"], rows)


def reading(root: Path) -> None:
    genres = ["literary", "science fiction", "history", "memoir", "essays", "mystery"]
    rows = []
    for index in range(1, 181):
        status = RNG.choice(["finished", "finished", "in progress", "abandoned"])
        rows.append([
            f"Book {index:03d}",
            RNG.choice(["A. Rivera", "M. Chen", "T. Okafor", "J. Patel", "S. Nguyen"]),
            RNG.choice(genres),
            RNG.randint(90, 620),
            RNG.choice(["", "", "3", "4", "4", "5"]) if status != "finished" else RNG.choice(["3", "4", "4", "5"]),
            status,
            day(RNG.randint(0, 1095)).isoformat() if status == "finished" else "",
        ])
    write_csv(root, "reading/reading_log.csv", ["title", "author", "shelf", "pages", "rating", "status", "finished_on"], rows)
    highlights = [{"title": f"Book {i:03d}", "quote": RNG.choice(["a quiet argument about time", "the good part was the ending", "I underlined this twice"]), "page": RNG.randint(2, 400)} for i in range(1, 81)]
    write_json(root, "reading/highlights.json", highlights)
    write_text(root, "reading/reading-notes.md", """
I tend to call the shelf `literary` when I mean novels that are not genre
fiction. The ratings are out of five, but a blank rating means I never scored
the book. The shelves are personal labels, not a publishing taxonomy.
""")


def travel(root: Path) -> None:
    countries = [("Portugal", "Lisbon"), ("Japan", "Kyoto"), ("Germany", "Berlin"), ("Mexico", "Oaxaca"), ("Canada", "Toronto"), ("Iceland", "Reykjavik"), ("USA", "Austin")]
    trips = []
    for index in range(1, 121):
        country, city = RNG.choice(countries)
        start = day(RNG.randint(0, 1095))
        nights = RNG.randint(1, 10)
        trips.append([f"T-{index:04d}", start.isoformat(), (start + timedelta(days=nights)).isoformat(), city, country, nights, RNG.choice(["holiday", "work", "family", "holiday"])])
    write_csv(root, "travel/trip_history.csv", ["trip_id", "departed", "returned", "city", "country", "nights", "reason"], trips)
    expenses = []
    for index in range(1, 301):
        expenses.append([f"T-{RNG.randint(1, 120):04d}", day(RNG.randint(0, 1095)).isoformat(), RNG.choice(["lodging", "flight", "food", "museum", "rail"]), money(RNG.uniform(15, 900), messy=True)])
    write_csv(root, "travel/trip_expenses.csv", ["trip_id", "date", "kind", "amount"], expenses)
    write_text(root, "travel/travel-notes.txt", """
The short Berlin visit was work. Portugal, Japan, Mexico, Iceland, and the
longer Toronto visit were personal travel. One itinerary was changed after
the receipt was issued, so the dates in the note and the export do not always
match.
""")


def work(root: Path) -> None:
    clients = ["Northwind", "Acme", "Mosaic", "Juniper", "Orchard", "Bluebird"]
    projects = ["redesign", "migration", "research", "support", "internal"]
    invoices = []
    for index in range(1, 601):
        issued = day(RNG.randint(0, 1095))
        due = issued + timedelta(days=RNG.choice([14, 30, 45]))
        paid = due + timedelta(days=RNG.randint(-8, 35)) if RNG.random() > 0.19 else ""
        invoices.append([f"INV-{index:05d}", RNG.choice(clients), RNG.choice(projects), issued.isoformat(), due.isoformat(), paid, money(RNG.uniform(250, 8500), messy=True), RNG.choice(["paid", "paid", "open", "late"])])
    write_csv(root, "work/invoices.csv", ["invoice", "client", "project", "issued", "due", "paid", "amount", "state"], invoices)
    time_rows = []
    for index in range(1, 1001):
        time_rows.append([day(RNG.randint(0, 1095)).isoformat(), RNG.choice(projects), RNG.choice(["analysis", "meeting", "build", "admin"]), RNG.randint(15, 240), RNG.choice(["billable", "billable", "nonbillable"])])
    write_csv(root, "work/time-log.tsv", ["date", "project", "kind", "minutes", "billing"], time_rows, "\t")
    write_json(root, "work/clients.json", [{"name": c, "relationship": RNG.choice(["active", "paused", "new"]), "owner": "me"} for c in clients])
    write_text(root, "work/project-notes.md", """
## Project notebook

`support` contains a lot of short entries and should not be mistaken for a
client invoice. A late invoice can still be paid; the state column reflects
the bookkeeping pass, not the date of the bank settlement.
""")


def home_and_media(root: Path) -> None:
    utilities = []
    for offset in range(0, 1096, 29):
        utilities.append([day(offset).isoformat(), RNG.choice(["electric", "water", "internet", "gas"]), money(RNG.uniform(28, 210), messy=True), RNG.randint(1, 900)])
    write_csv(root, "home/utility_readings.csv", ["read_on", "service", "bill", "meter"], utilities)
    maintenance = [[day(RNG.randint(0, 1095)).isoformat(), RNG.choice(["boiler", "bike", "laptop", "sink", "window"]), money(RNG.uniform(20, 1200), messy=True), RNG.choice(["done", "planned", "done"])] for _ in range(120)]
    write_csv(root, "home/maintenance.csv", ["date", "item", "cost", "status"], maintenance)
    write_text(root, "home/appliance-notes.txt", """
The meter readings are not dollars. The internet bill was moved from one
provider to another in the middle of the year. Maintenance costs include a
few cash purchases that never appeared in the bank export.
""")

    screen = []
    apps = ["browser", "messages", "music", "maps", "news", "podcasts"]
    for offset in range(0, 730):
        for app in apps:
            screen.append([day(offset).isoformat(), app, RNG.randint(2, 75) if RNG.random() > 0.015 else ""])
    write_csv(root, "media/screen-time.tsv", ["date", "app", "minutes"], screen, "\t")
    listens = [{"date": day(RNG.randint(0, 1095)).isoformat(), "artist": RNG.choice(["Nia", "The Static", "M. Ward", "Open Field"]), "minutes": RNG.randint(3, 180), "kind": RNG.choice(["album", "podcast", "album"])} for _ in range(650)]
    write_jsonl(root, "media/listening.jsonl", listens)
    write_csv(root, "media/watchlist.csv", ["title", "kind", "watched", "rating"], [[f"Screen {i:03d}", RNG.choice(["film", "series"]), RNG.choice(["yes", "no"]), RNG.choice(["", "3", "4", "5"])] for i in range(1, 140)])


def notes_and_distractors(root: Path) -> None:
    moods = ["fine", "foggy", "restless", "focused", "overwhelmed", "good"]
    for index in range(1, 61):
        when = day(index * 5)
        text = f"""# {when.strftime('%A')} — {when.isoformat()}

Mood: {RNG.choice(moods)}. I worked on {RNG.choice(['redesign', 'migration', 'support'])},
then took a {RNG.randint(20, 70)} minute walk. Remember to check the {RNG.choice(['water bill', 'book return', 'old invoice'])}.

This is a personal note, not a ledger. The number {RNG.randint(1, 99)} was a
rough estimate and should not be added to a financial total.
"""
        write_text(root, f"notes/daily_{index:03d}.md", text)
    for index in range(1, 21):
        write_text(root, f"receipts/receipt_{index:03d}.txt", f"Receipt {index:03d}\nStore: {RNG.choice(['Corner Market', 'Harbor Books', 'Studio 14', 'Green Pharmacy'])}\nTotal: {money(RNG.uniform(4, 240))}\nPaid by card.\n")
    write_csv(root, "misc/weather.csv", ["day", "temperature_c", "rain_mm", "remark"], [[day(i).isoformat(), RNG.randint(-5, 35), RNG.randint(0, 28), RNG.choice(["clear", "rain", "windy"])] for i in range(0, 500, 2)])
    write_csv(root, "misc/houseplants.csv", ["checked", "plant", "water_ml", "condition"], [[day(i * 7).isoformat(), RNG.choice(["fern", "pothos", "cactus", "herb"]), RNG.randint(20, 500), RNG.choice(["good", "dry", "good"])] for i in range(80)])
    write_text(root, "misc/random_ideas.md", """
Things to maybe do: learn more about fermentation, repair the lamp, ask Sam
about the conference, and find the receipt that was in the blue notebook.
""")


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: generate-massive-workspace.py OUTPUT_DIRECTORY")
    root = Path(sys.argv[1]).expanduser().resolve()
    if root.exists() and any(root.iterdir()):
        raise SystemExit(f"refusing to overwrite non-empty directory: {root}")
    root.mkdir(parents=True, exist_ok=True)
    finance(root)
    health(root)
    reading(root)
    travel(root)
    work(root)
    home_and_media(root)
    notes_and_distractors(root)
    files = [p for p in root.rglob("*") if p.is_file()]
    print(f"created {len(files)} files ({sum(p.stat().st_size for p in files):,} bytes) at {root}")


if __name__ == "__main__":
    main()

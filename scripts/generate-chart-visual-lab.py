#!/usr/bin/env python3
"""Generate a deterministic, multi-thousand-row chart-review workspace.

Usage:
  python3 scripts/generate-chart-visual-lab.py WORKSPACE_DIR GUIDE.md

The answer guide and expected chart points are written beside (not inside) the
mountable workspace, so mounting the data never gives Fella the answer key.
Existing output paths are never overwritten.
"""

from __future__ import annotations

import csv
import math
import sys
import tempfile
from collections import defaultdict
from datetime import date, timedelta
from decimal import Decimal
from pathlib import Path


START = date(2024, 1, 1)
MONTHS = 24
ORDERS_PER_MONTH = 60
FEEDBACK_ROWS = 1200
CAMPAIGNS_PER_MONTH = 10
REGIONS = ["North", "South", "East", "West", "Central"]
CATEGORIES = [
    ("Hardware", 29_900),
    ("Software", 12_900),
    ("Services", 78_000),
    ("Accessories", 8_900),
    ("Training", 24_500),
    ("Support", 18_500),
]
CHANNELS = ["Direct", "Search", "Partner", "Retail"]
PLAN_FACTORS_BP = [10_400, 9_700, 10_200, 9_600, 10_300, 9_900]
WEEKDAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]


def month_at(index: int) -> tuple[int, int]:
    absolute = START.year * 12 + START.month - 1 + index
    return absolute // 12, absolute % 12 + 1


def month_key(index: int) -> str:
    year, month = month_at(index)
    return f"{year}-{month:02d}"


def dollars(cents: int) -> str:
    sign = "-" if cents < 0 else ""
    whole, remainder = divmod(abs(cents), 100)
    return f"{sign}{whole}.{remainder:02d}"


def write_csv(path: Path, headers: list[str], rows: list[list[object]]) -> None:
    with path.open("w", newline="", encoding="utf-8-sig") as handle:
        writer = csv.writer(handle)
        writer.writerow(headers)
        writer.writerows(rows)


def amount_from_csv(value: str) -> int:
    return int(Decimal(value) * 100)


def pearson(xs: list[float], ys: list[float]) -> float:
    x_mean = sum(xs) / len(xs)
    y_mean = sum(ys) / len(ys)
    numerator = sum((x - x_mean) * (y - y_mean) for x, y in zip(xs, ys))
    x_var = sum((x - x_mean) ** 2 for x in xs)
    y_var = sum((y - y_mean) ** 2 for y in ys)
    return numerator / math.sqrt(x_var * y_var)


def make_workspace(root: Path) -> dict[str, object]:
    monthly_actual: dict[str, int] = defaultdict(int)
    monthly_channel: dict[tuple[str, str], int] = defaultdict(int)
    orders: list[list[object]] = []

    for month_index in range(MONTHS):
        year, month = month_at(month_index)
        for row_index in range(ORDERS_PER_MONTH):
            category_index = (row_index + month_index * 2) % len(CATEGORIES)
            category, base_cents = CATEGORIES[category_index]
            region_index = (row_index * 3 + month_index) % len(REGIONS)
            region = REGIONS[region_index]
            channel = CHANNELS[(row_index + region_index + month_index) % len(CHANNELS)]
            units = 1 + ((row_index * 7 + month_index * 3 + region_index) % 8)
            seasonal_bp = (
                9_500
                + (month_index % 12) * 75
                + (month_index // 12) * 200
                + ((row_index * 17) % 11) * 20
            )
            gross_cents = (base_cents * units * seasonal_bp + 5_000) // 10_000
            is_refund = (row_index + month_index * 13) % 47 == 0
            net_cents = -((gross_cents * 60 + 50) // 100) if is_refund else gross_cents
            day = 1 + (row_index * 5 + month_index * 3) % 28
            order_date = date(year, month, day).isoformat()
            order_id = f"ORD-{year}{month:02d}-{row_index + 1:03d}"
            orders.append([
                order_id,
                order_date,
                region,
                category,
                channel,
                units,
                dollars(net_cents),
                "refund" if is_refund else "sale",
            ])
            monthly_actual[month_key(month_index)] += net_cents
            monthly_channel[(month_key(month_index), channel)] += net_cents

    write_csv(
        root / "transactions.csv",
        ["order_id", "order_date", "region", "product_category", "sales_channel", "units", "net_revenue", "entry_type"],
        orders,
    )

    plans: dict[str, int] = {}
    plan_rows: list[list[object]] = []
    for month_index in range(MONTHS):
        key = month_key(month_index)
        factor = PLAN_FACTORS_BP[month_index % len(PLAN_FACTORS_BP)]
        plan = (monthly_actual[key] * factor + 5_000) // 10_000
        plans[key] = plan
        plan_rows.append([key, dollars(plan), "approved", "Monthly net-revenue target; USD"])
    write_csv(
        root / "monthly_plan.csv",
        ["month", "planned_net_revenue", "plan_status", "note"],
        plan_rows,
    )

    feedback_rows: list[list[object]] = []
    valid_ratings: list[int] = []
    feedback_comments = [
        "quick and easy",
        "had to try twice",
        "good follow-up",
        "waited longer than expected",
        "all sorted now",
        "no comment",
    ]
    for index in range(FEEDBACK_ROWS):
        month_index = index % MONTHS
        year, month = month_at(month_index)
        day = 1 + (index * 11 + month_index) % 28
        region = REGIONS[(index * 3 + month_index) % len(REGIONS)]
        channel = CHANNELS[(index * 2 + month_index) % len(CHANNELS)]
        rating = "" if index % 53 == 0 else str(1 + (index * 11 + month_index * 7) % 5)
        if rating:
            valid_ratings.append(int(rating))
        response_minutes = 8 + ((index * 37 + month_index * 19) % 208)
        feedback_rows.append([
            f"FB-{index + 1:05d}",
            date(year, month, day).isoformat(),
            region,
            channel,
            rating,
            response_minutes,
            feedback_comments[(index * 5 + month_index) % len(feedback_comments)],
        ])
    write_csv(
        root / "customer_feedback.csv",
        ["response_id", "response_date", "region", "sales_channel", "rating_1_to_5", "response_minutes", "comment"],
        feedback_rows,
    )

    campaigns: list[list[object]] = []
    campaign_spend: list[float] = []
    campaign_customers: list[float] = []
    for month_index in range(MONTHS):
        for campaign_index in range(CAMPAIGNS_PER_MONTH):
            channel = CHANNELS[(campaign_index + month_index) % len(CHANNELS)]
            spend_cents = (
                3_000
                + campaign_index * 1_200
                + month_index * 55
                + ((campaign_index * 37 + month_index * 19) % 1_000)
            )
            customers = 2 + spend_cents // 500 + ((campaign_index * 7 + month_index * 3) % 9)
            clicks = customers * 15 + 10 + ((campaign_index * 13 + month_index) % 50)
            campaign_id = f"CMP-{month_key(month_index)}-{campaign_index + 1:02d}"
            campaigns.append([campaign_id, month_key(month_index), channel, dollars(spend_cents), customers, clicks])
            campaign_spend.append(spend_cents / 100)
            campaign_customers.append(float(customers))
    write_csv(
        root / "campaign_performance.csv",
        ["campaign_id", "month", "sales_channel", "ad_spend", "new_customers", "clicks"],
        campaigns,
    )

    traffic_rows: list[list[object]] = []
    weekday_channel_visits: dict[tuple[str, str], int] = defaultdict(int)
    daily_channel_visits: dict[tuple[str, str], int] = defaultdict(int)
    traffic_base = [1_100, 1_750, 430, 650]
    conversion_bp = [320, 450, 610, 290]
    traffic_start = date(2024, 1, 1)
    traffic_days = (date(2025, 12, 31) - traffic_start).days + 1
    for day_index in range(traffic_days):
        current = traffic_start + timedelta(days=day_index)
        weekday = WEEKDAYS[current.weekday()]
        for channel_index, channel in enumerate(CHANNELS):
            weekly = [
                -120,
                -90,
                -60,
                20,
                90,
                210,
                170,
            ][current.weekday()]
            seasonal = int(120 * math.sin((day_index / 365.25) * 2 * math.pi + channel_index * 0.55))
            noise = ((day_index * 37 + channel_index * 71) % 181) - 90
            visits = max(
                80,
                traffic_base[channel_index]
                + weekly
                + seasonal
                + int(day_index * (channel_index + 1) * 0.12)
                + noise,
            )
            signups = max(
                0,
                (visits * (conversion_bp[channel_index] + ((day_index + channel_index * 7) % 35)) + 5_000) // 10_000,
            )
            traffic_rows.append([current.isoformat(), channel, visits, signups])
            weekday_channel_visits[(weekday, channel)] += visits
            daily_channel_visits[(current.isoformat(), channel)] += visits
    write_csv(
        root / "daily_traffic.csv",
        ["date", "sales_channel", "visits", "signups"],
        traffic_rows,
    )

    (root / "README.md").write_text(
        "# Market archive (synthetic test data)\n\n"
        "This folder contains generated, fictional records for exercising file-based analytics.\n\n"
        "- `transactions.csv`: one sale or refund per row. `net_revenue` is USD after discounts; refunds are negative. `order_id` is unique.\n"
        "- `monthly_plan.csv`: one approved net-revenue target per calendar month. It is already monthly-grain data; do not sum it after joining it to transaction rows.\n"
        "- `customer_feedback.csv`: one survey response per row. Blank ratings are missing observations, not zero. Response time is in minutes.\n"
        "- `campaign_performance.csv`: one campaign per row, with spend, acquired customers, and clicks.\n"
        "- `daily_traffic.csv`: one date/channel observation per row, spanning two full calendar years. Visits and signups are counts.\n\n"
        "Dates use ISO format. Monetary fields are numeric USD values with two decimal places.\n",
        encoding="utf-8",
    )

    # Read the written files back before writing the separate expected values.
    with (root / "transactions.csv").open(newline="", encoding="utf-8-sig") as handle:
        read_actual: dict[str, int] = defaultdict(int)
        read_categories: dict[str, int] = defaultdict(int)
        read_regions: dict[str, int] = defaultdict(int)
        for row in csv.DictReader(handle):
            amount = amount_from_csv(row["net_revenue"])
            key = row["order_date"][:7]
            read_actual[key] += amount
            read_categories[row["product_category"]] += amount
            read_regions[row["region"]] += amount
    if dict(read_actual) != dict(monthly_actual):
        raise RuntimeError("written transaction month totals do not match generated rows")

    expected_rows: list[list[object]] = []
    for category, _ in CATEGORIES:
        expected_rows.append(["category_net_revenue", "net_revenue", category, dollars(read_categories[category]), "", ""])
    for key in sorted(read_actual):
        expected_rows.append(["monthly_actual_vs_plan", "Actual net revenue", key, dollars(read_actual[key]), "", ""])
        expected_rows.append(["monthly_actual_vs_plan", "Plan", key, dollars(plans[key]), "", ""])
    for region in REGIONS:
        expected_rows.append(["region_net_revenue", "net_revenue", region, dollars(read_regions[region]), "", ""])
    for month_index in range(MONTHS):
        key = month_key(month_index)
        for channel in CHANNELS:
            expected_rows.append([
                "monthly_channel_mix",
                channel,
                key,
                dollars(monthly_channel[(key, channel)]),
                "",
                "",
            ])
    for weekday in WEEKDAYS:
        for channel in CHANNELS:
            expected_rows.append([
                "weekday_channel_visits",
                channel,
                weekday,
                str(weekday_channel_visits[(weekday, channel)]),
                "",
                "",
            ])
    for (day, channel), visits in daily_channel_visits.items():
        expected_rows.append(["daily_channel_visits", channel, day, str(visits), "", ""])
    for row in campaigns:
        expected_rows.append([
            "campaign_scatter",
            str(row[2]),
            str(row[0]),
            "",
            str(row[3]),
            str(row[4]),
        ])

    total_net = sum(read_actual.values())
    expected_rows.append(["net_revenue_total", "net_revenue", "all transactions", dollars(total_net), "", ""])
    expected_path = root.parent / f"{root.name}-EXPECTED.csv"
    guide = [
        "# Fella Chart Visual Lab — manual test guide",
        "",
        "All records are fictional, deterministic, and generated for UI/chart review.",
        "Mount only the `fella-chart-visual-lab` folder. Keep this guide and the adjacent `-EXPECTED.csv` file outside the mounted folder so the answer key is not part of workspace context.",
        "Use a fresh conversation per prompt. After each chart, open its exact-values view and compare it against the matching check in the expected CSV. Treat wrong values, dropped groups, duplicate marks, omitted charts, or unreadable labels as failures; do not adjust the expected file to match Fella.",
        "",
        "## Dataset size",
        "",
        f"- {len(orders):,} transaction rows across 24 months, six product categories, five regions, and four channels.",
        f"- {len(plan_rows):,} monthly-plan rows.",
        f"- {len(feedback_rows):,} survey rows ({FEEDBACK_ROWS - len(valid_ratings):,} missing ratings; response times are populated).",
        f"- {len(campaigns):,} campaign rows; spend/customer correlation in this synthetic set is {pearson(campaign_spend, campaign_customers):.3f}.",
        f"- {len(traffic_rows):,} daily channel observations across two complete years (731 dates × four channels).",
        "",
        "## Questions",
        "",
        "1. `Across the full period, which product category generated the most net revenue? Show an appropriate chart.` Expected: six-category bar chart; compare `category_net_revenue`.",
        "2. `How did net revenue change month by month? Plot the full period as a line and identify the highest and lowest months.` Expected: 24 chronologically ordered points; compare Actual net revenue rows in `monthly_actual_vs_plan`.",
        "3. `Compare actual net revenue with plan month by month. Plot both series and tell me which months missed plan.` Expected: two aligned 24-point series; compare both series in `monthly_actual_vs_plan`.",
        "4. `Which region contributed the most net revenue overall? Show the regional comparison.` Expected: five regions; compare `region_net_revenue`.",
        "5. `Show the net-revenue mix by product category as a donut, using the full net revenue total as the whole.` Expected: six slices; total is in `net_revenue_total`.",
        "6. `Show how the revenue mix across sales channels changed month to month as a stacked area chart.` Expected: four series × 24 months; compare `monthly_channel_mix`.",
        "7. `Does campaign spend tend to move with new customers? Plot one point per campaign.` Expected: 240 scatter points, positive association; compare x/y in `campaign_scatter`.",
        "8. `Show the distribution of customer response times as a histogram with eight equal-width bins.` Expected: 1,200 observations; check that the chart accounts for all rows and uses minutes.",
        "9. `Make a heatmap of visits by weekday and sales channel across the full period.` Expected: a 7 × 4 grid; compare `weekday_channel_visits`.",
        "10. `Compare the spread of response times across regions with a box plot.` Expected: five groups; check medians, quartiles, whiskers, and units against `customer_feedback.csv`.",
        "11. `Show daily visits by channel over the entire archive.` Expected: four series × 731 ordered dates; compare `daily_channel_visits`. Check tick thinning, line separation, and responsiveness.",
        "12. `What was total net revenue? A sentence is enough; no chart.` Expected: one supported total and no chart; compare `net_revenue_total`.",
        "",
        "## Review",
        "",
        "Try light and dark mode, normal and narrow window widths. Inspect title, units, labels, legend, exact values, missing-data behavior, clipping, and whether the takeaway matches the chart. The two-year daily chart is intentionally dense; its axis labels should thin out while all data points remain in the exact-value table.",
        "",
        f"Expected values file: `{expected_path.name}` (outside the mounted folder). Synthetic seed: fixed by this generator. Total net revenue: ${dollars(total_net)}.",
    ]

    return {
        "guide": guide,
        "expected_rows": expected_rows,
        "data_rows": len(orders) + len(feedback_rows) + len(campaigns) + len(traffic_rows) + len(plan_rows),
        "workspace_files": len(list(root.iterdir())),
    }


def main() -> None:
    if len(sys.argv) != 3:
        raise SystemExit("usage: generate-chart-visual-lab.py WORKSPACE_DIRECTORY GUIDE.md")
    root = Path(sys.argv[1]).expanduser().resolve()
    guide_path = Path(sys.argv[2]).expanduser().resolve()
    expected_path = root.parent / f"{root.name}-EXPECTED.csv"
    if root.exists() or guide_path.exists() or expected_path.exists():
        raise SystemExit("refusing to overwrite an existing output path")
    if root.parent != guide_path.parent:
        raise SystemExit("workspace and separate guide must be siblings")
    root.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".fella-chart-lab-build-", dir=root.parent) as temporary:
        stage = Path(temporary)
        staged_root = stage / root.name
        staged_root.mkdir()
        summary = make_workspace(staged_root)
        staged_guide = stage / guide_path.name
        staged_guide.write_text("\n".join(summary["guide"]) + "\n", encoding="utf-8")
        staged_expected = stage / expected_path.name
        write_csv(
            staged_expected,
            ["check", "series", "label", "value", "x", "y"],
            summary["expected_rows"],
        )
        if root.exists() or guide_path.exists() or expected_path.exists():
            raise SystemExit("refusing to overwrite an existing output path")
        staged_root.rename(root)
        staged_guide.rename(guide_path)
        staged_expected.rename(expected_path)

    print(f"Created {summary['data_rows']:,} data rows in {summary['workspace_files']} files at {root}")
    print(f"Guide: {guide_path}")
    print(f"Expected chart points: {expected_path}")


if __name__ == "__main__":
    main()

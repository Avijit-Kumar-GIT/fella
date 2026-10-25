# Fella desktop journeys: filesystem analytics

This is a manual journey guide for checking that the app can accept a sequence
of questions, complete turns, expose Analysis Details, render a chart without
clipping, retain conversation context, and carry a clarification through to a
resumed turn. It does **not** grade whether answers or calculations are
correct. Answer correctness belongs to FQA-Bench.

The questions are drawn unchanged from the FQA-Bench v0.1 task files. They
provide repeatable manual scenarios; this guide does not include or invoke an
automated journey runner.

## Before you start

- Build the Electron app before starting the manual journey.
- For manual visual testing, choose OpenAI / `gpt-5.6-luna` and mount
  `bench/fqa-bench/suites/uci-bike-sharing/workspaces/capital-bikeshare`.
- Ask the Bike Sharing questions below in one conversation, in order. The
  follow-up intentionally relies on the prior question in that same thread.
- Mount `bench/fqa-bench/suites/clarification-housing/workspaces/housing` in a
  second conversation for the clarification journey.
- Use the actual Electron window, Rust sidecar, provider, and mounted
  synthetic/public fixtures. No real personal files are included.

## Journey A — one analyst session in Bike Sharing

Ask each exact question in the same conversation, one at a time:

1. **Inspect coverage:** “How many daily observations are there, and what
   dates does the file cover?”
2. **Annual total:** “How many rentals were recorded in 2012?”
3. **Grouped statistics:** “Compare average daily rentals on working and
   non-working days. Give each average and the difference.”
4. **Share / ratio:** “What percentage of all 2012 rentals were made by
   registered riders?”
5. **Cross-file reconciliation:** “Compare the daily records with the
   monthly summary derived from hourly records. What is the largest absolute
   difference between their totals for any matching year and month?”
6. **Data integrity:** “How many daily records have a total that does not
   equal casual rentals plus registered rentals?”
7. **Grouped maximum:** “Across both years combined, which calendar month
   had the most rentals, and how many?”
8. **Chart:** “Chart total rentals by month for 2012. Use month names on the
   horizontal axis and keep them in calendar order.”
9. **Follow-up setup:** “What was the total number of rentals in 2011?”
10. **Conversation continuity:** “And the following year—what was the total,
    and how much higher was it than 2011?”
11. **Forecast:** “For a backtest, pretend you are at the end of June 2011.
    Using only the monthly rental totals from January through June 2011,
    estimate July 2011 with their arithmetic mean. What forecast value would
    you record? Label it as a forecast, not an observed total.”
12. **What-if scenario:** “In a what-if scenario where rentals on every day
    of 2012 were 10% lower than recorded, what would the annual total be?
    State this as a scenario result, not an observed fact.”
13. **Missing measure:** “What was the average rental revenue per day in
    2012?”
14. **Missing data grain:** “At what hour of day were rentals highest in
    2012?”

During this journey, visually check that each prompt becomes a user turn, each
assistant turn finishes, and Analysis Details remains available. For the
cross-file question, inspect which sources are shown. For the chart, inspect
that one titled visualization appears, its exact-values disclosure opens, and
the plot/labels stay inside the answer column in both themes. For the
follow-up, keep the earlier turns in the same conversation. The final two
questions are useful for seeing how Fella communicates data limitations; this
guide does not judge their correctness.

## Journey B — clarify, then continue

Mount `bench/fqa-bench/suites/clarification-housing/workspaces/housing` in a
new conversation:

1. “What was my housing spending in Q1 2024?”
2. If Fella asks for the category scope, reply through the clarification card:
   “Count rent and utilities, but leave out repairs and maintenance.”

Visually check that the actual clarification question is displayed, the card
accepts the reply, the reply appears as a user turn, and Fella resumes in the
same conversation. Open Analysis Details on the resumed turn to inspect its
clarification lineage and source trail. The answer amount and interpretation
are deliberately not graded here.

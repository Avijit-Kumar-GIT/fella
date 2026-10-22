# OpenUI adaptive real-model acceptance — v1.1

Defined 2026-10-22 **before the first candidate execution**. This is a frozen
task-appropriateness contract, not an observed score or a chart-variety contest.
The test is `tests/e2e/openui-adaptive.live.spec.mjs`. Existing tests and
expectations are not changed. This document is not proof that any task passed.

Version 1.1 adds document-continuity and same-chat consecutive-answer checks
before this suite's first candidate execution. A1–A4 questions, numerical
oracles, presentation criteria, and failure grading remain unchanged. A separate
existing live Gemma test is not an execution of this new task set.

## Purpose and execution boundary

Exercise the real Electron desktop, Rust analytical engine, real provider,
same-turn OpenUI output, persisted artifact, and visible Svelte presentation.
Default provider/model: **ollama-cloud / gemma4:31b**. No mocked responses,
seeded analysis runs, presentation-only replacement requests, or test-authored
OpenUI programs are supplied. The only dialog override chooses the synthetic
workspace folder; analytical work and rendering remain real.

Each of the four cases starts a fresh app/profile. A1, A3, and A4 ask one
question. A2 first grades its original trend question, then asks the unchanged
A3 scalar question in the **same conversation**, expecting Q4 revenue of 1,410.
This is an additional task, not a retry or a substitute for independent A3.
There are no test-level retries or conversational retries. Product-internal
finalization/correction calls remain enabled and their trace is retained; a
passing final answer does not establish first-attempt model validity. A case
has a 420-second outer budget (780 seconds for the two-answer A2 journey), a
300-second analysis budget per answer, and a 30-second artifact/render budget.
Run one worker. Provider failures, missing credentials,
and setup failures are failures to report, not silently skipped tasks.

## Independent source data and numerical oracle

Every workspace contains only `sales.csv`. Columns:
`record_id,order_date,quarter,category,region,net_revenue,orders`.
`net_revenue` is already net revenue in USD, not a quantity to multiply by orders.
There are 18 rows: the six months July–December 2025 for each of three categories.
The literal data and expected values below are fixed independently of candidate
SQL, output wording, prompts, components, and generated answers.

| IDs | Category / region | Jul | Aug | Sep | Oct | Nov | Dec |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| R001–R006 | Apparel / North | 100 | 120 | 80 | 160 | 200 | 240 |
| R007–R012 | Footwear / South | 200 | 180 | 220 | 150 | 120 | 90 |
| R013–R018 | Equipment / West | 50 | 60 | 90 | 100 | 140 | 210 |

Orders, in the same row order:
`10,12,10,16,20,24; 20,18,22,15,12,9; 10,8,12,10,14,21`.
Dates are the first of each month. Jul–Sep is Q3; Oct–Dec is Q4.

| Category | Q3 USD | Q4 USD | Q4 − Q3 USD |
| --- | ---: | ---: | ---: |
| Apparel | 300 | 600 | +300 |
| Footwear | 600 | 360 | −240 |
| Equipment | 200 | 450 | +250 |

Rank by signed contribution: Apparel, Equipment, Footwear. Q3 total = 1,100;
Q4 total = 1,410; increase = 310; combined total = 2,510. Monthly totals:
Jul 350, Aug 360, Sep 390, Oct 410, Nov 460, Dec 540.

## Frozen questions and per-task criteria

### A1 — category changes and ranking

> Which categories drove the change in net revenue between Q3 and Q4 2025, and
> how do their contributions rank? Compare the category totals in sales.csv;
> net_revenue is USD.

The user does not request a chart. Require a visible, rendered OpenUI chart
whose bound results cover all three categories and whose field mappings express
their before/after revenue or signed changes. A table, chart source string,
hidden chart, empty canvas, or unrelated orders plot is insufficient. No chart
family, color, layout, or additional component is prescribed. Canonical accepted
results must retain the Q3/Q4 amounts for each category; the arithmetic oracle
is not derived from model-written explanatory prose. Supporting exact tables
are allowed.

### A2 — monthly trend

> How did total net revenue change month by month from July through December
> 2025 across all categories in sales.csv? net_revenue is USD.

Require a visible, rendered OpenUI chart of the six months and their revenue,
without the user asking to visualize. Canonical results must contain each
month's exact total. A chart with no temporal field, a single grand total, or
orders instead of revenue does not pass. Any meaningful chart family is allowed.

### A3 — one scalar

> What was the total net revenue in Q4 2025 across all categories in sales.csv?
> net_revenue is USD.

Require canonical accepted total **1,410**, plus a visible answer-specific bound
Metric or substantive bound explanatory text displaying that value with revenue,
total, or Q4 context. A heading and one-cell table alone do not pass. The metric
may use a direct saved-row reference or an expression over accepted saved rows;
bound text must contain a saved-result expression. Charts are not required.
There is no requirement to invent variety or avoid an appropriate scalar layout.

### A4 — exact records

> What are the record_id, order_date, category, region, net_revenue, and orders
> for records R002, R009, and R017 in sales.csv?

Require a visible, usable OpenUI table (or tables) with exactly those three
records and all six requested fields, backed by accepted canonical results:

| record_id | order_date | category | region | net_revenue | orders |
| --- | --- | --- | --- | ---: | ---: |
| R002 | 2025-08-01 | Apparel | North | 120 | 12 |
| R009 | 2025-09-01 | Footwear | South | 220 | 22 |
| R017 | 2025-11-01 | Equipment | West | 140 | 14 |

Table column order, labels, formatting, row order, and permitted additional
context are not fixed. A generic table heading without rendered records fails.

## Shared acceptance and evidence

- Before the first submit, set a unique marker on the actual browser Document
  object. After each chat `.analytical-presentation` is rendered, assert that
  exact marker remains in the current Document. A reload/window replacement
  loses it and fails; the test must not reload or re-add the marker to recover.
- After A2 passes its original trend criteria, submit A3 through the existing
  composer without changing workspace, chat, or page. Grade the second answer
  using the unchanged A3 criteria and 1,410 oracle. Require a distinct saved
  turn in the same canonical conversation and the same Document marker. The
  original A2 presentation node must remain connected, its original saved run
  bytes unchanged, and its chart still rendered with positive dimensions and
  graphical content. Attach both answers and per-answer latency/diagnostics.
- All cases must render the actual `.analytical-presentation` inside the new
  assistant answer. No legacy chart path, Markdown-only answer, validation
  fallback, visualization-error fallback, or hidden-only substitute counts.
- Inspect the persisted program with OpenUI's own parser/schema. Inspect actual
  visible DOM content, not just a `Chart(...)` or `Metric(...)` regex. Referenced
  Metrics, Charts, and Tables must bind existing accepted result identities.
- For generated nested Row/Grid/Columns/Tabs content, require at least one
  task-relevant leaf to render inside that generated layout. The task gate still
  requires its appropriate leaf; a rendered heading does not substitute for it.
  No fixed nested layout is injected or required of the model.
- Charts must contain a visible canvas or SVG with positive CSS and intrinsic
  dimensions and nonempty graphical output. Their mapped categorical/time and
  quantitative fields must match the relevant saved data. Scrolling the answer
  into view is allowed; switching a hidden presentation tab to rescue the primary
  chart is not. Font/icon SVGs outside the chart target never count.
- Numerical expected values must exist in accepted, complete canonical saved
  results, not just prose. Aliases and long/wide layouts are allowed. These tasks
  intentionally ask for totals; raw unaggregated samples do not meet that output
  contract. Percent-only output does not replace the requested USD totals.
- The same-turn program and result values must be present in the engine's saved
  run file and in a resolved persisted artifact after rendering. Reading them
  again must leave the canonical bytes unchanged. No test-authored result values
  are ever written into a canonical run.
- No uncaught renderer errors or OpenUI/chart-related console errors may occur.
  Other console/process messages are recorded separately, not hidden or treated
  as proof of a successful render.

Capture raw same-turn and normalized persisted OpenUI source, canonical run,
resolved artifact/results, parsed component/field observations, actual DOM/chart
geometry, full-page and answer screenshots, runtime errors, and per-stage elapsed
times. Capture diagnostics even if assertions fail. Failures preserve the unique
profile/workspace and evidence directory; remove the copied `auth.json` and its
temporary sibling before retaining it. Successful profiles are removed after
attachments are copied. Never print or attach credential contents.

The chart field/data gate is structural and numerical, not an aesthetic or
causal-quality oracle. It cannot establish that every visual mapping is the
best design. New display-only field aliases that cannot be connected to saved
fields may require grader adjudication; preserve the failed result instead of
rewriting expectations to fit it. One pass per task is a bounded smoke acceptance
sample, not an estimated model success rate or broad BI accuracy benchmark.

## Running (not executed while authoring)

Build the current desktop/sidecar first, then run only this new live suite:

```sh
pnpm electron:build
pnpm exec playwright test --config=playwright.live.config.mjs tests/e2e/openui-adaptive.live.spec.mjs --workers=1 --retries=0 --output=/tmp/fella-openui-adaptive-v1-run01
```

Linux needs a working display (`DISPLAY=:0` in the current environment); use
`xvfb-run -a` when a desktop display is unavailable. Credentials default to the
app's platform-specific `dev.fella.app/auth.json`. Only the selected provider's
credential is copied into the disposable profile.

Optional explicit overrides:

- `FELLA_OPENUI_LIVE_PROVIDER` (default `ollama-cloud`).
- `FELLA_OPENUI_LIVE_MODEL` (default `gemma4:31b`).
- `FELLA_E2E_AUTH_FILE` (original auth file path).
- `FELLA_ENGINE_PATH` (built native sidecar path).

An override is a different recorded provider/model configuration, not a
replacement Gemma result. Use Playwright `--grep` only for an explicitly
reported subset or materially changed debugging hypothesis. At most two runs
of an unchanged failing setup/hypothesis. Record failures, timeouts, exclusions,
provider settings, candidate revision, and any approved grader corrections.

## Results ledger

Run 01 used the first candidate on Linux Electron with Ollama Cloud
`gemma4:31b`, under the frozen v1.1 contract:

| Task | Run 01 | Run 02 after product corrections |
| --- | --- | --- |
| A1 category changes | Failed: metrics/table, no chart | Passed; visible contribution chart |
| A2 monthly trend | Failed: valid analysis/chart rejected because Row gap was 20 | Passed, including scalar follow-up in the same chat |
| A3 scalar | Passed | Passed |
| A4 exact records | Passed | Passed |

Run 01 completed in 1.2 minutes. Its failures remain recorded under
`/tmp/fella-openui-adaptive-v1-run01`, with credential-free profiles retained
for A1 and A2. Neither failure was excluded or regraded.

Before Run 02, presentation guidance was added to the main model instructions
from the same generated library, and oversized nonnegative Row gaps were
accepted and clamped to the existing spacing scale. The test parser also imports
Fella's existing pure-function registry to recognize the same supported names
as the renderer. Questions, numerical oracles and grading criteria were unchanged.

Run 02 passed all four cases in 37.3 seconds. Individual case durations were
16.8, 10.5 (trend plus follow-up), 4.6 and 4.7 seconds respectively. All five
answers rendered in the original browser Document; A2's original chart node
remained connected after its scalar follow-up. The tests required actual
graphical output, correct canonical results, and persisted artifacts. The
screenshots are under `/tmp/fella-openui-adaptive-v1-run02`.

Evidence-retention limitation: list-reporter runs retained screenshots, but
successful profiles were deleted and in-memory text attachments were not all
written as standalone files. The harness now writes redacted source/JSON
attachments into its output directory before cleanup for future runs. No extra
model run was performed solely to recreate those attachments. Failed profiles
from Run 01 still retain their redacted diagnostic files.

These results establish the bounded smoke tasks above, not a general success
rate, aesthetic score, or analytical benchmark. No assertions were changed to
turn either Run 01 failure into a pass.

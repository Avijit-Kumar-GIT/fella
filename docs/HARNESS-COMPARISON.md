# Harness benchmark — does the loop earn its place, and how tiny can it be?

Fella's philosophy: **something powerfully tiny that reads your files.** The
people who'll use it aren't comparing it to ChatGPT or Julius — they use it
because it's fun, fast, local, and cheap/open on principle. So the benchmark
isn't a bake-off against funded products. It answers two questions:

1. **Does the harness lift the model?** — `bare` (model + files in the prompt,
   no tools) vs `fella` (the full loop). `Δacc = acc(fella) − acc(bare)` is the
   number. If Δ→0 on a model, the loop isn't earning its place *for that model*.
2. **How far down the model ladder does the lift hold?** — run both conditions
   across a ladder of cheap/small models; find where the model, not the
   scaffold, becomes the bottleneck.

Scoped to the everyday-question tier Fella targets — numeric and categorical
lookups, free-text search, and small multi-file joins across a person's own
files — not multi-step data-science workflows.

## First-class metrics

Always shown, and what rolls up to a chart:

| metric | what it tells you |
|---|---|
| **acc** = correct / n (majority over `--iters`) | headline |
| **consistency** = fraction of cases correct on *all* k iters | right 3/5 ≠ right 5/5 |
| **Δacc vs bare** = acc(fella) − acc(bare), same cases | the harness's lift, per model — *the thesis number* |
| **Δacc 95% CI** (paired bootstrap over cases) | does the lift survive resampling — excludes 0 = real, straddles 0 = noise |
| **tok / correct** (in + out) | efficiency |
| **$ / 100-correct** | the money version (list price) |
| **wasted calls / case** | the loop's own overhead; small models flail here |
| **round trips / answer** | provider-independent latency proxy (Fella targets ≤ 2) |
| **wall s** | reported, caveated (network + provider load) |
| **self-catch rate** | of the wrong answers, how many `verify` flagged — *Fella-only*, and the trust story |
| **policy adherence** | correctly declines the forecast case vs fabricates a number |

`bare` has no evidence trail, so self-catch / grounding / waste are 0 for it by
construction — that contrast is the point.

### Reading Δacc and its 95% CI

**The metric underneath is `acc`** — the fraction of the battery a model gets
right: `acc = cases correct / 64`, written as a percent. `bare` and `fella` each
have their own `acc` per model (the two left columns of the results table).

**Δacc** = acc(fella) − acc(bare) for one model, on the *same* 64 cases — a
paired difference, so the model is held fixed and only the harness changes. Its
unit is **percentage points of that accuracy**, not "percent of": with 64 cases
one case ≈ 1.6 points, so `+22` ≈ 14 more questions right, `+45` ≈ 29 more.
`+22` on a 67% baseline means 89%, not 67 × 1.22. e.g. gemma: bare 34/64 (53%) →
fella 64/64 (100%) → **Δacc = +47 points**.

The 64 cases are one sample of "questions someone might ask a folder"; a
different 64 would move Δacc. The **95% CI** says how far. `aggregate.py`
computes it by **paired bootstrap**: resample 64 of the 64 per-case
`(bare-right?, fella-right?)` pairs *with replacement*, recompute Δacc, 2000
times; the 2.5th–97.5th percentile of those 2000 values is the interval (seeded,
so it's reproducible). Read it as: rerun the benchmark with fresh question
samples and ~95% of the intervals would contain the true lift.

- **Same unit as Δacc** — percentage points of battery accuracy. `[+22%, +45%]`
  is "the lift is plausibly anywhere from +22 to +45 points, best estimate the
  observed Δacc (+33)."
- **The endpoints are gain sizes, not accuracies.** `[+22%, +45%]` does **not**
  mean "fella scored 89%". deepseek's fella `acc` is a measured 100/100%; `+22`
  is the gap in a resample where *bare* happened to score 78%.
- **CI excludes 0** — deepseek-v4-flash `[+22%, +45%]`: the lift is real, even an
  unlucky draw of cases still shows ≥ +22 points. 9 of 10 models are here.
- **CI straddles 0** — muse-glimmer-30b `+6%`, `[−9%, +22%]`: *cannot* conclude
  the harness helps this model; the observed +6 is inside the noise. The one
  non-result.
- **Width ≈ ±12 points** here because n = 64 is modest — a bigger battery
  tightens the intervals.

The other metrics are *not* deltas: `acc`, `consistency`, `tok / correct`,
`$ / 100-correct` and `waste / case` are each reported per condition (`bare` and
`fella` columns). Only `Δacc` and its CI compare the two, which is why they
carry the "is the lift real" question.

Why it's first-class: without the CI, a +6% and a +33% look like the same kind
of result. The CI is what separates "the loop earns its place for this model"
from "we can't tell."

## The battery

**`bench/folder-qa/`** — 66 hand-built cases across ~11 life domains, so it isn't
a finance benchmark: spending + budget, workouts (minutes and km), a reading log
+ an authors table, trips, a plain-text journal, screen-time, sleep, contacts, a
goals note, subscriptions, and a lease. Question shapes: numeric aggregate /
filter / average / multi-step / delta, categorical group-by / top-N / rank-2 /
distinct-count, boolean filter, temporal max, free-text lookup / count / search,
three "what will I…" forecasts (must decline), one no-tool, and a 12-case
multi-file tier where
each needs a JOIN or a table + a document (most non-finance: pages by British
authors = books ⋈ authors; minutes on journal-noted run days = journal ∩
workouts; contacts in visited countries = contacts ⋈ trips; nationality with the
most pages = books ⋈ authors; km short of the goal = goals.md + workouts). Three
**distractor files** nothing asks for (`receipts_2019.csv`, `old_notes.md`,
`playlist.json`); 12 cases stage a *cluttered* folder so the model has to pick
the right files, not just be handed them.

The **Results** below are from the 64-case version; two more forecast cases
(`fqa-refusal-spend`, `fqa-refusal-trips`) were added afterward when the run
surfaced the forecast-fabrication bug (see below) — a re-run would be on 66.

**Every file type Fella ingests is covered**, one file per format: CSV, TSV
(`screen_time.tsv`), JSON array (`contacts.json`), NDJSON (`sleep.jsonl`),
Markdown (`goals.md`), XLSX (`subscriptions.xlsx`), PDF (`lease.pdf`). Four of
the multi-file cases join *across* formats (JSON×CSV, MD+CSV, NDJSON×TSV,
PDF+CSV). `--harness bare` can't parse a truly binary format (xlsx) — it embeds
it as a "binary file, not readable" stub, so those single-file cases are
effectively Fella-only (the hand-rolled PDF is plain text, so `bare` reads it).

Deterministic gold (`gen.py`, stdlib — the xlsx/pdf writers are hand-rolled
OOXML / PDF, no third-party deps; regen is byte-reproducible). Runs free on
`gemma4:31b` and any ollama-cloud model.

## The hard tier — `bench/folder-qa-hard/`

`bench/folder-qa/` targets the everyday-question tier on purpose (see the top
of this doc); its own `gen.py` comment says the next step is a harder battery
on the same schema. This is that battery: **16 cases**, same data files
(byte-identical copies of `folder-qa/`'s CSV/JSON/MD, since they're
deterministic — no xlsx/pdf, those formats aren't needed here), but harder
question shapes:

- **rank-2 / rank-min with a real gap** — "which category was the
  *second*-most over its annual budget", "by the *smallest* amount" — not
  just top-1.
- **unit conversion the DB can't do** — "how many *miles* (not km)" forces
  the model to convert, not just report a stored column.
- **non-standard-date half-year boundary** — `rent_ledger.csv`'s `"Jan 1,
  2024"` strings, split Jan–Jun vs Jul–Dec.
- **compound-filter joins** — contacts in countries visited *for leisure, not
  work* (not just "visited"); contacts in countries visited *only* for work.
- **a 5-file synthesis case** — check each of 5 stated goals against its own
  file, report the one actually on track.
- **two-group cross-file comparison** — average screen time on
  worst- vs best-sleep-quality nights.
- **two traps** — a near-miss category name that doesn't exist
  ("restaurants" vs the real "dining") and a date outside the data's range
  (Jan 2025) — the correct answer is a plain zero/no-data, not a guess or a
  match on the nearest real label.
- **two chart-generation cases**, graded on the `make_chart` tool call itself
  (see `Gold::Chart` in `agent_eval.rs`) — labels + numeric series compared
  to gold, order- and series-name-agnostic (a model can chart categories in
  whatever order and call its one series whatever it wants; it still has to
  get the *numbers* right). Prior chart cases in the easy tier
  (`fqa-rentl-chart`, `fqa-fin-chart-cat`) only graded a side numeric claim in
  the prose — a model could describe the right numbers and never actually
  chart them and still pass. These two can't be passed that way.

All gold values are computed by `bench/folder-qa-hard/gen.py` from the copied
files at generation time, not hand-typed, so a data edit can't silently drift
from the grader.

```
python3 bench/folder-qa-hard/gen.py   # regen cases.jsonl if the copied data changes
agent_eval bench --dir bench/folder-qa-hard --models "$MODELS" --harness fella --iters 3 \
  --json bench/folder-qa-hard/out/fella.json
```

Same `Gold` shapes as the easy tier, plus:
`{"chart": {"labels": [...], "series": [{"name": "...", "values": [...]}]}}`.

**Results (2026-09-12, the actual baseline models — `ollama-cloud/gemma4:31b`
and `openrouter/openai/gpt-5.6-luna`, `--iters 3`):** both **16/16**. Getting
there took two rounds — the first pass showed 14/16 and 15/16 with what
looked like real misses, but three of the four failures turned out to be bugs
in this battery's own grading, not model gaps:

- **`fqah-third-month`'s gold only matched the digits "07"**, not the word
  "July" — both models answered correctly (right month, right $123.48 gap)
  every single time and were marked wrong regardless.
- **`fqah-chart-h2-line`'s gold expected abbreviated month labels** ("Jul")
  but gemma correctly charted the ISO `YYYY-MM` labels it read straight from
  the `month` column ("2024-07") — a reasonable, correct choice the grader
  didn't allow for.
- **`fqah-trap-2025`'s "says no data" check** didn't recognize the phrasing
  "is not listed" (only "no ", "none", "n/a", etc.) — gemma's answer was
  already correct.

All three are now fixed (`gen.py`'s month-name lookup; `Gold::Chart` labels
accept `"name|iso"` alternatives the same way `Gold::Contains` already does;
`says_zero()`'s phrase list widened) and covered by new unit tests in
`agent_eval.rs`. The one real, reproducible finding: on `fqah-goal-ontrack`
(the 5-file synthesis case), gemma's SQL filtered on `'Dining out'` and
`'Leisure'` — capitalized, not matching the actual lowercase data — got an
empty result back, and **didn't notice**; it answered "goals.md wasn't
provided" instead, an unrelated and incorrect excuse. Majority vote (2 of 3
iters) saved the case, so it doesn't show as a miss in the headline number,
but the underlying case-sensitivity-then-silent-empty-result failure mode is
real and worth tracking separately from accuracy.

**So: this particular 16-case battery isn't creating headroom on the two
baseline models** — they're simply strong at this question class once graded
correctly. That's a legitimate result, not a wasted effort: it says the
*easy* tier's near-100% ceiling (see Results above) isn't a grading
artifact, and it puts a number on how much harder "harder" has to get before
gemma/gpt-5.6-luna actually miss something. Real headroom on *this* task
class most likely needs either (a) genuinely deeper multi-hop chains (4+
files, not the 2-3 used here), or (b) moving down the model ladder — the
64-case easy-tier run already shows where accuracy actually falls off
(`muse-glimmer-30b`, and further down toward the free/cheap end).

## Running it

```
DATA=/copy/of/fella.db+auth.json   # must hold apikey:openrouter (+ apikey:ollama-cloud)

MODELS="openrouter/deepseek/deepseek-v4-flash-0731,openrouter/z-ai/glm-5.3-flash,\
ollama-cloud/gemma4:31b,openrouter/openai/gpt-5.6-luna,\
openrouter/thinkingmachines/inkling-small,openrouter/google/gemini-3.8-flash,\
openrouter/deepseek/deepseek-v4-pro-0813,openrouter/x-ai/grok-4.3"
# nemotron-3.5-lightning, muse-glimmer-30b and muse-spark-1.3 are dropped from
# the roster (see "Model ladder" below) -- not run in the next benchmark.

AGENT_EVAL_DATA_DIR=$DATA BENCH_PAUSE_MS=400 cargo run --release --features eval \
  --example agent_eval -- bench --dir bench/folder-qa --harness bare  --iters 3 \
  --models "$MODELS" --json bench/folder-qa/out/bare.json
#  …then --harness fella → fella.json   (--json is flushed after every model)

python3 bench/aggregate.py --lift --bare bench/folder-qa/out/bare.json \
                           --fella bench/folder-qa/out/fella.json --prices
python3 bench/aggregate.py --csv  bench/folder-qa/out/results.csv \
                           --bare …/bare.json --fella …/fella.json
python3 bench/chart.py bench/folder-qa/out/results.csv   # → summary.csv + lift.html
```

- `BENCH_PAUSE_MS` (default 400) spaces cases so a hosted endpoint doesn't
  degrade mid-run; `--iters 3` majority-votes over transient flakiness.
- Only the *first* `/` in a `--models` entry splits provider from model, so
  `openrouter/deepseek/deepseek-v4-flash-0731` and `ollama-cloud/gemma4:31b`
  coexist in one list; `set_model` rewrites provider + base_url + model per
  entry. A bare entry (no `/`) inherits the previous entry's provider.
- **Don't run two `bench` processes against the same `AGENT_EVAL_DATA_DIR` at
  once** — they share `fella.db` and race on the provider/model row. Give a
  concurrent run its own copied data dir **and** `TMPDIR` (the per-case staging
  dir is `$TMPDIR/fella-bench-ext`).

## Model ladder (revealed-preference cheap models, Sept 2026)

Prices verified 2026-09-09 against OpenRouter `/api/v1/models` + per-model
`/endpoints` (headline rate; OpenRouter load-balances providers, so `$/correct`
is an estimate — the authoritative spend is the dashboard). ollama-cloud on a
plain key only serves `gemma4:31b` free (the rest is credit-gated), so
everything except gemma runs through OpenRouter (one key).

| # | slug | $/1M in–out |
|--:|---|--:|
| 1 | `openrouter/deepseek/deepseek-v4-flash-0731` | 0.065 / 0.18 |
| 2 | `openrouter/z-ai/glm-5.3-flash` | 0.075 / 0.25 |
| 3 | `ollama-cloud/gemma4:31b` | free |
| 4 | `openrouter/openai/gpt-5.6-luna` | 0.20 / 1.20 |
| 5 | `openrouter/thinkingmachines/inkling-small` | 0.45 / 1.20 |
| 6 | `openrouter/google/gemini-3.8-flash` | 0.75 / 3.75 |
| 7 | `openrouter/deepseek/deepseek-v4-pro-0813` | 1.0494 / 3.1482 |
| 8 | `openrouter/x-ai/grok-4.3` | 1.25 / 2.50 |

**Dropped from the roster** (2026-09-12, not run in the next benchmark):
`nemotron-3.5-lightning` (rung 3 previously) — its `fella` condition was
already unmeasurable here (~50% of tool-calling requests failed at the
OpenRouter/NVIDIA endpoint, see below); `muse-glimmer-30b` (rung 6) — the one
model whose lift didn't clear the noise floor (Δacc CI straddled 0) and the
worst offender on value errors and wasted calls; `muse-spark-1.3` (rung 10) —
measurable but the most expensive per correct answer on the ladder, with no
counterbalancing strength. `muse-spark-1.3-contributor` ($0.10/$0.20) was
separately dropped before this: its endpoint requires opting into
prompt-training (OpenRouter privacy setting).

Actual OpenRouter spend for the whole ladder × {bare, fella} × `--iters 3` on
the 64-case battery: **≈ $3** (gemma free) — the 3 dropped models are removed
from that estimate for the next run.

## Results (2026-09-10, `main` + the 64-case battery)

Full table: `bench/folder-qa/out/lift-table.md`; per-case CSV: `results.csv`;
chart: `lift.html` (3 inline-SVG panels + the table, no deps).

| # | model | bare | fella | Δacc | 95% CI | fella 3/3 | fella tok/ok | $/100-ok fella | waste/case |
|--:|---|--:|--:|--:|:-:|--:|--:|--:|--:|
| 1 | deepseek-v4-flash-0731 | 43/64 | 64/64 | **+33%** | [+22,+45] | 63/64 | 4,364 | $0.03 | 0 |
| 2 | glm-5.3-flash | 44/64 | 62/64 | **+28%** | [+17,+41] | 61/64 | 4,028 | $0.03 | 0 |
| 3 | nemotron-3.5-lightning | 26/64 | — | — | — | — | — | — | — |
| 4 | gemma4:31b *(free)* | 34/64 | 64/64 | **+47%** | [+34,+59] | 64/64 | 3,706 | $0.00 | 0 |
| 5 | gpt-5.6-luna | 38/64 | 63/64 | **+39%** | [+27,+53] | 62/64 | 3,092 | $0.07 | 0 |
| 6 | muse-glimmer-30b | 39/64 | 43/64 | +6% | [−9,+22] | 30/64 | 7,748 | $0.30 | 4.5 |
| 7 | inkling-small | 44/64 | 63/64 | **+30%** | [+19,+42] | 63/64 | 3,900 | $0.18 | 0.4 |
| 8 | gemini-3.8-flash | 53/64 | 63/64 | **+16%** | [+6,+27] | 62/64 | 3,575 | $0.33 | 0 |
| 9 | deepseek-v4-pro-0813 | 40/64 | 64/64 | **+38%** | [+25,+48] | 64/64 | 4,375 | $0.49 | 0 |
| 10 | muse-spark-1.3 | 42/64 | 62/64 | **+31%** | [+19,+45] | 61/64 | 5,673 | $0.94 | 0.4 |
| 11 | grok-4.3 | 59/64 | 64/64 | **+8%** | [+2,+14] | 64/64 | 4,237 | $0.56 | 0 |

**Overall (10 models measured both ways): bare 68% → fella 96%, Δacc +27.5%;
median per-model +30.5%, range +6% … +47%.**

What it says:

- **The scaffold sets the ceiling, not the model.** `bare` accuracy spans
  34–59/64 across the ladder; with the loop, 9 of 10 measurable models land at
  **62–64/64** regardless of price or size. A free 31B model + the loop matches
  a $1.25/1M reasoning model.
- **Every measurable model gets a statistically significant lift** (CI clears
  zero) *except* `muse-glimmer-30b` (+6%, CI [−9,+22]) — and it's also the only
  one that flails: **4.5 wasted tool calls/case**, 7,748 tok/correct, and just
  30/64 consistent across all 3 iters. A model can be big enough to answer yet
  not disciplined enough to drive the loop.
- **The lift shrinks but survives at the top.** grok-4.3 (bare 92%) still gains
  +8% [+2,+14]; gemini-3.8-flash (bare 83%) gains +16%. The loop closes most of
  the residual gap even where the model is already strong.
- **Cost of a correct answer, with the loop:** $0.00 (gemma, free) to $0.94
  (muse-spark) per 100. Best value: **gpt-5.6-luna** (+39%, 0 waste, $0.07) and
  **deepseek-v4-flash** (+33%, 0 waste, $0.03).
- **Round trips:** the loop answers in ≤ 2 tool-call rounds on the clean runs
  (`steps` ≈ 1 in `results.csv`); only muse-glimmer inflates it.
- **`nemotron-3.5-lightning` fella is not measurable** — ~50% of tool-calling
  requests fail at the OpenRouter/NVIDIA endpoint (18–49 s, then error), on two
  separate runs. Its `bare` pass was 100% clean (26/64). The harness needs a
  model *and an endpoint* that can hold a multi-round tool loop.
- **Self-catch = 0.** `verify`'s hard-fail did not flag a single wrong `fella`
  answer. It guards against regression-on-recompute (cited query broke / returns
  something new / figure ungrounded), not wrong-computation — see
  **Where the 28 wrong answers came from** below.
- **Policy adherence is the model's, not the harness's.** On `fqa-refusal`
  ("how many books will I finish next year?") only **5 of 10** models declined —
  glm-5.3-flash, muse-glimmer-30b, inkling-small, gemini-3.8-flash and
  muse-spark-1.3 fabricated a forecast despite the prompt's no-forecasting
  rule. `fqa-notool` (a definition question): 10/10 answered with no tool call.

### Where the 28 wrong `fella` answers came from

640 `fella` answers (10 models × 64), **28 wrong (4.4%)**. None were endpoint
errors; **`verify` flagged none of them**. By failure mode:

**1 — Forecast fabrication (5 answers, all on `fqa-refusal`).** "Based on my
reading log, how many books will I finish next year?" — glm-5.3-flash,
muse-spark-1.3, muse-glimmer-30b, inkling-small and gemini-3.8-flash each
computed a number instead of declining. `bare` 80% → `fella` 50%: the tool
loop's "you have `run_sql`, go compute" framing overrode the no-forecast rule
(`agent.rs` `refuse_rule`). **Fixed** — the rule now says outright not to run a
query to estimate a future value and covers more phrasings; on a 3-case
forecast check (`fqa-refusal` + the two new cases) the two models that were
fabricating (glm-5.3-flash, gemini-3.8-flash) now decline every time, with
gemma unchanged. A full re-run on the 66-case battery would confirm it across
the ladder.

**2 — Value errors (23 answers): a valid query, a real number, faithfully
reported — but the wrong computation.** These are *concentrated in one model*:
**20 of the 23 are `muse-glimmer-30b` alone**, spread across every domain and
every numeric shape (`num-aggregate`, `num-avg`, `num-filter`, `distinct-count`,
`text-max`/`min`, `cat-filter`, `mf-join-aggregate`). Same model as the +6%
Δacc, 4.5 waste calls/case, 30/64 consistency — the loop can't rescue a model
that miscomputes. For the **other 9 models, value errors total 3 in all**:

| case | tier | model | the trap |
|---|---|---|---|
| `fqa-read-top-genre-pages` | cat-groupby | glm-5.3-flash | "which genre did I read the most **pages** of, among **finished** books" — grouped by *book count*, or dropped the `finished='yes'` filter |
| `fqa-read-genres` | distinct-count | gpt-5.6-luna | distinct genre count off by one |
| `fqa-mf-most-over-budget` | mf-join-compare | muse-spark-1.3 | actual − 12×monthly-budget, picked the wrong category |

The question shapes that invite value errors: **numeric aggregates/averages with
an implicit filter** ("finished" books, "active" subscriptions, workouts that
"logged" a distance), **distinct-counts**, and **"which X has the most Y" where
Y is a sum, not a count**. `verify` is blind to all of these — the query runs,
the figure is grounded, so `hard_fail` never fires. Grounding-only verification
can't see "valid query, wrong question"; that is the self-catch gap, and it
matters more than 4.4% suggests once questions get ambiguous or multi-hop.

## Task taxonomy & measured difficulty

`cases.jsonl` carries `domain` / `tier` / `multifile` / `cluttered` / `formats`
per case; `python3 bench/taxonomy.py bench/folder-qa/out/results.csv` joins them
with the run and writes `by_domain.csv`, `by_tier.csv`, `by_cut.csv`,
`by_case.csv` (hardest first) and `taxonomy.md`. **Difficulty is measured** —
mean accuracy across the 10 models — not assigned, so it stays comparable as the
battery gets harder.

**By life-data domain** (bare → fella):

| domain | n | bare | fella | Δ | | domain | n | bare | fella | Δ |
|---|--:|--:|--:|--:|---|---|--:|--:|--:|--:|
| subscriptions *(xlsx)* | 4 | 0% | 95% | **+95** | | spending | 12 | 56% | 98% | +42 |
| screen-time *(tsv)* | 4 | 30% | 98% | **+68** | | housing *(pdf)* | 4 | 92% | 100% | +8 |
| sleep *(jsonl)* | 3 | 40% | 97% | **+57** | | contacts / travel / journal | 14 | ~97% | ~98% | ~0 |
| fitness | 10 | 48% | 95% | **+47** | | reading | 11 | 94% | **89%** | **−6** |

- **`subscriptions` bare = 0%** because `bare` can't parse `.xlsx` at all — the
  whole domain is an ingestion result. `touches xlsx or pdf`: bare 46% → fella
  98% (+51); `plain csv/txt only`: 74% → 95%.
- **`reading` is the one domain where the loop *hurts* (−6%)** — its tables are
  tiny (8 books), so `bare` already nails them and the loop occasionally
  over-works a question that needed no SQL.

**By task shape** — biggest lifts on computation (`num-multistep` +60,
`cat-rank` +55, `num-aggregate` +50, `cat-groupby` +48); flat-to-slightly-negative
on already-trivial text lookups (`text-max` −7, `text-list` −10). **`refusal`
80% → 50%**: the tool loop's "compute an answer" pull made models *fabricate*
the "books next year" forecast more than bare did — a guardrail regression, now
**fixed** in the `refuse_rule` prompt (see failure modes above).

**Structural cuts:** multi-file (+27) ≈ single-file (+28); cluttered folder
(+30) ≈ clean (+27) — distractor files don't dent Fella's retrieval.

**This battery is now too easy for the harness.** Only **2 of 64** cases land
under 90% fella accuracy (`fqa-refusal`, `fqa-read-top-genre-pages`) and **18 are
saturated** at bare-100% / fella-100%. It cleanly separates *bare* (34–59/64) and
proves the lift, but it no longer stresses *Fella*.

## Toward harder batteries

Keep the schema (`domain`/`tier`/`multifile`/`cluttered`/`formats` + deterministic
`gen.py` gold + measured difficulty) and raise the difficulty along these axes:

- **Scale** — tables of 10k–100k rows (the `folder-scale` cmd already synths
  these), 30–60 files in the folder, documents of many pages.
- **Ambiguity** — questions with an implicit filter or a term the data defines
  loosely ("my big trips", "recently"); questions answerable two defensible ways.
- **Multi-hop** — 3+ files, or a join whose key needs deriving (name→id via a
  third table), or a figure from a PDF that must be reconciled against two tables.
- **Adversarial data** — the `robustness` traps (amounts as text, totals rows,
  mixed date formats), duplicated rows, unit mismatches, a distractor file whose
  schema *looks* like the answer's.
- **Trust** — cases with a knowably-wrong premise the model should push back on;
  cases where `verify` *should* fire (self-catch is 0% here).
- **Retire the 18 saturated cases** or fold them into harder compound questions.

Track each new battery as its own `bench/folder-qa-v2/` (etc.) with the same
tooling; `taxonomy.py` + `aggregate.py --lift` give a like-for-like difficulty
and Δacc history.

## Not in scope

Comparison against funded data-analysis harnesses (ChatGPT code_interpreter,
Claude analysis, Julius). The `--harness openai-ci` adapter stays in the code
for anyone who wants it, but on this task tier every execution harness saturates
near the top on accuracy — the interesting axes are efficiency, cost, locality,
and trust, which `bare` vs `fella` already covers. DABStep's published
smolagents baseline is the number to cite if an external anchor is ever needed.

## Status

- [x] `bench --dir` + JSONL loader + `$/100`
- [x] `--harness bare` — the no-harness baseline (`EngineState::ask_once_usage`)
- [x] `--harness openai-ci` — kept, not the focus
- [x] `bench/folder-qa/` battery + `gen.py`; `bench/convert.py` (DABStep/InfiAgent, untested)
- [x] `bench/aggregate.py` — roll dumps into a comparison table
- [x] `aggregate.py --lift` — pair a bare + fella dump, emit Δacc per model + a
  paired bootstrap 95% CI; `--csv` writes a tidy per-case table
- [x] `bench/chart.py` — `summary.csv` + `lift.html` (stdlib inline-SVG, no deps)
- [x] `bench/taxonomy.py` — per-domain / per-tier / per-cut / per-case difficulty
  from `cases.jsonl` taxonomy + `results.csv`
- [x] the ladder run + the chart — 11 rungs, 64 cases, `--iters 3`; see
  **Results** + **Task taxonomy** above and `bench/folder-qa/out/`
- [ ] harder battery v2 (see **Toward harder batteries**)

# What "lightweight" actually means

"Small, fast, anti-bloat" (`README.md`, `PRINCIPLES.md`) has never been one
thing. Read closely, the project has used it to mean four different,
genuinely independent properties, sometimes in the same sentence. This doc
names them, says which existing numbers already hold each one to account,
and — the part that didn't exist anywhere before — which parts of the
engine are allowed to spend weight on which axis, and which must not.

## The four axes

1. **Binary/dependency weight.** What ships, and what a cold build costs.
   Measured today: `docs/PERFORMANCE.md`.
2. **Runtime performance.** What a person feels while using it: startup
   time, time-to-answer, memory while running. Partially measured today
   (`docs/PERFORMANCE.md`'s "Agent-loop latency" section); this doc adds
   the piece that was missing, which parts of *this* axis actually
   dominate.
3. **Codebase/architectural simplicity.** "One person can read all of it"
   (`docs/WHY.md`) — whether the harness stays small enough to audit by
   reading it, independent of how big the shipped binary is or how fast it
   runs.
4. **Feature/scope minimalism.** "Vertical, not horizontal"
   (`DECISIONS.md`, 2026-09-08): the base build stays narrow in *what it
   does*. **Locked, not reopened by this doc** — restated here only so all
   four axes are in one place; see that entry and `docs/NON-GOALS.md` for
   the actual reasoning. Nothing below trades against this one.

These don't move together. DuckDB-as-default cost 5x the binary (axis 1)
for effectively **no change to idle runtime memory** (axis 2, PERFORMANCE.md's
before/after table: 173 MB → 177 MB, "unchanged; webview-dominated") —
proof the axes are genuinely separate, not four names for the same thing.

## Axis 1: binary/dependency weight

Real numbers exist: `docs/PERFORMANCE.md`, `docs/PERFORMANCE-LOG.md`. As of
the SQLite-default migration: **11 MB stripped**, down from 54 MB with
DuckDB default (−80%); cold release build ~3 min, down from ~9 min.

The embedded Python guest adds about **6.7 MB** to the shipped source artifact.
That is the deliberate cost of carrying one capability boundary across the
three desktop OSes without requiring a Python installation or maintaining an
OS-specific sandbox executable.

**Is sub-50MB the actual bar?** No, and the DuckDB number itself shows why
a round absolute threshold is the wrong tool: 54 MB would have *snuck
under* a naive "under 50 MB" rule despite being the single change that made
the binary 5x heavier for a capability (Parquet, big-file speed) most
installs never touch. The rule that actually held, because it came from a
real measurement (`cargo bloat`) instead of a round number: **no single
dependency should dominate the binary's `.text` share**, and the base
should sit in the tens of MB, not hundreds. Post-migration, nothing does —
the largest single crate share is `std` itself at 15%.

**Component verdict:**

| Component | Axis-1 cost today | Allowed to grow? |
|---|---|---|
| `analytics::data` (SQLite backend) | ~1.3% of `.text` (bundled, tiny) | Stays default. This is the floor. |
| `analytics::data` (DuckDB backend) | Was >50% of `.text` alone | **Opt-in only** (`--features duckdb`). Not revisited by the retrieval-at-scale work (#125) unless the SQLite-side fixes (streaming/lazy ingestion, parallelism — already the roadmap's stated candidates) genuinely can't get there. |
| `analytics::pyexec` + embedded Python guest | Was subprocess-only; now adds Wasmi plus a stripped ~6.7 MB RustPython/WASM artifact | This is the deliberate portability/security tradeoff: one capability boundary across Linux, macOS, and Windows, with no Python install or OS-specific sandbox executable. |
| `analytics::chart` | Near-zero — a few structs + one validation function; rendering is 100% client-side JS, not in this binary at all | Fine to grow modestly (a new chart kind, e.g.) — it was never a weight concern. |
| `analytics::verify` | Zero new deps | N/A |
| `tools.rs` (the fixed tool set) | Whatever each tool's own backend costs (already accounted above) | Locked at 7 tools by axis 4, not a weight decision. |
| Ingest (`pdf`, `xlsx` features) | `lopdf`/`pdf_extract` 3.3%, `calamine` 4.0% | Already the pattern to follow: real user-facing capability, feature-gated, each independently measured. |

## Axis 2: runtime performance

Also already measured, partially: `docs/PERFORMANCE.md`. Startup: **<1000 ms
warm** (target, met — 516 ms observed). Idle RSS: **~150 MB**, but that's
almost entirely the WebView, not fella's own code. The number that was
never stated plainly, and is the actual answer to "what feels slow":

> **`run_sql` is ~10–50 ms. A model round trip is seconds.** Fella's own
> code is never the bottleneck a user notices. Time-to-answer is
> dominated, almost entirely, by *how many model round trips a question
> costs*, not by anything in this codebase.

That reframes "runtime-lightweight" for this project: it isn't really
about CPU or memory efficiency of fella's own compute (SQLite queries are
already millisecond-scale), it's about **keeping the model-call count per
question low**. `PERFORMANCE.md`'s own target: a simple question should be
one or two model calls, landing well under 30 s once the model is warm.

**What this means for the depth_rule work.** The decomposition pattern
added this session (group by a second dimension, isolate outliers,
recompute) sounds like it should cost a third model round trip. It
doesn't, in the common case: those two queries are independent of each
other, so the existing `parallel_rule` ("independent lookups go in one
reply as several tool calls; they run together") batches them into **one**
model turn, and the model's own final answer (which needs both results) is
the second turn. **Two round trips, not three** — the existing
architecture already absorbs the "enterprise-grade" analytical pattern
without a runtime-weight cost, as long as the sub-queries stay
independent of each other. That stops being true the moment a step
genuinely needs the *result* of a prior step before it can be written —
see the linear-loop section below for where that starts to cost real time.

**Component verdict:**

| Component | Axis-2 cost | Notes |
|---|---|---|
| `analytics::data::run_sql` | ~10–50 ms | Not the bottleneck, ever. |
| `analytics::pyexec::run` | Wasmi + RustPython startup, on top of the script's own time | Release tests run the two basic snippets in ~0.18 s on the development machine. The guest artifact is the main size cost; keep Python focused on calculations SQL cannot express. |
| `analytics::verify::rerun_queries` | One extra `run_sql` per *distinct* cited query, deduped, and **skipped** for a query that was already slow (>500 ms) or truncated | Already axis-2-aware by design — a good existing example to match going forward, not something this doc is introducing. |
| Model round trips | **Seconds. The dominant cost, by a wide margin.** | Everything else on this list is noise next to this one. |
| The system prompt itself | Every rule added (`depth_rule`, the expanded `python_rule`, the strengthened no-workspace instruction, `chart_rule`'s one-chart clause) costs **every single turn**, whether or not that rule is relevant to the question asked | This is the real, direct tension with this session's own prompt additions. `FELLA_PROMPT_DROP` (the ablation-testing env var already built for `agent_eval`) is the actual tool for measuring whether a given rule earns its place on this axis — it hasn't been re-run against the rules added this session. Worth doing before adding more. |

## Axis 3: codebase/architectural simplicity

The one axis with no numbers anywhere yet. A rough proxy, in the absence
of a better one: can a file be read in one sitting. Current sizes:
`state.rs` 2122 lines, `agent.rs` 1076, `tools.rs` 798, `analytics/verify.rs`
1369, `analytics/data/sqlite.rs` 1029. `state.rs` is the one genuinely
pushing past "one sitting" today — not a new problem, and not something
this doc proposes fixing, just naming honestly since it's the axis with no
stated budget at all.

The `analytics/` extraction (this session, prior commit) is a clean
example of spending effort on *this* axis alone: it changed zero bytes of
the shipped binary (axis 1), zero runtime behavior (axis 2), and nothing
about what the base build does (axis 4) — the entire point was moving
already-decoupled logic somewhere it reads as decoupled, and narrowing the
one piece (`verify.rs`) that wasn't. Proof the four axes really are
separable: you can move a thousand lines of code and not touch the other
three at all.

## Axis 4: feature/scope minimalism — not reopened here

Stated once, plainly, so this doc is complete on its own: the base build
stays vertical, not horizontal (`DECISIONS.md`, 2026-09-08). A capability
earns its place by making the existing job better, not by adding a
parallel thing to do. This axis doesn't trade against the other three —
it was never a weight or performance decision, it's a decision about what
Fella is *for*. Nothing in this doc, or in the "enterprise-grade" framing
generally, is a reason to revisit it.

## The linear-loop question

`agent.rs` is "one linear loop, purpose-built, ~one file"
(`docs/NON-GOALS.md`): no branching, no sub-agents, no speculative
parallel exploration of different approaches. Concretely, "linear" means
one shared conversation thread advancing step by step — it does **not**
mean one tool call at a time; multiple independent tool calls inside a
single step already run concurrently (`parallel_rule`).

**Real limitations of this shape:**
- No cheap way to explore two different approaches to an ambiguous
  question in parallel and keep the better one — a linear loop commits to
  one line of reasoning per step; changing course costs another
  sequential step (another model round trip, i.e. more seconds).
- No isolated sub-context for a genuinely independent sub-investigation —
  everything accumulates in one growing history, bounded by `num_ctx`
  (8192) and `trim_history`, rather than each sub-question getting its own
  smaller context the way a multi-agent system could give it.
- Cost scales with **sequential** (dependency-chained) step count, not
  total tool-call count — and each sequential step is a full model round
  trip, i.e. seconds, the dominant cost on axis 2.

**Does it matter for what Fella actually does?** Mostly, no. The kind of
depth `depth_rule` asks for, decompose a question, isolate outliers,
check a correlation, is 2–4 queries that are almost always independent of
each other and only need to be *seen together* at the final answer — which
is exactly the shape `parallel_rule` already handles in one batched turn,
not a shape that needs branching or sub-agents. Fella's job is bounded
(one folder, a verified answer), not open-ended multi-domain research
where a linear loop's lack of speculative branching would actually bite.

**Where it would start to matter** (named honestly, not currently a goal):
genuine multi-hypothesis testing (try three different groupings, keep
whichever tells the clearest story) or synthesis that spans many
genuinely independent sub-analyses each deep enough to need its own
context. Past roughly 5–6 truly *sequential* steps, at seconds per round
trip, a user would start to feel it, against `PERFORMANCE.md`'s own "well
under 30 s" bar for a simple question. That's a real boundary condition,
not a hypothetical one, but it's not the boundary any current roadmap item
(including the depth/correlation work) actually presses on.

## See also

- `docs/PERFORMANCE.md` — the actual measurement tooling and numbers for
  axes 1 and 2.
- `docs/PRINCIPLES.md` — "Anti-bloat in the base," the commitment this doc
  expands on.
- `docs/NON-GOALS.md`, `DECISIONS.md` (2026-09-08) — axis 4, locked,
  not revisited here.
- `DECISIONS.md` (2026-08-27, SQLite-default entry) — the actual
  measurement that produced axis 1's real rule.

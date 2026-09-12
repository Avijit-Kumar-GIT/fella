# The harness

The *harness* is everything wrapped around the model: what it may see, which
tools it can call, how its answers are checked. `WHY.md` makes the argument that
this matters more than the model choice; `ARCHITECTURE.md` documents the
mechanics. This file is the engineering log: the design choices and *why they're
model-agnostic*, the method for changing them, and a dated record of what's been
tried.

## Stance

Fella's harness is deliberately at the small end of every axis in the
[literature](#reference). The choices, and why each one holds across a weak
local model and a frontier one:

| Choice | Why it's model-agnostic |
|---|---|
| **One linear loop**, no planner/critic/sub-agents (`agent.rs`, ~one file) | Coordination overhead and context-sharing bugs scale with orchestration complexity, not model strength. A weak model derails in a multi-agent graph; a strong one doesn't need it. |
| **Deterministic tools are the only data path**; the model never emits a figure | The correctness floor is the same whatever the model. A better model writes better SQL; it can't make the numbers less checked. |
| **Deterministic verification, no LLM critic** (`verify.rs`) | A self-grading model is generous to its own output, and that bias is worse on weaker models. Re-running the cited SQL is exact for everyone. |
| **Minimal prompt**, split into toggleable sections (`PromptProfile`) | Every added instruction is a token tax on the strong model and a distraction risk for both. Measured: the shipped prompt is already at the Pareto point for a 31B local model (see log). |
| **Schema-in-prompt instead of RAG** | The folder *is* the scope. A retrieval config to tune is a second system that fails independently of the model. |
| **`num_ctx` is a growing floor, not a fixed size** (`fit_num_ctx`) | Small models have small default context; large folders need more. Growing only when the prompt demands it keeps a weak model from paging its whole context every turn. |

## Method

The rule that makes an optimisation *general* rather than model-specific:

> **Remove a failure mode or an ambiguity — don't add scaffolding.**

Removing ambiguity (a tool result that reads as "query broke" when it means
"zero"; a verification check that false-positives) helps whichever model was
tripping on it and is neutral for the rest. Adding scaffolding (few-shot
examples, forced planning turns, verbose step-by-step rules, aggressive history
trimming) props up the weak model *at the strong model's expense* — more tokens,
more distraction, more over-constraint. The first kind compounds upward: a
better model can only do better once the trap is gone. The second kind has a
ceiling and a cost.

This is roughly the maintainer's "optimise the worst case" intuition, made
precise. Two caveats:

- The "worst case" is *the worst model you'd actually run*. For Fella that is
  fixed: **the `gemma4` series** the shipped Ollama-Cloud default, the most
  tested model, and the floor the maintainer is willing to ship on. An
  optimisation that helps a model weaker than `gemma4` but costs `gemma4` or a
  frontier model is not taken. Tuning for a model nobody uses is waste.
- It only compounds upward for the *remove-ambiguity* class. The
  *add-scaffolding* class regresses the strong model, so the eval compares
  across a weak, a mid, and a frontier model on a **frozen** battery and keeps a
  change only if **no model regresses**. That check is what enforces the rule.

- **"Don't add scaffolding" is about correctness-neutral additions, not a token
  cap.** When a change *buys correctness* — per-folder memory is the case in
  point — the tokens are accepted; performance beats minimalism
  (`DECISIONS.md` 2026-09-07). Two guards remain: it must not *regress* the
  questions that don't need it (a distraction cost), and prompts stay
  **permissive** — no rigid step lock-step, no forbidding the model from
  exploring or reasoning its own way to a correct answer. Added context is
  reference the model *may* use, not a rule it *must* follow.

Mechanics: `docs/PERFORMANCE.md` §`agent_eval`. Frozen 18-case battery,
`--compare` two JSON runs, `--iters 5` (fewer is noisy — a single case flipping
at `--iters 3` is usually variance).

The running list of open design questions from shaping this work is in
`docs/QUESTIONS.md`.

## Log

### Shipped

- **2026-09-07 · Empty aggregate reads as "0", not a failed query**
  (`tools.rs`). A `SUM`/`AVG` over no rows is one all-NULL row → a blank cell →
  a weak model probes for the missing category. The result now says so.
  *Remove-ambiguity.* gemma4:31b `empty_cat`: 4 tool calls → 1, ~10.3K tok →
  3.9K; luna's one baseline miss fixed; grok a smaller win; nobody regressed.
- **2026-09-07 · Three `verify.rs` precision fixes.** A NULL aggregate backs
  the answer "0"; the re-run comparison tolerates float-serialisation jitter
  (`rows_match`); a digit inside an identifier (`txns_00`) isn't a stated
  figure (`number_tokens`). Latent for the fold warning; surfaced by making
  verification actionable. *Remove-ambiguity.*
- **2026-09-07 · Corrective re-ask**, then narrowed (`DECISIONS.md`
  2026-09-07). One tool-free turn to reconcile when a cited query *re-runs to a
  different result* (`verify::rerun_regression`). The broader trigger ("a
  figure appears in no result") was net-negative — on grok it rewrote a correct
  "≈18%" as the raw ratio "0.176…". `FELLA_VERIFY_REASK=0` disables. Dormant on
  read-only data by design.

- **2026-09-08 · Case-sensitivity flag** (`case_collision` at ingest;
  `case_sensitive_label_filter` in `verify` + `run_sql`). A label column whose
  distinct values collapse under case-folding (`Rent` / `rent` / `RENT`) gets a
  note; a bare `= '…'` / `IN (…)` filter on it that isn't `lower()`-wrapped
  gets an inline warning. *Remove-ambiguity* — surfaces a real property of the
  data, forces no rewrite. It's what let the `gemma4` floor turn a carried
  memory correction into a correct query (below): ✗ 4 850 → ✓ 7 350.

- **2026-09-08 · Per-folder memory v1** (`engine::memory`, branch
  `feat/folder-memory`, [`FOLDER-MEMORY.md`](FOLDER-MEMORY.md)). A plain-text
  `memory.md` per folder, written from deterministic signals (a `verify`-clean
  one-query answer → a recipe; a correction → a vocabulary note), a semantic
  core prepended after the schema block, empty for a fresh folder.
  `FELLA_MEMORY=0`/`ro`. *This is an add-scaffolding change, so it has to buy
  correctness.* Two `agent_eval memory` runs:
  - *Same-conversation, clean folder* — no accuracy/step/waste change, ~+130
    prompt tokens. Neutral-to-slightly-negative; clean synthetic tables don't
    need a recipe.
  - *Cross-session, messy folder* (session 1 corrects "rent"; a cold session 2
    asks the total) — with the case-sensitivity flag in place, **all three
    models go memory-on ✓ 100 % (7 350) vs memory-off ✗ 0 %**, for ~+180
    prompt tokens. Each folds case and applies the carried
    `IN ('rent','housing','mortgage')`. (gemma4 without the flag was ✗ 4 850 —
    right synonyms, case-sensitive `IN` — so the flag is what closed it.)
  Ships default-on: a fresh folder costs nothing, and it earns its tokens on
  the cross-session messy-folder case it exists for, across frontier and floor
  models.
- **2026-09-11 · Recipes cut from per-folder memory** (`engine::memory`,
  `docs/DECISIONS.md`). A user's real `memory.md` had cached a `strftime`
  GROUP BY over a non-ISO date column as a "verified" recipe (before
  `check_null_group_key` existed to catch that shape) it kept getting
  replayed verbatim on later asks, reproducing the same wrong answer with no
  fresh reasoning. Neither of the two `agent_eval memory` runs above actually
  showed a recipe earning its tokens — same-conversation was neutral, and the
  cross-session win came from the vocabulary note + case-sensitivity flag, not
  a recipe. The self-healing this doc originally called for ("a recipe that
  fails verify is demoted, two failures drops it") was never built; rather
  than build it for a mechanism with no demonstrated win, it's gone. Memory
  now stores facts only (preferences, vocabulary, table notes); `## Recipes`
  in an existing file is silently dropped on next load.

### Measured, no change

- **2026-09-07 · Prompt minimalism.** `prompt-ablation` on gemma4:31b: every
  section above the core rules + schema either loses a battery case
  (`background_rule`), drives a shipped feature (`note_rule` → the activity
  display), or is under-tested by the battery (`docs_rule`). Dropping the
  schema's sample rows sends redundant-peek waste 0 → 11. No cut is free.
- **2026-09-07 · Few-shot** — deferred; the ablation shows no prompt slack to
  trade for exemplars.
- **2026-09-07 · Folder scale.** `folder-scale` 1 → 120 tables: 18/18 through
  40 tables, 16–17/18 at 120; step cap never hit; latency flat. Adaptive
  `num_ctx` beats fixed 8192 at the top end. **`trim_history` by token budget
  not warranted** — no evidence of history bloat causing a miss.
- **2026-09-07 · Messy data.** `robustness` (text amounts, totals row, mixed
  dates, cumulative): 6/6 at every level. Ingest-time coercion (`parse_num`,
  totals-row drop) absorbs it before the model reasons about it. Scoped to
  format-level messiness on one synthetic table; doesn't cover category-label
  semantics or format diversity across life-domain files — see the two
  2026-09-12 messiness findings below, a different failure mode, not a
  regression of this one.

### Next

- **2026-09-12 · Case-mismatch on a uniformly-cased column, not caught by the
  case-sensitivity flag.** The shipped flag (`mixed_case_columns`, above) only
  fires when a column's *ingested data* actually has colliding case variants
  (`Rent`/`rent`/`RENT` all present). It has nothing to check when a column is
  internally consistent (`trips.purpose` is only ever lowercase `leisure`/
  `work`) but the model invents a differently-cased literal anyway
  (`= 'Leisure'`) — that's not a data collision, so no note, no warning, and
  the query silently returns 0 rows. New eval axes built to stop naming files
  in questions (real users don't; see `bench/out/dashboard.html`) caught this
  live: `bench/folder-qa-hard`'s `fqah-goal-ontrack`, gemma4:31b, 2 of 3
  iterations wrote exactly this query and concluded the wrong goal off the
  back of it. Candidate fix, unexplored: extend `case_sensitive_label_filter`
  to *every* bare `=`/`IN` text-column filter, not only ones gated on a
  detected collision — `lower()`/`COLLATE NOCASE` as a default habit, not a
  reactive flag.
- **2026-09-12 · Semantic near-duplicate category labels.** A different, harder
  problem from the one above — not a case mismatch but genuinely different
  words for the same thing (`HOUSING`, `mortgage` both meaning "rent"; see
  `bench/messiness`'s `msy-near-duplicate-labels`/`msy-categorical-casing`).
  gemma4:31b: 5/9 on the messiness axis, the worst of the ten axes measured.
  No candidate fix yet — `lower()`-by-default doesn't touch this.
- **2026-09-12 · Correction-trigger breadth, unmeasured.**
  `memory::is_correction()` fires only on a fixed marker-word list ("actually",
  "no,", "that's wrong", …) at the start of a message, and only with a prior
  question in the same conversation (`state.rs`'s `record_turn_memory`).
  `bench/memory`'s scored assertions (`memory-axes`, 5/6) all pass today, but
  every scripted correction in them deliberately used a marker phrase to
  trigger it — a correction phrased another way ("wait, that's not it", a
  plain rephrased follow-up) is untested, not confirmed working. Add cases
  before calling correction-detection itself validated, not just its mechanics.
- **Distill a small model on Fella's own verified tool-use traces.** Longer-
  horizon, written up in full in [`ROADMAP.md`](ROADMAP.md#harness-quality-measured) —
  the eval harness now produces exactly the two ingredients this needs: real
  tool-call trajectories, and a reliable grader that already knows which ones
  were actually correct.

### Open

- **Older / cheaper models.** `model-ladder` down a dated list — the point where
  bad SQL/arithmetic (model decay) overtakes a correct refusal (tool ceiling).
  Needs dated model ids; costs provider spend.

## What larger harnesses do that Fella doesn't

Frontier assistant harnesses (ChatGPT's agent mode, coding agents, the
frameworks) carry machinery Fella omits. Most omissions are deliberate; a few
are worth revisiting.

| Technique | Elsewhere | Fella's position |
|---|---|---|
| **Cross-session memory / learned playbook** | ChatGPT "memory"; persistent strategy memory keyed to a project | Only a per-conversation `recent` block. A *per-folder* learned playbook (this user's vocabulary, which tables mean what) is the [next planned change](#next) local, per-folder, bounded to a small prompt block. |
| **Code-as-orchestration (CodeAct)** | Model writes one Python program that calls several tools, runs once, returns a consolidated result — fewer round-trips, fewer places to derail | Fella *has* the pieces: `run_python` with a `sql()` helper. It isn't the encouraged default. Making it the pattern for multi-step questions is a model-agnostic round-trip cut. Candidate. |
| **Progressive context compaction** | LLM-summarise the transcript at token thresholds; structured handoffs; full context resets | `trim_history` only elides old tool results by count. `folder-scale` says Fella doesn't need more yet; this is the standard next tier if long multi-step runs start failing. |
| **Generator–evaluator separation** | A distinct critic model grades the worker's output against a rubric | Against "powerfully tiny". The deterministic `verify` pass plus the one narrow re-ask is the most Fella will do here. |
| **Plan-Execute-Verify with hard phase gates** | Pre-tool-call gates (known tool? valid args?); execution bounded to an approved plan | Fella has a soft plan rule and a post-hoc deterministic verify. Cheap pre-dispatch arg validation could save a round-trip; the rest (plan-bounds enforcement) needs a plan artifact Fella doesn't keep. |
| **Retrieval config (chunking, top-k, rerankers)** | Tuned with Bayesian search over 6–10 params | N/A — no RAG. The schema block is the "retrieval" and it's deterministic. |
| **Multi-agent / orchestrator-worker** | Declared agents with handoff edges | Explicit non-goal (`ARCHITECTURE.md`). |
| **Tool-description optimisation (span-level scoring)** | Score tool-*selection* accuracy separately from answer quality; rewrite overlapping descriptions | Fella's seven tool descriptions are already terse and non-overlapping; low value here, but the eval *could* score tool choice separately. |

## Reference

Background reading behind the stance and the technique table:

- Anthropic, *Harness design for long-running application development* —
  <https://www.anthropic.com/engineering/harness-design-long-running-apps>
- *Architectural Design Decisions in AI Agent Harnesses* (70-project study) —
  <https://arxiv.org/abs/2604.18071>
- *Automated Optimization for Agents in 2026: 5 Axes, Not 1* —
  <https://futureagi.com/blog/automated-optimization-for-agent-2026/>
- *Building AI Coding Agents for the Terminal: Scaffolding, Harness, Context
  Engineering* — <https://arxiv.org/abs/2603.05344>
- `ai-boost/awesome-harness-engineering` —
  <https://github.com/ai-boost/awesome-harness-engineering>

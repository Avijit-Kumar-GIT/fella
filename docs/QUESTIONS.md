# Open questions — harness & memory

A running register of the questions raised while shaping the agent harness and
per-folder memory. Each is kept as asked, with where it's answered or the fork
that's still open. Newest section last.

---

## Harness engineering (prior session → `agent_eval`)

- **"How can we empower the LLMs to more intelligently reason a solution? How do
  we implement the harness engineering that ChatGPT / Hermes / OpenCode do, and
  what optimisations exist?"**
  → Built `examples/agent_eval` (scored: correctness, closeness, tool-waste,
  tokens/correct). Landscape and Fella's position: `HARNESS.md` §"What larger
  harnesses do that Fella doesn't". Five optimisation axes (system prompt, tool
  descriptions, retrieval config, few-shot, model) from the references there.

- **"Build a full test suite: tool-use efficiency (mitigate *unnecessary*
  calls, not minimise calls), token efficiency, answer correctness, closeness
  to the expected answer, prompt minimalism, the biggest folder before serious
  drops, how far back in model age before the *model* (not the tool) decays."**
  → `agent_eval` subcommands: `accuracy`, `prompt-ablation`, `folder-scale`,
  `model-ladder`, `robustness`, `session-memory`. Metrics in
  `docs/PERFORMANCE-LOG.md`.

- **"Are we editing test cases to fit the output? We should change the code to
  fit the tests, not the tests to fit the code."**
  → Battery **frozen**. Grader changes need a behaviour trace + a ground-truth
  argument (rule in `PERFORMANCE-LOG.md` and in `grade()`'s comment). The 2026-09-07
  grader fixes each cite the trace.

- **"Diversify the cases; run more iterations; get a solid baseline *then*
  optimise the harness against it."**
  → 18 diverse cases (aggregate / filter / min-max / group / time-series /
  ratio / multi-step / trap / doc / empty / refusal / non-financial).
  Baseline: `PERFORMANCE-LOG.md` §"Baseline 2026-09-07". Then optimised —
  §"After 2026-09-07".

---

## Recording & process (2026-09-07)

- **"As long as we're model-agnostic I don't care too much — can we record
  these improvements somewhere?"**
  → `docs/HARNESS.md` (the running log + rationale), `PERFORMANCE-LOG.md` (numbers),
  `CHANGELOG.md`, `DECISIONS.md`. GitHub #40 / #41.

- **"What do higher-end harnesses like ChatGPT do that we currently aren't?"**
  → `HARNESS.md` §"What larger harnesses do that Fella doesn't". Short version:
  most of their size solves problems Fella doesn't have (arbitrary scope,
  multi-hour tasks, shared team context). The two that transfer — per-folder
  playbook memory and CodeAct-style orchestration — are both small.

- **"My theory: if efficiency is optimised on the worst case, a better model
  can only improve. True?"**
  → Mostly, with a split (`HARNESS.md` §Method): *remove a failure mode /
  ambiguity* compounds upward and is safe; *add scaffolding* (few-shot, forced
  planning, verbose rules) props up the weak model at the strong one's cost and
  has a ceiling. The frozen-battery `--compare`-across-3-models check enforces
  the distinction.

- **"Define the worst model I'll ship with."**
  → **`gemma4` series** — shipped default, most-tested. An optimisation that
  costs it isn't taken (`HARNESS.md` §Method).

- **"Create GitHub issues for everything so far, with their commit(s) and PR."**
  → #40 (engine/harness changes) and #41 (eval baselines + docs), both closed,
  commit lists in the closing comments; #42 open. All under PR #39.

- **"Put per-folder memory in its own PR?"**
  → Yes — branches off `main` after #39. The line between harness tuning and
  agent memory.

- **"Are there OSS memory libraries better than building custom?"**
  → No, for Fella. Mem0 / Letta / Zep / Graphiti / Cognee / LangMem each need a
  Python runtime, a vector/graph store, a server, or a per-turn LLM extraction
  call. Borrow the patterns (self-editable block, supersede-don't-append,
  one end-of-session extraction, file-as-truth). `FOLDER-MEMORY.md`, #42.

---

## Per-folder memory design (2026-09-07)

- **"Don't assume a folder is '10 short facts'. Account for larger datasets."**
  → `FOLDER-MEMORY.md`: what scales is schema notes (∝ columns) and recipes
  (∝ question shapes), ~tens of KB over a folder's life. Still not a vector-DB
  problem — schema notes ride the catalog's existing size-tiering; recipes /
  vocab are lexical (SQLite FTS5, already bundled).

- **"A `MEMORY.md`-style artifact per folder — what do you think?"**
  → Adopted as the shape. One human-readable file per folder, the *learned*
  sibling of `fella.md`; auditable, hand-editable, rebuildable index.

- **"We'd probably want some combination of episodic and semantic memory."**
  → `FOLDER-MEMORY.md` §"Episodic and semantic". Episodic = a capped,
  timestamped log of what happened (questions asked, answers reached,
  corrections). Semantic = the curated `memory.md`, *distilled from* episodic +
  ingest + corrections. Semantic is always-on-small; episodic is recall-only.

- **"Recall tool vs. `memory.md` file — which is more efficient for the input
  context window / token use?"**
  → Fork resolved as a split (`FOLDER-MEMORY.md` §Retrieval): the small
  **semantic core** goes in the system prompt (static, so prompt-cached after
  turn 1 ≈ 10% cost on repeat); **episodic + the long semantic tail** sit
  behind a `recall(topic)` tool (dynamic, paid only when the model reaches for
  it, kept out of the cached prefix). "Static first, dynamic last."

- **"In real-world LLMs, what's the greater tradeoff — tool use or token use?"**
  → **Tool use**, usually. Each tool call is a full extra round-trip
  (~1–3 s on the models tested; agentic workflows run 2–30× the tokens of a
  chat and 10–30 s for reflexion loops) and each step compounds the chance a
  weak model derails. Prompt tokens on a *stable* prefix are cheap and, with
  prompt caching, ~90% off on repeat. Token cost only wins the argument when
  the context is large **and** uncached, or the question is one-shot.

- **"Does the tradeoff matter if it buys a significant performance gain?"**
  → No. Single-shot accuracy plateaus ~60–70% on hard tasks; tool use + more
  context gets to 95%+. If memory takes `gemma4` from re-deriving-and-sometimes-
  failing to right-first-try, a ~400-token cached block or a ~1.5 s recall hop
  is trivially worth it. The cost analysis picks the *delivery*, not *whether*.

- **"I'd prefer performance over token efficiency / prompt minimalism, but not
  intense strictness — the model should have freedom to reason and still be
  correct."**
  → Recorded as a principle (`HARNESS.md` §Method, `DECISIONS.md` 2026-09-07):
  the "don't add scaffolding" rule is for *correctness-neutral* changes; when a
  change **buys correctness**, spend the tokens. And keep prompts permissive —
  no rigid step lock-step, no forbidding exploration; give room to reason.

### v1 built (2026-09-08, branch `feat/folder-memory`)

`engine::memory` — plain-text `memory.md` per folder, deterministic writer
(verified query → recipe; correction → vocab note), semantic core in the
prompt, empty for a fresh folder. `FELLA_MEMORY=0`/`ro`. Details:
`FOLDER-MEMORY.md` §Implementation.

`agent_eval memory` — two runs:
- *same-conversation, clean folder* → no accuracy/step change, ~+130 tokens.
- *cross-session, messy folder* (session 1 corrects "rent"; cold session 2 asks
  the total) → **luna 0 % → 100 %** (carried correction becomes the right
  `LOWER(cat) IN (…)` filter). gemma4 alone botched the case-fold; **with the
  case-sensitivity flag (added 2026-09-08) gemma4 goes ✗ 4 850 → ✓ 7 350**.

**"Can our memory system accurately convey ideas from previous sessions and
apply them in new sessions?"** (2026-09-08)
→ **Convey: yes** — the correction text crosses the session boundary intact.
**Apply: yes on a capable model** (luna turns a loose note into the correct
query), **partially on the `gemma4` floor** (it reads the note and moves in the
right direction but mis-executes on case-inconsistent data — a SQL ceiling, not
a memory failure).

### Still open

- **Close the `gemma4` gap** with more *actionable* notes: the deferred
  end-of-session tidy pass turns "count HOUSING and mortgage as rent" +
  observed categories into `rent → lower(cat) IN ('rent','housing','mortgage')`.
- Core-block token budget vs. the prompt-minimalism finding — maybe the core is
  *only* vocabulary + preferences, with even the top recipes behind `recall()`.
- `recall()` reliability on `gemma4` — does a weak model reach for it when it
  should? If not, pre-inject the top-K instead. (`recall()` not built in v1.)
- Recipe-match precision — a "2024 spend" recipe pulled for "2023 spend" and
  reused with the stale filter. How aggressively to genericise stored recipes.
- Episodic log retention — how many sessions / how much before it rotates, and
  whether the user ever sees it directly. (Log written in v1, not read.)
- The correction heuristic (`is_correction()` keyword match) — measure its
  false-positive rate on real follow-ups before trusting it.

---

## Data quality (2026-09-08, prompted by the memory benchmark)

- **"How often does case sensitivity actually matter in data and queries? Is it
  worth forcing case-insensitivity, or too strict?"**
  → *Forcing* it is too strict — it would break the rare case-significant column
  (SKUs, codes, slugs) silently, and a silent rewrite is against "shows its
  working". But for **Fella's domain** (personal data: category / status / type
  / merchant labels) case is almost always data-entry noise — `Rent` = `rent` =
  `RENT`. Direction: **surface, don't force.**
  1. *Ingest / schema (remove-ambiguity).* Fella already counts distinct values
     per column. When distinct-case-insensitive < distinct, flag it in the
     schema block: `"cat" TEXT [mixed case: Rent / rent / HOUSING …]`. The model
     then knows to `lower()`-fold. No rule, no rewrite.
  2. *verify + `run_sql`.* `case_sensitive_label_filter`: a bare `= '…'` /
     `IN (…)` on such a column that isn't `lower()`-wrapped gets an inline NOTE
     mid-loop and a soft warning post-answer.
  **DONE 2026-09-08.** On `agent_eval memory` (cross-session) it took gemma4
  from ✗ 4 850 → ✓ 7 350 — folds case *and* keeps the carried memory correction.

## External memory products (2026-09-08)

- **"Supermemory — a FUSE filesystem mount where `grep` becomes semantic search,
  a live `profile.md` synthesises context, any format auto-indexed, knowledge
  graph + a custom user-understanding model. Worth considering?"**
  → **No, not to adopt or depend on.** It conflicts with the constitution on
  nearly every axis: a hosted "user-understanding model" is a second thing
  leaving the machine (or a heavy local ML stack) vs. "one small binary, local
  by default"; a writable FUSE mount is a new dependency + failure + permission
  surface vs. "the folder is the boundary, read-only is the whole safety
  story"; "grep → semantic search" is a ranked non-deterministic result you
  can't re-run and check vs. Fella's literal-regex `grep_files`; a knowledge
  graph is the "memory system" #42 explicitly isn't building. **The one
  transferable idea** — a synthesised, human-readable `profile.md` — Fella
  already has as `memory.md`, minus the ML layer. Worth watching as a reference
  for the synthesised-profile pattern and the plain-`ls`/`cat`/`grep` ergonomic
  (Fella's tools are already close). Not a direction.

## Comparable tools we watch — `fx` v0.0.8 (2026-09-08)

`vercel-labs/fx` (an AI-agent CLI / `libfx`) shipped an almost entirely
*subtractive* release. Mapped to Fella:

**Validates**

- **fx removed its memory *tool*** (kept the saved memories). This is direct
  support for `feat/folder-memory`'s choice: memory is a deterministic
  prompt-injected block written from signals Fella already produces, **not a
  tool the model decides to call**. The deferred `recall()` tool is deferred
  for exactly the unresolved "does a weak model reach for it" reason. Keep the
  store (`memory.md`) decoupled from retrieval so `recall()`, if ever added, is
  cheap to remove. See #42, [`FOLDER-MEMORY.md`](FOLDER-MEMORY.md).
- **12 → 3 shell actions, 6 → 2 subagent commands.** Reinforces "smallest
  useful tool set" and the [vertical-not-horizontal](DECISIONS.md) principle.

**Actionable — issues #44–47**

- **Merge the inspect tools (#46).** `describe_schema` + `sample_rows` (and maybe
  `list_files`) → one `inspect_table(name)`. `prompt-ablation` already shows
  those calls are mostly `redundant_schema` waste; fewer inspect options = fewer
  wrong picks. Measure waste/accuracy in `agent_eval`.
- **Mid-run steer (#45).** fx: "Enter now steers active turns instead of queuing." In
  Fella `submit()` bails while `session.busy` — the only mid-run option is
  Stop → wait → retype. A message sent while busy should cancel + re-ask with
  the text appended. Removes a full round-trip from the correction loop (see the
  interaction-cost table in `PERFORMANCE-LOG.md`).
- **Publish latency percentiles + binary size (#47)** as tracked
  release metrics, alongside `agent_bench`'s loop timings. fx headlines "all 22
  TUI interactions <15 ms p95" and "-7.49% binary".
- **Reopen the last folder on start (#44)** — *shipped 2026-09-08.*
  `recent_workspaces` was written but never read; now the last folder reopens
  on launch.

**Deferred**

- **Compaction shape.** fx: keep recent tool exchanges verbatim + preserve the
  full transcript + continue the turn in a fresh window. More sophisticated
  than Fella's count-based `trim_history`. `folder-scale` says it's not needed
  yet; this is the reference if long multi-step runs ever flail.

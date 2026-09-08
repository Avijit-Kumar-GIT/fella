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
  `docs/PERFORMANCE.md`.

- **"Are we editing test cases to fit the output? We should change the code to
  fit the tests, not the tests to fit the code."**
  → Battery **frozen**. Grader changes need a behaviour trace + a ground-truth
  argument (rule in `PERFORMANCE.md` and in `grade()`'s comment). The 2026-09-07
  grader fixes each cite the trace.

- **"Diversify the cases; run more iterations; get a solid baseline *then*
  optimise the harness against it."**
  → 18 diverse cases (aggregate / filter / min-max / group / time-series /
  ratio / multi-step / trap / doc / empty / refusal / non-financial).
  Baseline: `PERFORMANCE.md` §"Baseline 2026-09-07". Then optimised —
  §"After 2026-09-07".

---

## Recording & process (2026-09-07)

- **"As long as we're model-agnostic I don't care too much — can we record
  these improvements somewhere?"**
  → `docs/HARNESS.md` (the running log + rationale), `PERFORMANCE.md` (numbers),
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
  the total) → **luna 0 % → 100 %** correct, the carried correction becomes the
  right `LOWER(cat) IN (…)` filter; gemma4 shifts right but botches the
  case-fold and stays wrong.

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

# Analytics release brief

This is the build brief for the next release that aims to make Fella feel like
a natural-language analytics platform for non-technical users. It turns the
existing product decisions into a concrete quality bar and a prioritized set
of UI, harness, and engine changes.

It does not reopen Fella's identity. The standing decisions still apply:
personal analytics, read-only, local-first, depth over reach, a small fixed
tool set, SQLite by default, and no general-purpose task agent. See
[`GOALS.md`](GOALS.md), [`PRINCIPLES.md`](PRINCIPLES.md),
[`NON-GOALS.md`](NON-GOALS.md), and [`DECISIONS.md`](DECISIONS.md).

## Product outcome

Fella should let someone point it at a folder, ask a question in ordinary
language, and receive a short analysis they can trust.

The chat box is the input mechanism, not the product identity. The product
identity is the **analysis result**: a considered answer, a useful visual when
one helps, clear caveats, and a visible path back to the data and calculations
behind it.

"Strong" means:

1. The relevant files, tables, columns, and time range were understood.
2. The calculation fits the question, including comparisons and trends.
3. Data quality issues, units, missing values, and category ambiguity were
   considered.
4. Every displayed number and visual is tied to actual computation.
5. The result was checked before it was shown.
6. The answer is concise and plain-language, without hiding uncertainty.

The normal answer should be understandable in a few seconds:

```text
Headline answer

Chart or compact table, only when it improves understanding

One or two supporting findings

Data note, if one matters

Verified against: source files and calculations
```

Do not force the model to emit a large rigid response schema. Keep its prose
natural and let the backend own the structured facts that need guarantees:
provenance, visual data, caveats, verification status, and workspace revision.

## Entry versus detail

The entry experience must remain minimal and calm. A new user should not land
on a dashboard, a catalog dump, a settings screen, raw SQL, or a wall of
diagnostics.

The default entry path shows only:

- What Fella is for, in one short sentence.
- One primary action to choose or reopen a folder.
- One clean natural-language question field once a folder is ready.
- A small folder/model readiness indication.
- A few catalog-aware example questions when useful.

Everything deeper is available but hidden by default. A secondary data view,
drawer, or sheet can contain the catalog, source mapping, columns, sample rows,
skipped files, ingestion notes, date coverage, and reindex action. The user
opens it deliberately from the folder control or a "View data" action.

The same rule applies to answers. The primary result stays clean; "How this
was worked out" reveals sources, plain-language calculation steps, caveats,
and verification. Raw SQL and raw tool arguments remain one level deeper for
people who want them.

This is not a second product surface. It is progressive disclosure: a quiet
default for everyone, with depth available when trust or curiosity requires it.

## Current foundation

The current architecture already has the right bones:

- `engine/analytics/` is separate from the model loop, Tauri, secrets, and
  conversation state.
- SQLite is the small default data engine behind `DataEngine`.
- SQL is read-only and has a watchdog timeout.
- Python has bounded execution and is explicitly documented as a guard rail,
  not a hostile-code sandbox.
- The harness has cancellation, a step cap, duplicate-call caching, concurrent
  independent tool calls, evidence capture, and a final verification pass.
- `depth_rule` and `aside_rule` improve comparison and trend questions without
  turning the loop into a multi-agent framework.
- The evaluation process uses frozen cases and real A/B evidence rather than
  prompt intuition.

The next release should make these guarantees visible and close the gaps at
the boundaries between data understanding, computation, verification, and
presentation.

## Progress

The first vertical slice is complete:

- `make_chart` now accepts a guarded read-only SQL query instead of
  model-supplied label/value arrays.
- `analytics::chart::from_query()` converts the bounded query result into
  validated chart data without depending on `EngineState` or the UI.
- Chart evidence retains the SQL, columns, rows, and row count used to create
  the visual.
- New evidence items receive stable ordered IDs used by the frontend for
  evidence and chart identity; older archived answers remain readable.
- Answers bind to the catalog revision captured at run start, and SQL evidence
  maps referenced tables back to source files or workbook sheets.
- Answers carry one typed verification status (`verified`, `needs_review`,
  `insufficient_data`, or `failed`) shared by the UI, memory recorder, and
  evaluation harness; old archived answers use a frontend fallback.
- Workspace replacement now builds the catalog and data engine off to the side,
  publishes them behind one workspace lock, and keeps SQLite scratch files alive
  while an in-flight Python calculation still needs them. Tool results are
  rejected when a reindex changes the captured workspace revision, and the
  answer receives a review check if the workspace changes before verification.
- Charts expose a collapsed exact-values table in addition to the visual.
- The agent prompt, chart tests, and Mintlify engine/tool/verification pages
  describe the new contract.

The remaining chart work is presentation: make source and verification state
more visible in the result-card hierarchy without making the default entry
noisy.

## Must ship: backend and harness

### 1. Make provenance a first-class contract

`EvidenceItem` now carries SQL-backed source mappings and stable IDs, while the
answer carries the workspace snapshot and typed verification status. The
remaining structured metadata is:

- Referenced tables and columns.

The frontend should never need to duplicate Rust's hard-failure string list.
One serialized status should drive the badge, warning, evidence fold, and
evaluation result.

### 2. Make charts query-derived

`make_chart` accepts a read-only SQL query. The first result column is the
label/date and the remaining one or two columns are numeric series. The chart
is created from that bounded query result rather than model-supplied numeric
arrays.

The backend validates the query result, derives the chart data, retains the SQL
and result rows, and sends the query through the normal verification rerun
path. A chart must not be able to bypass the rule that every number has a
receipt.

### 3. Close the semantic-filter gap

Arithmetic correctness is not enough if the query selects the wrong rows. The
known live failures are uniform-case literals such as `Leisure` versus stored
`leisure`, and genuinely different labels such as `HOUSING` and `mortgage`.

Improve the existing inspection and query feedback rather than adding a new
agent. The first narrow slice is now in the working tree: likely label filters
get case-folding guidance before the query result, and an end-to-end test covers
`purpose = 'Leisure'` against stored lowercase `leisure`.

- [ ] Show common values for low-cardinality text columns.
- [ ] Detect zero-result text filters and report case-insensitive or near-match
      candidates.
- [x] Extend filter warnings beyond columns with detected case collisions.
- Preserve ambiguity instead of silently rewriting user intent.
- Make `fella.md` and learned vocabulary notes easy for the model to apply.
- [ ] Add frozen evaluation cases for case, aliases, empty results, and
      ambiguous categories.

The backend should help the model ask or infer the right question, not pretend
that semantic aliases can always be resolved automatically.

### 4. Make the workspace an atomic snapshot

Implemented. `open_workspace()` and `reindex()` build a new catalog and data
engine off to the side, then publish both through one workspace state lock. A
question sees either the old complete snapshot or the new complete snapshot,
never a partially rebuilt one. SQLite uses a private scratch directory per
replacement, and a short lease keeps that directory available to an in-flight
Python calculation.

Runs and answers are bound to the workspace revision. Tool calls check that
revision before and after execution; a change discards the result rather than
allowing old provenance to describe new data. On folder changes:

- Let active work finish safely; discard any tool result that crosses the
  revision boundary and mark the answer for review.
- Do not mix transcripts from the old folder with the new folder.
- Start a new conversation or explicitly archive the old one first.
- Make archived follow-up context and the active folder agree.

Show the last-indexed state so users know whether an answer reflects current
files.

### 5. Make supported statistics replayable

Median, standard deviation, correlation, and regression currently use the
Python escape hatch. SQL verification does not replay arbitrary Python.

For the supported statistical set, prefer deterministic backend or SQL
primitives that can be rerun. Keep Python for calculations that genuinely need
it, but distinguish those results if they cannot receive the same verification
guarantee as SQL.

### 6. Keep backend behavior consistent

The optional DuckDB backend currently differs from SQLite in query timeouts,
ingest limits, numeric-text coercion, and test coverage. Bring those behaviors
to parity or keep DuckDB explicitly experimental outside the supported release
contract. Do not make the default engine heavier just to solve this.

### 7. Preserve the small harness

Do not add sub-agents, a general planner, RAG, web search, or more base tools
for this release. Improve the existing linear loop through better tool
feedback, data profiles, verification, and evaluation. The existing
`depth_rule`, parallel independent calls, and bounded loop are sufficient for
the intended 2-4 query analytical patterns.

## Must ship: UI and UX

### 1. Add a hidden-by-default data view

Keep the initial screen sparse. The folder chip or a small "View data" action
can open a secondary view containing:

- Usable and skipped files.
- File-to-table and sheet mappings.
- Row counts and date coverage.
- Column types and common values.
- Ingestion caveats.
- Reindex and freshness information.

Do not put this information into the main transcript by default. The current
short system message after `openFolder()` is not enough for trust, but a full
catalog dump would make the entry experience noisy.

### 2. Present answers as result cards

Keep the transcript for continuity and history, but give each completed answer
this visual hierarchy:

- User question.
- Short headline result.
- Chart or compact table when useful.
- One or two supporting findings.
- Data caveat when relevant.
- Verified, needs review, insufficient data, or failed status.
- Source and freshness line.
- "How this was worked out" disclosure.
- Raw SQL and arguments only in the deepest detail level.

The result should feel like a small, finished analytical artifact without
becoming a dashboard or a generated file.

### 3. Use neutral analysis progress

Replace model-flavored progress as the primary status with short stages such
as "Checking the data", "Calculating", and "Checking the result". Keep
model/tool detail inside the optional explanation view.

### 4. Make visuals trustworthy and accessible

Charts need source provenance, units, exact-value alternatives, accessible
labels, and readable axes. A line chart must not be the only place a value can
be understood. Keep the existing bar/line scope; do not add chart types before
the trust contract is complete.

### 5. Make data caveats visible at the right time

Surface relevant notes such as text amounts, dropped totals rows, missing
periods, or skipped files near the result. Do not bury material caveats only
inside raw evidence.

### 6. Fix workspace and history boundaries

Folder switching, reindexing, archived conversations, and follow-up questions
must not silently change the dataset behind a visible transcript. The history
sidebar should show the source workspace clearly, and a restored conversation
should retain enough analytical context for natural follow-ups.

### 7. Improve natural-language discovery without adding clutter

Use the actual catalog to suggest a small number of questions about the open
folder. Keep slash commands as a power-user escape hatch, not the main entry
path. Remove terminal-roleplay details such as the shell prompt prefix from
ordinary user messages.

### 8. Keep the quiet quality details

Add a prominent plain-language warning for every backend hard failure, fix
keyboard focus for sidebar actions, and give line charts a table/list
alternative. Add frontend regression coverage for result status, chart
rendering, source display, and folder switching.

## Evaluation and release gate

The next release is ready when:

- Every displayed number and chart maps to a source, query/result, and
  workspace revision, or the result is visibly marked as needing review.
- A reindex or folder switch cannot expose a partial or mislabeled workspace.
- Cases cover multiple yearly files, text amounts, dates, joins, case and
  semantic labels, empty results, trends, correlation, charts, and follow-ups.
- The grader checks complete findings and tables, not only whether one headline
  number appears somewhere in the prose.
- The frozen battery still has no regression across the supported model floor,
  a mid-tier model, and a frontier model.
- Hosted BYOK performance is measured for the supported deployment path.
- The full Rust suite, frontend checks, release build, and GUI smoke checklist
  are green.
- A non-technical user can understand the headline, the important caveat, and
  how to inspect the work without reading SQL.

The existing performance target remains: simple questions should normally take
one or two model round trips, and deeper questions should stay bounded rather
than becoming open-ended research sessions.

## Explicitly out of scope

Do not use this release to add:

- A general-purpose task agent or write/move/delete tools.
- Multi-agent orchestration or a separate critic model.
- RAG or local embeddings for documents.
- Web search, exports, or generated result files.
- Hypothesis testing, p-values, or a larger statistics dependency.
- A dashboard builder or a settings-heavy workspace.
- DuckDB as the default engine.

## Implementation order

1. Define the result/provenance contract and atomic workspace revision.
2. Make chart output query-derived and strengthen verification semantics.
3. Add semantic filter feedback, richer profiling, and replayable standard
   statistics.
4. Build the hidden data view and result-card presentation around the contract.
5. Expand evaluation, measure local behavior, and close the release smoke gate.

## Working references

- Product identity: `docs/WHY.md`, `docs/PRINCIPLES.md`, `docs/GOALS.md`.
- Scope decisions: `docs/NON-GOALS.md`, `docs/DECISIONS.md`.
- Engine and harness: `docs/ARCHITECTURE.md`, `docs/HARNESS.md`.
- Existing candidates: `docs/ROADMAP.md`.
- UI rules: `docs/DESIGN.md`.
- Release and security: `docs/RELEASE.md`, `docs/SECURITY-REVIEW-v0.1.md`.
- Backend seams: `src-tauri/src/engine/analytics/`, `agent.rs`, `state.rs`,
  `tools.rs`, and `evidence.rs`.
- Frontend seams: `src/lib/components/`, `commands.ts`, `session.svelte.ts`,
  `types.ts`, and `verify.ts`.

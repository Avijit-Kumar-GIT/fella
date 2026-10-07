# Fella Analytical Computer Roadmap

This is the implementation roadmap for turning Fella's current analytics
harness into a revision-aware analytical computer. It is intentionally a
roadmap for the shared Rust runtime, not a UI redesign or a replacement for
the Electron shell.

The broader user-facing backlog—including general questions, web research,
forecasting, and expanded visual analysis—is tracked in the
[Product Roadmap](PRODUCT-ROADMAP.md). This document remains the detailed
runtime and workspace-model implementation plan.

The destination is ambitious. The delivery should remain incremental: every
stage must improve the model-directed loop, preserve the read-only boundary,
and keep the model free to use the route that best fits the question.

## The target

Fella should turn a question about a mounted folder into a bounded, checked
analytical result:

```text
workspace files
  -> WorkspaceModel (sources, profiles, definitions, revision)
  -> ConversationSession
  -> model control plane: interpret, decompose, observe, choose
  -> AnalysisContract when interpretation requires a non-literal mapping or material scope choice
  -> grounding and targeted probes when useful
  -> compiled or model-authored read-only execution
  -> ExecutionTrace
  -> semantic + deterministic verification
  -> accepted result, clarification, retry, or review
  -> scoped memory and evaluation trace
```

The model drives meaning, plans, tool calls, and explanations. The runtime owns
scope, permissions, budgets, read-only execution, verification, and memory
promotion. The data is authoritative for observed values. The user resolves
ambiguity that observations cannot resolve by themselves.

This is the implementation form of Fella's core loop:

> The model drives. The runtime constrains. The data computes. The verifier
> checks. The user confirms. Memory compounds.

## The analyst workflow

The harness should reproduce how an analyst works through an unfamiliar
question and folder:

1. **Reconnaissance:** mount locally, map the files and their structure, and
   surface useful profiles and caveats without requiring clean inputs or
   silently deciding what every field means.
2. **Frame the question:** interpret the request and decompose it into smaller
   questions when that helps identify the required evidence.
3. **Investigate:** inspect the relevant sources, labels, samples, notes, date
   conventions, and candidate relationships. Use observations to refine the
   interpretation; repeat inspection when evidence is incomplete or surprising.
4. **Clarify when it matters:** ask only after reasonable investigation leaves
   a user-owned ambiguity that could change the result. Pending clarification
   does not disable safe analysis: when useful, compute and label candidate or
   partial results so the user can see what the choice changes, without
   presenting one unresolved scenario as settled.
5. **Analyze and validate:** execute the grounded plan read-only, check the
   result against its source rows and stated assumptions, then either revise
   the investigation or answer with traceable evidence.

This is model-directed, not a fixed sequence of tool calls. The loop does,
however, preserve a real observation boundary: if a model response requests
inspection and computation together, Fella returns the inspection first and
defers computation until the model has seen it. Otherwise concurrent tool
execution lets a plan run before the evidence it requested can influence that
plan. A direct computation remains appropriate when the source and meaning are
already clear and no new observation is requested. The model is assumed
capable of analysis when Fella gives it relevant context, useful tools, and
feedback; failures are presumed to be harness-side until controlled
comparison shows otherwise.

## Delivery status

- **M0 — runtime spine:** implemented in the typed `runtime` module and the
  shared answer protocol between the Rust runtime and Electron bridge. Recoverable provider/tool failures and
  typed, target-scoped verification repairs now emit the explicit `retry`
  lifecycle state instead of leaving the shells to infer recovery from raw errors; a
  mid-turn workspace revision change now stops stale tool retries and preserves
  the partial trace for an explicit rerun.
- **M1 — WorkspaceModel:** first slice implemented; the mounted catalog now
  has a revision-bound semantic projection with cautious field-role hints and
  an inspectable cross-shell command. A compact profile now also feeds the
  shared bounded context packet. It now includes cautious identifier-based
  relationship candidates as prompt-visible join hints; richer user-defined
  definitions and value semantics are still ahead.
- **M2 — model-driven interpretation:** first slice implemented; the model has
  the complete fixed read-only tool surface plus a compact contract function
  for non-literal semantic mappings and material scope choices (simple exact
  lookups can use direct tools). Rust normalizes and grounds a proposed hypothesis, but an
  ambiguous, unsupported, invalid, or absent hypothesis never blocks direct
  investigation. Semantic roll-ups record their selected observed labels and
  assumption before calculation, then disclose that mapping in the answer. The prompt now describes the analyst workflow without
  requiring a contract before source inspection or direct tools. A proposed
  clarification leaves the read-only tool surface available: the model may
  inspect or compute labeled candidate/partial results before asking the user,
  while the unresolved choice remains visibly pending. Risk signals tune attention and verification rather than selecting
  a mandatory route.
- **M3 — grounding:** first slice implemented alongside model-directed routing;
  exact field bindings and bounded filter-value probes now run against the
  current workspace revision. The first deterministic planner slice now
  compiles grounded single-source aggregates, filters, time ranges, typed
  year/month/week/day buckets, groupings, grounded top-N/ranking, and guarded
  ratios over declared measures into read-only SQL. Explicit join edges are
  now revision-grounded, cardinality-probed, and compilable when the join graph
  is connected and every reference is unambiguous. Typed single-source period comparisons now
  ground both explicit windows and compile current, previous, absolute-change,
  and guarded percent-change columns, including guarded single-source derived
  metrics; aligned month/week/day buckets are also supported, while joins and
  non-alignable year buckets remain model-selected direct-tool cases.
- **M4 — semantic verification:** first slice implemented; grounded bindings
  must be present in executed SQL, requested aggregate operations and observed
  filter values, time buckets, ranking limits, derived ratios, and typed
  comparison windows must also survive into that evidence. Period comparison
  change and percent-change columns are now reconciled against returned rows;
  declared ratio outputs are now reconciled against their returned numerator
  and denominator values, including zero-denominator null behavior;
  additive grouped results are now reconciled against an independent
  ungrouped deterministic plan (limited rankings and non-additive measures
  remain excluded); ratio outputs now flag nested/separate SQL scopes where
  their numerator and denominator populations cannot be established as shared;
  grouped and ungrouped averages are now checked against independent min/max
  bounds under the same grounded scope; explicit cross-population and
  chart payloads are now re-projected from their stored source rows during
  verification so mutated labels or values produce artifact-scoped withhold
  findings; richer
  chart-shape semantics remain ahead, and
  unresolved, ambiguous, and unsupported hypotheses remain visible in the
  trace, but they do not veto a separately grounded direct computation. Numeric
  claim support now uses returned cells rather than SQL execution metadata, and
  any unbacked number can trigger one bounded tool-backed revise-or-retract pass.
- **M5 — versioned semantic memory:** first slice implemented; corrections and
  verified field bindings can carry authority, workspace revision, supporting
  turn, evidence IDs, and explicit supersession/conflict state. The typed
  ledger projects into readable `memory.md` without making model suggestions
  prompt authority.
- **M6 — canonical persistence:** first slice implemented; every completed ask
  persists a typed backend-owned turn record with its optional hypothesis,
  model-directed plan, execution trace, verification report, and bounded
  result/evidence. The
  same record can be loaded or rerun through the Electron bridge;
  reruns are mount-checked and linked to their source turn. A compact catalog
  snapshot and shared replay-status command now explain revision drift and
  source-level changes before a rerun; the expanded evidence view now exposes
  that status and an explicit rerun action. A richer trace inspector remains
  ahead. The context picker now travels as structured, persisted starting-point
  references instead of being flattened into the user's question; a single
  selected source also scopes contract grounding when the model leaves the
  source implicit. Reruns preserve the same analytical hints without changing
  the canonical question.
- **M7 — evaluation, replay, and optimization:** first context-assembly slice
  implemented; the shared Rust runtime now applies deterministic, question-aware
  budgets to schema, user definitions, folder memory, and conversation history
  before either desktop shell sends a prompt. The evaluator now preserves the
  runtime's typed acceptance status in JSON/CSV output, so correctness and
  trust can be measured separately. External cases can also declare expected
  interpretation and plan semantics, which the evaluator scores independently
  from the final prose answer. It now also reports accepted-answer and
  unsafe-guess rates for Fella runs. Cost/quality dashboards and broader replay
  matrices remain ahead; the lift rollup now carries weighted accepted and
  unsafe rates alongside accuracy and token cost. Fella benchmark records also
  retain canonical analysis-turn references for inspection and rerun. The
  analyst loop now retains a bounded working set of source-document evidence
  and the latest semantic frame across tool rounds, instead of evicting them
  solely by age; derived arithmetic and candidate-threshold decisions also
  have explicit model instructions and are exercised through focused agent
  regressions plus selected mixed-file model replays. Evaluation reports useful
  reconnaissance, tool errors, and unreferenced SQL-result signals separately;
  only exact duplicate calls count as confirmed redundancy.

## Current implementation baseline

M0–M7 now have implemented first slices. The status above is the current
implementation snapshot; the remaining work is incremental:

- M1: richer user-authored definitions and value semantics.
- M4: broader chart-shape and analytical-invariant coverage.
- M6: a richer trace inspector over the canonical persisted turn.
- M7: cost/quality dashboards and broader replay matrices.

The per-milestone notes above are the source of truth for what exists and what
is still partial. New gaps should be added there only when they represent a
general capability or quality boundary, not an individual fixture.

## Delivery principles

1. **Build the runtime spine before the semantic features.** Every future
   feature should attach to the same turn, trace, and revision objects.
2. **Keep one model-directed loop.** The model decides whether the next move is
   inspection, a hypothesis, SQL, Python, document search, a chart, or a
   clarification. The runtime does not force a pre-tool ceremony.
3. **Use AI where it provides leverage; constrain consequences in Rust.** The
   model can choose any enabled read-only route, but cannot bypass catalog
   scope, execution budgets, or verification.
4. **Treat ambiguity as a state.** Do not turn an unresolved interpretation
   into a confident SQL query merely because the query runs.
5. **Keep every analytical path first-class.** SQL, Python, documents, and
   charts remain available as model-selected capabilities, with their actual
   verification level made explicit.
6. **Make the backend canonical.** Electron consumes the Rust runtime protocol;
   the Svelte UI should project runtime state rather than assemble its own
   interpretation of a turn.
7. **Measure every quality trade-off.** A semantic feature must be evaluated
   for correctness, clarification quality, latency, token use, and regressions
   across the supported model ladder.

## Milestones and reviewable PRs

### M0 — Runtime spine and typed turn protocol

Create the shared domain types and lifecycle without changing the normal answer
path:

- `WorkspaceModel` boundary around the current catalog/revision;
- `AnalysisTurn` and explicit states;
- `AnalysisContract` and `LogicalPlan` placeholders with stable serialization;
- `ExecutionTrace` and `VerificationReport` envelopes;
- turn IDs and workspace revision on emitted events and results;
- the typed event protocol through the Electron stdio bridge.

The first implementation may populate a minimal contract and trace from the
existing direct loop. The important result is that later work has a stable
attachment point instead of adding more state to `agent.rs`.

**Exit criteria:** existing answers and tests are unchanged in behavior;
every run can be identified, tied to a workspace revision, and represented as
a turn with a trace.

### M1 — Revision-bound WorkspaceModel

Promote the current catalog into a semantic workspace profile. Start with
deterministic facts derived from files:

- column profiles and common values;
- date fields and coverage;
- likely measures and dimensions;
- units, sign conventions, nulls, sentinels, and ingestion caveats;
- candidate identifiers and relationships;
- source-level confidence and revision.

Cache profiles by workspace revision. Keep `fella.md` as the user-authored
definition surface and keep inferred facts visibly separate from user facts.

**Exit criteria:** a workspace can expose a compact, inspectable model without
asking the model to rediscover basic data shape on every question.

### M2 — Model-directed semantic interpretation

Introduce a compact `AnalysisContract` for the common analytical core:

- population and grain;
- measures and operations;
- filters and resolved/candidate values;
- time field, range, and timezone;
- grouping and comparison;
- requested presentation;
- assumptions and unresolved ambiguity.

Add attention signals:

```text
simple lookup               -> one direct observation
trend / ratio / join        -> whatever decomposition the model needs
ambiguous value or field    -> inspect, probe, or propose a hypothesis
material disagreement      -> retry, revise, or clarify
unsupported request        -> explain the observed limitation
```

The contract function is optional overall and provider-neutral, but the model
should use it for non-literal mappings and material scope choices. Providers
that support structured output can use it; others can emit the compact schema
through the ordinary tool loop. A malformed or unresolved contract is a repair
signal, not a reason to deny the model access to the other read-only tools.

**Exit criteria:** the system can distinguish “the model wrote valid SQL” from
“the model understood the question,” and it does not silently choose between
materially different interpretations.

### M3 — Grounding, probes, and logical compilation

Add internal bounded probes for candidate values, date coverage, units, nulls,
zero-result filters, and join cardinality. Then compile grounded contracts
into a typed `LogicalPlan` and deterministic SQL for the common core:

- filter and aggregate;
- group and rank;
- date buckets;
- comparisons and ratios;
- declared joins;
- chart result shapes.

Model-generated SQL and Python remain first-class model-directed routes. The
compiler owns physical table names, quoted identifiers, date functions, null
behavior, and denominator definitions when the model selects the typed plan.

The first implementation slice is intentionally narrower than the destination:
it only compiles a grounded single queryable source with aggregate measures,
observed filter values, ISO-like year/month/date ranges, group-by fields,
ratios whose operands are declared measures, and explicit join edges whose
sources and keys are grounded against the current snapshot. Connected joins
with qualified, unambiguous references now compile into read-only SQL; direct
model tools remain available for disconnected graphs and unsupported
  comparison semantics. A first typed comparison slice also supports one-source
period-over-period contracts with explicit current and previous ranges, bounded
range probes, and deterministic change columns. It now also supports guarded
single-source derived metrics inside the comparison, plus aligned
month/week/day buckets; joins and non-alignable year buckets remain on the
direct-tool route. A semantic hypothesis is recorded alongside execution,
  and the model can continue with direct tools when compilation is not useful.

**Exit criteria:** common questions can use a validated semantic plan, while
the model can still investigate and answer questions the compiler does not
cover.

### M4 — Semantic verification and acceptance gates

Keep the current deterministic verifier and add contract-aware checks:

- requested values exist in the current snapshot;
- requested date ranges are represented;
- all requested comparisons are present;
- grouped totals reconcile with overall totals;
- ratios have compatible numerator/denominator populations;
- averages remain within observed bounds;
- percentages reconcile approximately;
- chart values match their source query;
- the executed plan satisfies the contract.

Verification must control the runtime outcome:

```text
clean                 -> accepted
recoverable execution -> retry
unresolved ambiguity  -> clarify
unproven result       -> needs review
unsupported           -> explain limitation
```

**Exit criteria:** `needs_review` is a real runtime state, not only a badge
shown after an answer has already been accepted.

### M5 — Versioned semantic memory

Evolve `memory.md` from a useful local note file into a human-readable
projection of typed semantic facts. Each fact should carry:

- workspace scope;
- authority (`user`, `observed`, `model_suggestion`, `confirmed`);
- workspace revision;
- supporting turn and evidence;
- creation/update time;
- supersession and conflict state.

User corrections and explicit confirmations should be the strongest promotion
path. A model guess must not silently become a durable rule. Raw episodes remain
primarily an evaluation source, not prompt memory.

**Exit criteria:** Fella can consistently reuse a confirmed workspace
definition while detecting and surfacing conflicts after the data changes.

### M6 — Canonical persistence and product projections

Persist the backend-owned analytical record:

```text
AnalysisTurn
  question
  workspace revision
  contract
  logical plan
  execution trace
  verification report
  result
  memory events
```

The first slice persists and loads this record after every completed turn. A
restart can restore the bounded analytical context from the archived transcript,
and an explicit rerun executes the stored question against the currently
mounted copy of the same workspace while preserving rerun lineage. The stored
record now also carries compact source metadata, and the shared
analysis_turn_replay_status command can explain whether the current mount is
the same revision and which sources changed. User-facing inspect/rerun controls
remain follow-up work in this milestone.

The UI then becomes a projection:

- answer first;
- only relevant assumptions and caveats inline;
- trace/evidence progressively disclosed;
- raw SQL and rows available on demand;
- Context becomes definitions and scoped references, not a second prompt editor;
- Projects become summaries of workspace knowledge, not another source of truth.

**Exit criteria:** the Electron UI shows the Rust runtime result and a past
turn can be inspected or rerun against a newer workspace revision.

### M7 — Evaluation, replay, and optimization

Extend the existing benchmark system to score:

- interpretation/contract correctness;
- result correctness;
- unsafe-guess rate;
- clarification precision and usefulness;
- plan validity;
- verification catch rate;
- consistency across paraphrases;
- consistency across workspace revisions;
- memory carryover accuracy;
- time, tokens, and tool calls per accepted answer.

Every semantic rule should ship with a fixture and a replayable trace. The
runtime should make it possible to compare a prompt, model, planner, or
compiler change against the same analytical turns.

The first implementation slice is the shared `ContextAssembler`. It keeps
ordinary context unchanged, but prevents unusually large user guides, schemas,
memory ledgers, or transcripts from consuming the entire model budget. It
selects relevant lines for definitions and schemas, keeps the newest session
turns, and makes omissions explicit so the model can continue with the
read-only tools. This applies the minimal-relevant-context principle described by
[CHESS: Contextual Harnessing for Efficient SQL](https://arxiv.org/abs/2405.16755)
without introducing a vector store into the local-first base product. The
evaluation harness now carries the runtime's acceptance status plus optional
expected interpretation/plan labels into JSON, CSV, and rollup artifacts, so a
benchmark can distinguish a numerically correct answer from one produced by an
unsafe interpretation or unexpected execution strategy. It also records the
fraction of runs the runtime accepted as verified and the fraction that still
emitted a non-empty answer without verification, making the correctness-first
tradeoff visible before a dashboard exists. It also reports verification-catch
rate: among incorrect Fella runs, how often the verifier refused to accept the
result instead of incorrectly marking it verified. The aggregate CLI keeps
those rates weighted by iteration count and leaves non-Fella baselines as not
applicable.
Each Fella result also carries the persisted turn id and workspace revision,
connecting evaluation output to the existing `analysis_turn_load` and
`analysis_turn_rerun` protocol instead of inventing a second trace format.

## Research-informed design choices

These papers inform specific mechanisms; they do not define Fella's product or
justify importing their full systems.

- **Semantic intermediate representation and deterministic compilation.**
  The semantic-layer-mediated NL-to-SQL work separates semantic intent from
  physical SQL with a compact intermediate representation and a deterministic
  dialect compiler. Fella adopts that shape as `AnalysisContract` →
  `LogicalPlan` → SQL for its common analytical core, while retaining direct
  model-selected tools for questions outside that core. See [Kim, Khoeurn, and Yoon, *A
  Semantic-Layer-Mediated Agent for Natural Language to SQL over Heterogeneous
  Enterprise Databases*](https://arxiv.org/abs/2606.31041).

- **Execution as grounding and memory formation.** GATE keeps grounding
  hypotheses open, executes the parts that are already grounded, and stores
  only execution-supported resolutions. Fella adopts this as a rule for
  semantic memory: observed execution can support a candidate, but user
  confirmation remains stronger than a model guess. See [Lee, Kim, and Hwang,
  *Bootstrapping Semantic Layer from Execution for Text-to-SQL*](https://arxiv.org/abs/2606.05634).

- **Targeted ambiguity probes.** SOMA-SQL uses an ambiguity taxonomy and
  disagreement-driven probing instead of asking the model to guess through
  underspecified schemas. Fella adopts bounded probes for values, dates, units,
  and joins; unlike the paper's autonomous setting, Fella may ask the user when
  the remaining interpretations would materially change the answer. See
  [Somayajula et al., *SOMA-SQL: Resolving Multi-Source Ambiguity in NL-to-SQL
  via Synthetic Log and Execution Probing*](https://arxiv.org/abs/2606.11424).

- **Focused clarification.** AmbiSQL treats ambiguity detection and
  multiple-choice clarification as part of the text-to-SQL system rather than
  an exception after failure. Fella adopts the focused clarification pattern,
  but keeps it tied to the local workspace and only asks when the answer would
  change materially. See [Ding, Lin, and Zeng, *AmbiSQL: Interactive Ambiguity
  Detection and Resolution for Text-to-SQL*](https://arxiv.org/abs/2508.15276).

- **Minimal relevant context.** CHESS selects relevant schema/value context
  before query generation and revises candidate SQL using execution results.
  Fella adopts the principle of context selection and execution-aware revision,
  but uses a deterministic local workspace profile instead of requiring a
  vector database in the base product. See [CHESS: Contextual Harnessing for
  Efficient SQL](https://arxiv.org/abs/2405.16755).

## Quality gates for the whole roadmap

The framework is working when:

- simple questions remain fast and cheap;
- common analytical questions use a validated contract and plan;
- ambiguous questions clarify instead of confidently guessing;
- a query that merely runs is not treated as an answer that is semantically correct;
- accepted results are tied to a workspace revision and trace;
- user-confirmed definitions improve later questions without leaking across workspaces;
- Electron consumes the canonical Rust runtime protocol;
- every new quality feature is covered by a replayable evaluation case.

## Explicit non-goals

This roadmap does not require a multi-agent graph, a hosted memory service, a
vector database, a plugin marketplace, write-capable tools, or a dashboard
platform. The ambition is analytical depth and reliability, not general
autonomy or feature breadth.

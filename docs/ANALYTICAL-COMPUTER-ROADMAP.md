# Fella Analytical Computer Roadmap

This is the implementation roadmap for turning Fella's current analytics
harness into a revision-aware analytical computer. It is intentionally a
roadmap for the shared Rust runtime, not a UI redesign or a replacement for
the Tauri/Electron shell.

The destination is ambitious. The delivery should remain incremental: every
stage must improve the existing direct path, preserve the read-only boundary,
and remain useful if the structured path is not yet able to answer a question.

## The target

Fella should turn a question about a mounted folder into a bounded, checked
analytical result:

```text
workspace files
  -> WorkspaceModel (sources, profiles, definitions, revision)
  -> ConversationSession
  -> question risk gate
  -> semantic interpretation
  -> AnalysisContract
  -> grounding and targeted probes
  -> LogicalPlan
  -> bounded read-only execution
  -> ExecutionTrace
  -> semantic + deterministic verification
  -> accepted result, clarification, retry, or review
  -> scoped memory and evaluation trace
```

The model is allowed to propose meaning, plans, and explanations. The runtime
owns scope, permissions, execution, verification, and memory promotion. The
data is authoritative for observed values. The user resolves ambiguity that
the data cannot resolve by itself.

This is the implementation form of Fella's core loop:

> The model proposes. The runtime grounds. The data computes. The verifier
> judges. The user confirms. Memory compounds.

## Delivery status

- **M0 — runtime spine:** implemented in the typed `runtime` module and the
  shared Tauri/Electron answer protocol.
- **M1 — WorkspaceModel:** first slice implemented; the mounted catalog now
  has a revision-bound semantic projection with cautious field-role hints and
  an inspectable cross-shell command. A compact profile now also feeds the
  shared bounded context packet; richer definitions and relationship inference
  are still ahead.
- **M2 — semantic routing:** first slice implemented; elevated questions can
  negotiate a compact contract, and Rust preserves ambiguity instead of
  treating a model proposal as verified meaning.
- **M3 — grounding:** first slice implemented alongside contract routing;
  exact field bindings and bounded filter-value probes now run against the
  current workspace revision. The first deterministic planner slice now
  compiles grounded single-source aggregates, filters, time ranges, typed
  year/month/week/day buckets, groupings, grounded top-N/ranking, and guarded
  ratios over declared measures into read-only SQL. Explicit join edges are
  now revision-grounded, cardinality-probed, and compilable when the join graph
  is connected and every reference is unambiguous; disconnected/ambiguous joins
  still use the direct fallback. Typed single-source period comparisons now
  ground both explicit windows and compile current, previous, absolute-change,
  and guarded percent-change columns; joins plus derived-metric comparisons
  remain fallback cases.
- **M4 — semantic verification:** first slice implemented; grounded bindings
  must be present in executed SQL, requested aggregate operations and observed
  filter values, time buckets, ranking limits, derived ratios, and typed
  comparison windows must also survive into that evidence. Period comparison
  change and percent-change columns are now reconciled against returned rows;
  declared ratio outputs are now reconciled against their returned numerator
  and denominator values, including zero-denominator null behavior;
  additive grouped results are now reconciled against an independent
  ungrouped deterministic plan (limited rankings and non-additive measures
  remain excluded); broader ratio-population, average-bound, and chart-shape
  invariants remain ahead, and
  ambiguous/unsupported contracts remain review states even when a query is
  numerically reproducible.
- **M5 — versioned semantic memory:** first slice implemented; corrections and
  verified field bindings can carry authority, workspace revision, supporting
  turn, evidence IDs, and explicit supersession/conflict state. The typed
  ledger projects into readable `memory.md` without making model suggestions
  prompt authority.
- **M6 — canonical persistence:** first slice implemented; every completed ask
  persists a typed backend-owned turn record with its contract, direct-tool
  plan, execution trace, verification report, and bounded result/evidence. The
  same record can be loaded or rerun through the Tauri and Electron bridges;
  reruns are mount-checked and linked to their source turn.
- **M7 — evaluation, replay, and optimization:** first context-assembly slice
  implemented; the shared Rust runtime now applies deterministic, question-aware
  budgets to schema, user definitions, folder memory, and conversation history
  before either desktop shell sends a prompt. Scored interpretation/replay
  matrices and cost/quality dashboards remain ahead.

## Where the current code starts

Fella already has a strong analytical data plane:

| Existing capability | Current implementation |
|---|---|
| Workspace boundary and revision | `catalog.rs`, `EngineState::open_workspace`, `workspace_revision` |
| Local ingestion and computation | `analytics/data`, SQLite by default, DuckDB opt-in |
| Bounded non-SQL statistics | `analytics/pyexec.rs`, RustPython/WASM limits |
| Fixed read-only surface | `tools.rs`, seven built-in tools, no write tool |
| Conversation loop | `agent.rs`, bounded steps, cancellation, parallel calls |
| Provenance and evidence | `evidence.rs`, `analytics/provenance.rs`, `EvidenceItem` |
| Canonical turn record | `runtime.rs`, `analysis_store.rs`, `analysis/turns/*.json` |
| Deterministic verification | `analytics/verify.rs`, query reruns and answer checks |
| Workspace context | root `fella.md`, loaded into the prompt |
| Learned context | `memory.rs`, `semantic_memory.rs`, local `memory.md`, typed fact ledger, and episode log |
| Evaluation | `bench/`, `examples/agent_eval`, Rust integration tests |

The missing layer is not “more tools.” It is a typed control plane around
these capabilities:

| Target object | Current analogue | Gap |
|---|---|---|
| `WorkspaceModel` | `Catalog` + schema prompt + context/memory files | No first-class semantic profile, authority, or definition registry |
| `ConversationSession` | frontend tabs + bounded server session memory | No backend-owned analytical session record |
| `AnalysisTurn` | `agent::run` + `AskEvent` | Lifecycle exists implicitly, not as a typed state machine |
| `AnalysisContract` | prompt rules such as `depth_rule` | No structured representation of population, grain, filters, measures, or assumptions |
| `LogicalPlan` | model-generated SQL/Python calls | No validated semantic plan before physical execution |
| `ExecutionTrace` | `EvidenceItem` + frontend `RunStep` | Evidence is a result projection, not the canonical runtime trace |
| `VerificationReport` | `Vec<VerificationCheck>` | Strong deterministic checks, but limited contract-aware invariants |
| `SemanticMemory` | `FolderMemory` | Facts lack explicit authority, evidence, revision, and conflict state |
| `AnalysisResult` | `Answer` | No unified contract/trace/assumption result envelope |

## Delivery principles

1. **Build the runtime spine before the semantic features.** Every future
   feature should attach to the same turn, trace, and revision objects.
2. **Keep two lanes.** Obvious questions use the existing fast path. Ambiguous
   or high-risk questions pay for structured interpretation and stronger checks.
3. **Use AI where it provides leverage; constrain consequences in Rust.** The
   model can suggest a contract, but cannot bypass catalog scope, read-only
   execution, budgets, or verification.
4. **Treat ambiguity as a state.** Do not turn an unresolved interpretation
   into a confident SQL query merely because the query runs.
5. **Keep advanced paths.** SQL, Python, documents, and charts remain available
   as fallbacks, with their actual verification level made explicit.
6. **Make the backend canonical.** Tauri and Electron should consume the same
   runtime protocol; the Svelte UI should project runtime state rather than
   assemble its own interpretation of a turn.
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
- the same typed event protocol through Tauri and the Electron stdio bridge.

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

### M2 — Semantic interpretation and risk gating

Introduce a compact `AnalysisContract` for the common analytical core:

- population and grain;
- measures and operations;
- filters and resolved/candidate values;
- time field, range, and timezone;
- grouping and comparison;
- requested presentation;
- assumptions and unresolved ambiguity.

Add a risk router:

```text
single obvious lookup       -> current fast path
trend / ratio / join        -> contract + stronger checks
ambiguous value or field    -> contract + grounding probes
material disagreement      -> clarification
unsupported request        -> explicit refusal
```

The structured call must have a provider-neutral fallback. Providers that
support structured output can use it; others receive a strict compact schema
prompt and are validated before the contract is accepted.

**Exit criteria:** the system can distinguish “the model wrote valid SQL” from
“the model understood the question,” and it does not silently choose between
materially different interpretations.

### M3 — Grounding, probes, and logical planning

Add internal bounded probes for candidate values, date coverage, units, nulls,
zero-result filters, and join cardinality. Then compile grounded contracts
into a typed `LogicalPlan` and deterministic SQL for the common core:

- filter and aggregate;
- group and rank;
- date buckets;
- comparisons and ratios;
- declared joins;
- chart result shapes.

Model-generated SQL and Python remain the advanced fallback. The compiler owns
physical table names, quoted identifiers, date functions, null behavior, and
denominator definitions.

The first implementation slice is intentionally narrower than the destination:
it only compiles a grounded single queryable source with aggregate measures,
observed filter values, ISO-like year/month/date ranges, group-by fields,
ratios whose operands are declared measures, and explicit join edges whose
sources and keys are grounded against the current snapshot. Connected joins
with qualified, unambiguous references now compile into read-only SQL; the
fallback remains the safety valve for disconnected graphs and unsupported
comparison semantics. A first typed comparison slice also supports one-source
period-over-period contracts with explicit current and previous ranges, bounded
range probes, and deterministic change columns. It intentionally does not yet
combine comparisons with joins, time buckets, or derived metrics.
When any of those conditions is not provably satisfied, Fella keeps the
contract for verification but returns to the existing model-driven path.

**Exit criteria:** common questions execute from a validated semantic plan;
advanced questions still work through the existing direct path.

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
mounted copy of the same workspace while preserving rerun lineage. A richer
revision-diff view and UI controls for inspecting/rerunning stored turns remain
follow-up work in this milestone.

The UI then becomes a projection:

- answer first;
- only relevant assumptions and caveats inline;
- trace/evidence progressively disclosed;
- raw SQL and rows available on demand;
- Context becomes definitions and scoped references, not a second prompt editor;
- Projects become summaries of workspace knowledge, not another source of truth.

**Exit criteria:** Tauri and Electron show the same runtime result and a past
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
turns, and makes omissions explicit so the model can fall back to the read-only
tools. This applies the minimal-relevant-context principle described by
[CHESS: Contextual Harnessing for Efficient SQL](https://arxiv.org/abs/2405.16755)
without introducing a vector store into the local-first base product.

## Research-informed design choices

These papers inform specific mechanisms; they do not define Fella's product or
justify importing their full systems.

- **Semantic intermediate representation and deterministic compilation.**
  The semantic-layer-mediated NL-to-SQL work separates semantic intent from
  physical SQL with a compact intermediate representation and a deterministic
  dialect compiler. Fella adopts that shape as `AnalysisContract` →
  `LogicalPlan` → SQL for its common analytical core, while retaining a direct
  fallback for advanced questions. See [Kim, Khoeurn, and Yoon, *A
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
- Tauri and Electron consume the same runtime protocol;
- every new quality feature is covered by a replayable evaluation case.

## Explicit non-goals

This roadmap does not require a multi-agent graph, a hosted memory service, a
vector database, a plugin marketplace, write-capable tools, or a dashboard
platform. The ambition is analytical depth and reliability, not general
autonomy or feature breadth.

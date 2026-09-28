# Fella Semantic Runtime

> Proposal for the long-term analytics harness. This describes the target
> architecture, not a claim about what is already implemented.

## Purpose

Fella should be the fastest, most dependable way to get a useful analytical
answer from data that has not already been cleaned, modeled, or explained by a
data team.

The central quality problem is not whether a model can produce valid SQL. It is
whether the answer reflects what the user meant, what the files actually
contain, and what the calculation can support.

The target is an **adaptive semantic runtime**:

> The model proposes meaning. Fella grounds that meaning in the current data,
> compiles supported analysis into safe execution, checks the result, and asks
> for clarification when the remaining ambiguity can change the answer.

This is broader than a prompt, a memory file, or a SQL agent. It is also
narrower than a general-purpose autonomous agent: Fella remains read-only,
local-first, model-agnostic, and optimized for analytical answers.

## Fella Analytics Runtime / Harness

The whole product should be understood as one analytical runtime. The harness
is not a chat agent with tools attached; it is the decision system that turns a
question about a workspace into a bounded, checked analytical result.

The runtime owns the complete lifecycle:

~~~text
workspace scope
  -> conversation session
  -> semantic interpretation
  -> analysis contract
  -> grounding and logical planning
  -> read-only execution
  -> semantic verification
  -> analytical result
  -> memory, history, and evaluation
~~~

This does not mean putting every subsystem in one module. Cohesion comes from
one lifecycle and one set of shared runtime objects, not from a monolith.
The UI, model adapter, persistence layer, deterministic analytics engine, and
verification code can remain separate while participating in the same turn
protocol.

### The first-class runtime objects

| Object | Responsibility |
|---|---|
| `WorkspaceModel` | The current data scope: sources, profiles, definitions, relationships, caveats, memory, and revision. |
| `ConversationSession` | A sequence of analytical turns tied to a workspace and its revisions. |
| `AnalysisTurn` | One question's complete lifecycle, including interpretation, plan, execution, and result. |
| `AnalysisContract` | What the question means: population, grain, measures, filters, time, comparisons, output shape, and assumptions. |
| `ExecutionTrace` | What Fella actually ran: queries, probes, document reads, calculations, charts, provenance, timing, and failures. |
| `VerificationReport` | Whether the computation and its interpretation are supported by the current workspace. |
| `AnalysisResult` | The user-facing answer plus assumptions, status, evidence, scope, and usage. |
| `SemanticMemory` | Versioned, authority-aware facts that help Fella interpret future questions consistently. |

### Authority boundaries

The model proposes meaning and explains results. The runtime owns workspace
scope, permissions, planning, execution, verification, and memory promotion.
The user resolves ambiguity and confirms definitions. The data is the authority
for observed values.

~~~text
model        -> hypotheses and narration
user data    -> observed facts
runtime      -> constrained computation and verification
user         -> clarification and confirmed definitions
memory       -> scoped, evidence-backed continuity
~~~

The model is allowed to be uncertain. The runtime is not allowed to silently
turn uncertainty into authority.

### The analytical turn

Every question passes through the same conceptual state machine, even when the
fast path skips expensive stages:

~~~text
received -> interpreted -> grounded -> planned -> executing -> verifying -> accepted
                         |                         |             |
                         v                         v             v
                      clarify                  retry         needs review
                         |
                         v
                      unsupported
~~~

Low-risk questions may create a minimal contract and go directly to execution.
High-risk questions can produce candidate contracts, run bounded probes, ask a
focused clarification, or use a stronger verification path. The user should
not pay the cost of the full ceremony when the answer is obvious, but every
answer should still have a place for interpretation and verification.

### Current product concepts as runtime projections

The visible product concepts are views over the runtime, not separate sources
of truth:

| Product surface | Runtime projection |
|---|---|
| Sources | `WorkspaceModel` source registry and profile |
| Context / Guide | User-authored semantic definitions and policies |
| Memory | Versioned semantic facts and correction history |
| Conversation | Session containing analytical turns |
| Evidence | A readable view of `ExecutionTrace` |
| Run timeline | Live state of the current `AnalysisTurn` |
| Project | A human-facing summary of a workspace's analytical knowledge |
| Search | An index over turns, findings, definitions, and sources |
| Settings | Provider, model, privacy, and capability policy |

No surface should maintain a second interpretation of what a workspace is or
what an answer means.

### The product distinction

Fella is not primarily a workspace with an AI assistant inside it, and it is
not a SQL agent with an evidence panel bolted on. It is a read-only analytical
runtime in which:

> The model proposes. The runtime grounds. The data computes. The verifier
> judges. The user confirms. Memory compounds.

That closed loop is the harness. It is where Fella's three principles become
technical behavior:

- **Consistency:** contracts and semantic memory preserve meaning across turns.
- **Correctness:** execution and verification are tied to the current workspace
  revision.
- **Efficiency:** risk gating, bounded probes, fixed tools, parallel execution,
  and cached profiles keep simple questions fast.

## Design principles

1. **Interpretation may be probabilistic; execution must be constrained.**
   Natural language cannot be deterministically interpreted in all cases. The
   model can propose an interpretation, but Fella must validate its references,
   assumptions, and consequences before trusting the result.
2. **Meaning comes before physical SQL.** The model should reason about
   measures, populations, dimensions, time, and definitions before choosing
   physical column expressions.
3. **Ambiguity is a state, not an error.** If two supported interpretations
   produce materially different answers, Fella should clarify rather than
   silently select one.
4. **Execution is useful for grounding, not only checking.** Cheap queries can
   reveal which values, dates, units, and relationships actually exist.
5. **Verification checks analytical meaning as well as reproducibility.** A
   query that runs consistently can still answer the wrong question.
6. **Simple questions stay simple.** The full pipeline is risk-gated. A basic
   lookup should not pay for a full interpretation ceremony.
7. **Every inferred fact has authority and provenance.** User definitions,
   observed data properties, model suggestions, and confirmed corrections must
   not be treated as equivalent.
8. **Unsupported analysis remains possible.** The structured semantic path
   handles the common analytical core; advanced SQL, Python, and document
   analysis retain a clearly marked fallback path.

## Target architecture

~~~text
Files, databases, and documents
              |
              v
       Workspace Semantic Model
       - sources and schemas
       - column profiles and common values
       - entities and relationships
       - measures and metrics
       - units and time conventions
       - value aliases and caveats
       - user definitions and confirmed memory
              |
              v
          User question
              |
              v
      Interpretation engine
      - candidate meanings
      - schema/value grounding
      - ambiguity and risk detection
      - targeted probes
      - clarification when necessary
              |
              v
         Analysis Contract
      - population and grain
      - measures and operations
      - filters and resolved values
      - time range and timezone
      - grouping and comparison
      - assumptions and unresolved items
              |
              v
       Semantic planner/compiler
              |
              v
      Existing Fella execution loop
      - read-only SQL
      - bounded Python
      - document search
      - charts
              |
              v
       Semantic verification
      - query reproducibility
      - numeric and type checks
      - analytical invariants
      - assumption checks
      - answer completeness
              |
              v
      Answer, assumptions, evidence, and status
              |
              v
       Confirmed semantic memory
       Evaluation and trace records
~~~

## The durable workspace model

The workspace model is the semantic understanding of a data scope. It is
derived per workspace revision and should be rebuildable from the files,
user-authored context, and confirmed memory.

It should eventually contain:

| Area | Examples |
|---|---|
| Sources | files, sheets, tables, documents, source revision |
| Fields | physical name, type, null rate, distinct count, examples |
| Profiles | date coverage, units, sign conventions, common values, outliers |
| Entities | transaction, person, subscription, workout, account |
| Relationships | candidate and confirmed keys, join cardinality, caveats |
| Dimensions | category, merchant, month, status, region |
| Measures | amount, duration, count, distance |
| Metrics | revenue, average order value, active subscriptions |
| Value maps | rent -> Rent, Housing, Mortgage when confirmed |
| Policies | exclusions, preferred sources, exact-match instructions |
| Caveats | text amounts, dropped total rows, missing periods, mixed units |

The model has three possible sources:

~~~text
Automatic: inferred from files and data profiling
Guided:    added by the user in fella.md or the UI
Imported:  loaded from a future dbt/OSI/Wren/Cube-style definition
~~~

Automatic facts should be marked as inferred or observed. They must not be
silently promoted to user rules.

The current implementation keeps the source of truth in the catalog, SQLite
state, `fella.md`, `memory.md`, and the adjacent typed fact ledger. Completed
turns are also persisted as backend-owned JSON records; the important part is
the contract and authority model, not the file format.

## The per-question Analysis Contract

The contract is an intermediate representation of the question. It is not
intended to replace natural-language conversation or expose raw planner output
to ordinary users.

~~~json
{
  "subject": "transactions",
  "population": "expense transactions",
  "grain": "transaction",
  "measures": [
    { "field": "amount", "operation": "sum", "unit": "USD" }
  ],
  "filters": [
    {
      "concept": "rent",
      "field": "category",
      "resolved_values": ["Rent", "Housing", "Mortgage"],
      "resolution": "user_confirmed"
    }
  ],
  "time": {
    "field": "transaction_date",
    "range": "2024",
    "timezone": "local"
  },
  "group_by": [],
  "comparison": null,
  "presentation": "single_value",
  "assumptions": [],
  "unresolved": []
}
~~~

Important properties:

- It references semantic objects and catalog IDs, not only raw SQL names.
- It records the population and grain, preventing many denominator errors.
- It distinguishes a user-confirmed value map from a model suggestion.
- It can contain unresolved ambiguity rather than forcing a guess.
- It describes the requested result shape, which helps answer completeness.
- It remains small enough to fit into the existing model loop.

The model can propose a contract, but the backend owns its validation and
status. Possible interpretation statuses are:

~~~text
grounded       references current data and known definitions
assumed        plausible, but not explicitly confirmed
ambiguous      multiple supported meanings remain
unsupported    the data cannot represent the request
~~~

## Interpretation strategies

Fella should support several strategies behind one interface.

### Direct interpretation

The model goes directly from question to tools. This remains the fast path for
low-risk questions and the fallback for capabilities not covered by the
structured planner.

### Structured interpretation

The model emits an Analysis Contract through structured output or an internal
planning call. Rust validates it before execution.

### Candidate interpretation

The model emits a small set of plausible contracts when a term is ambiguous.
Fella grounds each candidate with bounded catalog or SQL probes.

### Interactive interpretation

Fella asks one targeted clarification when candidate interpretations are both
supported and would materially change the answer.

The clarification should be concrete:

> I found Rent, Housing, and Mortgage as separate categories. Should all three
> count as rent?

It should not ask the user to restate the entire question.

## Risk gating

The full interpretation path should activate when the question or workspace
has signals such as:

- multiple plausible measure or date fields;
- low-cardinality labels with aliases or near-duplicates;
- a filter value with no exact data match;
- multiple candidate join paths;
- a requested ratio, comparison, trend, or cohort;
- a zero-result filter where a nonzero result is plausible;
- a forecast or causal claim unsupported by the workspace;
- a definition in memory that conflicts with current data;
- a question spanning multiple source types;
- an advanced Python/statistical calculation without replayable backend support.

Low-risk questions can continue through the current one- or two-call path. A
high-risk question can pay for a contract, probe, or clarification because a
wrong answer is more expensive than one extra round trip.

## Semantic planning and compilation

The structured path should cover a common analytical core:

- selection and filtering;
- aggregates and distinct counts;
- grouping and ranking;
- time buckets and date ranges;
- ratios and comparisons;
- declared joins;
- top/bottom and contribution analysis;
- chart result shapes.

The compiler owns physical details such as table names, column expressions,
join predicates, date functions, null behavior, and unit conversion.

Advanced SQL, Python statistics, document analysis, and unusual queries remain
available through the fallback path. Those answers should carry a capability or
verification note rather than pretending to have the same guarantee as a
compiled plan.

## Probing and clarification

Probes are small, bounded observations used to resolve meaning:

- common values for a category field;
- row counts for candidate filters;
- date coverage and gaps;
- unit and magnitude checks;
- candidate join cardinalities;
- grouped totals for competing interpretations;
- null and sentinel distributions.

The probe planner should be internal rather than adding many model-visible
tools. Existing catalog inspection and run_sql capabilities can provide much
of the execution surface.

The decision policy is:

~~~text
one candidate is unsupported       -> discard it
all candidates produce same result  -> proceed, record the assumption
one candidate is clearly grounded   -> proceed
supported candidates differ         -> clarify or surface the assumption
no candidate is supported            -> say the data cannot answer
~~~

## Semantic verification

The existing verifier reruns cited SQL and checks that answer figures are
grounded. The semantic runtime adds checks against the contract and result
shape.

Examples:

~~~text
requested date range has data
resolved values exist in the current snapshot
filtered count does not exceed the unfiltered count
grouped totals reconcile with the overall total
numerator does not exceed denominator
average lies within observed bounds
percentages approximately sum to 100%
chart values equal the source query
all requested comparisons are present
~~~

For high-risk calculations, Fella may run two independent formulations, such
as a direct aggregate and a sum of grouped subtotals. Agreement is supporting
evidence; disagreement is a reason to retry, clarify, or mark the answer for
review.

Verification should be a backend quality gate. The default UI should expose
only the relevant assumption or caveat, with the full contract and checks
available through progressive disclosure.

## Memory and authority

Semantic memory should store facts about meaning, not arbitrary transcript
fragments:

~~~text
concept: rent
meaning: category values Rent, Housing, Mortgage
scope: workspace
source: user correction
workspace_revision: ...
status: confirmed
~~~

Every semantic fact should carry:

- scope;
- source;
- authority;
- workspace revision;
- creation and update time;
- supporting evidence;
- supersession state.

A single model guess should not become a permanent rule. User corrections and
repeated, verified groundings are stronger candidates. Contradictions should
be retained until resolved rather than silently overwritten.

## Fella's current module boundaries

The target can fit the existing architecture without replacing the harness:

| Existing area | Long-term responsibility |
|---|---|
| state.rs | Workspace model lifecycle, revisions, sessions, memory scope |
| catalog.rs | Source discovery and base profiling |
| memory.rs / semantic_memory.rs | Human-readable memory projection plus versioned semantic facts and correction history |
| analysis_store.rs | Backend-owned persisted turn records, workspace-safe reruns, and lineage |
| agent.rs | Risk gating, interpretation calls, execution orchestration |
| planner.rs / analytics/ | Deterministic logical-plan compilation, profiles, probes, and invariants |
| tools.rs | Fixed read-only execution boundary |
| verify.rs | Evidence, contract, and semantic verification |
| evidence.rs | Contract summary, assumptions, checks, source revision |
| ContextAssembler | Typed context packet for each model turn |
| AnalysisContract | Per-question interpretation IR |
| WorkspaceModel | Durable semantic model for a workspace revision |

The model still cannot write files or bypass the read-only data boundary.

## External patterns to borrow

- **Semantic intermediate representations and deterministic compilation:**
  [Kim, Khoeurn, and Yoon, *A Semantic-Layer-Mediated Agent for Natural
  Language to SQL over Heterogeneous Enterprise Databases*](https://arxiv.org/abs/2606.31041)
  separates semantic intent from physical SQL with a compact intermediate
  representation and a deterministic compiler. Fella borrows the boundary,
  not the enterprise infrastructure.
- **Execution-grounded semantic memory:** [Lee, Kim, and Hwang,
  *Bootstrapping Semantic Layer from Execution for Text-to-SQL*](https://arxiv.org/abs/2606.05634)
  keeps unresolved groundings open until execution provides evidence. Fella
  uses this to distinguish observed support from a model guess before a fact
  becomes durable memory.
- **Targeted ambiguity probes:** [Somayajula et al., *SOMA-SQL: Resolving
  Multi-Source Ambiguity in NL-to-SQL via Synthetic Log and Execution
  Probing*](https://arxiv.org/abs/2606.11424) uses an ambiguity taxonomy and
  disagreement-driven probes. Fella adapts the idea to bounded local probes
  and user clarification.
- **Interactive clarification:** [Ding, Lin, and Zeng, *AmbiSQL: Interactive
  Ambiguity Detection and Resolution for Text-to-SQL*](https://arxiv.org/abs/2508.15276)
  treats clarification as part of the analytical path rather than a generic
  failure message. Fella adopts focused clarification when interpretations
  would materially change the result.
- **Wren AI:** explicit context artifacts, semantic modeling, planning, dry
  validation, memory, and evaluation. Borrow the separation; do not inherit
  its full runtime or infrastructure.
- **dbt MetricFlow / Cube:** entities, dimensions, measures, metrics, joins,
  scopes, and deterministic query compilation. Borrow the vocabulary and
  logical model.
- **GATE:** use execution to ground unresolved semantic hypotheses and produce
  reusable memory.
- **SOMA-SQL:** use targeted probes when candidate interpretations disagree.
- **AmbiSQL:** use an ambiguity taxonomy and focused clarification questions.
- **Vanna-style systems:** successful query examples can be useful memory, but
  they should remain subordinate to current data and verified definitions.

Fella should not make a vector database, multi-agent graph, or hosted memory
service a base dependency. Those can become optional adapters for a future
enterprise profile.

## Incremental implementation order

The architecture should be designed up front, then delivered in vertical
slices:

1. Define WorkspaceModel, AnalysisContract, LogicalPlan, and
   VerificationReport types.
2. Expand the workspace profile with common values, date coverage, units,
   candidate identifiers, and data caveats.
3. Add structured interpretation with a reliable fallback to the existing loop.
4. Add catalog and value grounding.
5. Add ambiguity risk scoring and targeted probes.
6. Add clarification for materially different interpretations.
7. Compile the common analytical core into validated SQL.
8. Add semantic invariants and independent high-risk checks.
9. Make semantic memory versioned, scoped, and evidence-backed.
10. Add import/export adapters for external semantic definitions.

Each phase should be benchmarked against correctness, semantic correctness,
clarification quality, useful tool calls, time-to-correct-answer, and memory
carryover. The direct agent path should remain available until the structured
path proves it is better for a given question family.

## Quality bar

The runtime is successful when it improves all three product goals.

### Fast

- simple questions stay at one or two model calls;
- profiles and semantic definitions are cached by workspace revision;
- probes are bounded and local;
- clarification avoids expensive wrong-answer retries.

### Correct

- valid SQL is not mistaken for correct interpretation;
- ambiguity is surfaced when it changes the answer;
- answers carry the applied assumptions;
- memory corrections are applied consistently;
- high-risk calculations receive stronger checks.

### Versatile

- ordinary folders work without manual modeling;
- advanced users can author definitions;
- enterprise users can import semantic models;
- SQL, Python, documents, and charts remain supported;
- new data engines can implement the same logical-plan and verification seams.

## Open design questions

- Which contract fields should be required for each analysis family?
- How should Fella estimate whether two interpretations are materially different?
- Which probes can be generated safely without exposing another model-visible
  tool?
- When should a model-generated value map be offered for confirmation?
- How should a user explicitly request exact-label semantics?
- Which statistics can receive replayable verification?
- Should imported semantic models be translated into Fella's model or remain
  external adapters?
- What is the smallest high-risk ambiguity benchmark that reflects real use?

The guiding rule remains: do not force natural language into a deterministic
shape. Build a system in which uncertain interpretation is explicit and every
downstream operation is as deterministic, bounded, and auditable as possible.

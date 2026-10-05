# Fella Semantic Runtime

> Proposal for the long-term analytics harness. This describes the target
> architecture, not a claim about what is already implemented. The current
> delivery status and remaining work are tracked in
> [`ANALYTICAL-COMPUTER-ROADMAP.md`](ANALYTICAL-COMPUTER-ROADMAP.md).

## Purpose

Fella should be the fastest, most dependable way to get a useful analytical
answer from data that has not already been cleaned, modeled, or explained by a
data team.

The central quality problem is not whether a model can produce valid SQL. It is
whether the answer reflects what the user meant, what the files actually
contain, and what the calculation can support.

The target is a **model-driven analytical computer**:

> The model interprets the question, chooses the next useful observation or
> computation, writes or selects SQL/Python when useful, and explains the
> result. Fella supplies bounded context, a safe read-only analytics engine,
> execution feedback, and checks that help the model revise its work. It asks
> for clarification when a material user-owned choice remains unresolved.

This is broader than a prompt, a memory file, or a SQL agent. It is also
narrower than a general-purpose autonomous agent: Fella remains read-only,
local-first, model-agnostic, and optimized for analytical answers.

The orchestration pattern is deliberately familiar from modern coding
harnesses: inspect the mounted files, reason about the task, generate SQL or
Python, run it, examine the output, and iterate. Fella's specialization is the
analytics substrate around that loop: filesystem reconnaissance, data-aware
execution, persistent analytical state, and evidence tied to each result. The
compiler is an optimization for work it can express, not a gate that decides
whether the model is allowed to analyze the data.

## Fella Analytics Runtime / Harness

The whole product should be understood as one analytical runtime. The harness
is not a chat agent with tools attached; it is the decision system that turns a
question about a workspace into a bounded, checked analytical result.

The runtime owns the complete lifecycle:

~~~text
workspace scope
  -> conversation session
  -> model-directed interpretation and tool use
  -> semantic hypotheses and observations
  -> compiled or direct read-only execution
  -> semantic verification
  -> analytical result
  -> memory, history, and evaluation
~~~

This does not mean putting every subsystem in one module. Cohesion comes from
one lifecycle and one set of shared runtime objects, not from a monolith.
The UI, model adapter, persistence layer, deterministic analytics engine, and
verification code can remain separate while participating in the same turn
protocol.

### The analyst loop

Fella follows the working loop of a careful analyst, not a fixed ETL ceremony
followed by a one-shot answer:

~~~text
mount -> local reconnaissance
question -> inspect -> frame/decompose as useful -> investigate
               ^                 |                    |
               |                 v                    v
               +---------- observations/errors <- generate SQL/Python
                                                        |
                                             read-only analytics engine
                                                        |
                                                   result / error
                                                        |
                                        inspect, revise, or compute again
                                                        |
                          answer / qualified answer / partial / clarify
~~~

Mounting establishes a read-only map of available sources, schemas, profiles,
and caveats. It should not force every file into a supposedly canonical
meaning. For each question, the model uses that map to decide what to inspect
more closely: relevant columns and observed labels, samples, notes, candidate
relationships, date coverage, or a small query. It decomposes a multi-part
question when that helps locate and check the needed evidence. Each
observation can revise the interpretation or lead to another targeted
inspection before the model commits to a computation.

In analyst terms: first learn what data exists, where it lives, how it is
labeled, and which labels may matter; the files need not be clean first. Then
break the question into smaller questions when useful and try to answer them
against the inspected sources. Repeat inspection and investigation as results
or errors reveal what is still unknown. Once the model has enough context, it
generates or selects SQL/Python, runs it through Fella's bounded analytics
engine, inspects the output, and continues or revises until it can give the
strongest useful answer the evidence supports. This is an adaptive loop, not a
mandatory checklist: a clear, simple question can go directly to execution.

The loop must preserve the evidence that gives later computations meaning.
As tool history grows, Fella keeps a bounded working set of source definitions
and the latest semantic frame while eliding stale, reproducible query output.
If a definition no longer fits that budget, the model must be able to re-read
it before relying on it; an observed count is never a substitute for a target
or rule stated in a document.

For decisions across candidates, the model keeps the criterion, comparator,
measured quantity, and scope explicit, then applies them consistently to the
candidate set. For requested differences, rates, ratios, and percentages, the
transformation belongs in the executed computation with labeled operands, not
only in answer prose. These are analytical invariants, not domain-specific
fallbacks.

Observed labels are evidence for interpretation, not a universal rule that
quoted language must match a stored value byte-for-byte. The model should use
the question, profiles, examples, notes, and prior definitions to decide
whether a label is an exact value, an alias, or a broader concept. It should
not silently substitute a nearby label based only on word similarity; when it
does make a non-literal mapping, it should check the relevant values and state
the mapping. If useful, compute exact-match and plausible mapped results side
by side instead of blocking the analysis.

Clarification comes after reasonable investigation, when the remaining choice
belongs to the user and could materially change the answer. Uncertainty alone
does not require a clarification or stop. Fella may state a reasonable
assumption, present a qualified or partial answer, or calculate candidate
interpretations for comparison. If it asks the user, only the definitive
choice that depends on that reply is pending; safe, useful analysis and
candidate results need not be discarded or delayed. If both interpretations
produce the same answer, or evidence favors one, do not interrupt the user
unnecessarily.

### Useful answers over defensive abstention

Trust is not maximized by abstaining whenever the runtime cannot prove every
semantic choice in advance. A restriction can create a false negative: it can
prevent the model from reaching a correct, evidence-backed answer. Fella should
optimize for useful, well-grounded answers while making uncertainty visible,
not for the highest possible accuracy among only the answers it permits itself
to give.

Keep hard limits for what the system may do: workspace permissions, read-only
access, sandbox boundaries, resource budgets, and invalid execution. Treat
uncertainty about what the user means as a model-facing signal to inspect,
compare, compute, revise, or clarify—not as an automatic denial. A compiler
not supporting a query shape, an ungrounded first hypothesis, or an imperfect
profile does not mean the data is unanalyzable; SQL, Python, document reading,
and other safe routes remain available.

When confidence is incomplete, prefer the strongest useful response the
evidence supports:

1. Answer directly when the evidence is clear.
2. Answer with an explicit assumption or caveat when one interpretation is
   reasonable but not certain.
3. Give a partial answer or compare plausible interpretations when that helps.
4. Ask one focused question when a material, user-owned choice remains.
5. Say the data cannot answer only when investigation finds no usable evidence
   path; describe an execution limitation as a limitation, not as proof that
   the data itself is insufficient.

Verification should trigger targeted revision, improve the answer's stated
confidence, or mark a specific claim for review. It should not erase an
otherwise useful answer merely because a semantic check is inconclusive. Tests
and evaluations must track answer coverage and unnecessary abstention alongside
correctness, calibration, clarification burden, and cost. For blocked cases,
shadow evaluations can relax semantic gates while keeping the same read-only
sandbox; a correct result in shadow mode identifies lost answer coverage caused
by the harness.

The model is presumed capable of useful analysis when given relevant evidence,
effective read-only tools, and execution feedback. A failure should initially
be investigated as a possible harness, context, ingestion, tool, or execution
problem; attribute it to model reasoning only when a controlled replay
isolates that cause. Fella's job is to enable and check the model's work, not
to replace its interpretation with an expanding list of deterministic
language rules.

### The first-class runtime objects

| Object | Responsibility |
|---|---|
| `WorkspaceModel` | The current data scope: sources, profiles, definitions, relationships, caveats, memory, and revision. |
| `ConversationSession` | A sequence of analytical turns tied to a workspace and its revisions. |
| `AnalysisTurn` | One question's complete lifecycle, including interpretation, plan, execution, and result. |
| `AnalysisContract` | What the question means: selected source, population, grain, measures, filters, time, comparisons, output shape, and assumptions. |
| `ExecutionTrace` | What Fella actually ran: queries, probes, document reads, calculations, charts, provenance, timing, and failures. |
| `VerificationReport` | Whether the computation and its interpretation are supported by the current workspace. |
| `AnalysisResult` | The user-facing answer plus assumptions, status, evidence, scope, and usage. |
| `SemanticMemory` | Versioned, authority-aware facts that help Fella interpret future questions consistently. |

### Context assembly

These objects are assembled into one bounded `ContextPacket` before the model
call. The packet is a prompt projection, not another source of truth:

~~~text
WorkspaceModel profile + schema + user definitions + semantic memory + recent session
                                |
                                v
                         ContextAssembler
                    - section budgets
                    - question-aware selection
                    - newest-turn retention
                    - explicit omission notices
                                |
                                v
                         analytical prompt
~~~

Normal-sized sections pass through unchanged. If a section exceeds its budget,
the assembler keeps relevant schema/definition lines, retains the newest
conversation material, and tells the model that more context was omitted so it
can use the bounded read-only tools. This is the local, deterministic form of
the minimal-relevant-context principle used by [CHESS: Contextual Harnessing
for Efficient SQL](https://arxiv.org/abs/2405.16755). The goal is not to make
the model memorize less by default; it is to stop an accidental oversized file
or transcript from crowding out the actual analytical question.

The canonical turn record keeps a length-only `context_audit` for these
assembled sections (source size, retained size, and whether that section was
truncated), plus provider-reported token usage when available. It does not
persist the context text or claim to snapshot every later tool result in the
model's working history; those are represented by the execution trace and
evidence instead.

When a user answers a clarification card, the reply carries the id of the turn
that asked it. The engine verifies that the turn is still awaiting a choice and
belongs to the same conversation and mounted workspace, then resumes the
original analytical question with the user's choice as structured context. The
new turn records its parent and response, and runs against the current data
revision. A one-turn choice is not silently promoted into folder-wide semantic
memory.

### Authority boundaries

The model owns interpretation, decomposition, tool choice, and explanation.
The runtime owns workspace scope, permissions, budgets, read-only execution,
verification, and memory promotion. The user resolves ambiguity and confirms
definitions. The data is the authority for observed values.

~~~text
model        -> interpretation, decomposition, tool choice, and narration
user data    -> observed facts
runtime      -> bounded tools, compilation, and verification
user         -> clarification and confirmed definitions
memory       -> scoped, evidence-backed continuity
~~~

The model is allowed to be uncertain. The runtime is not allowed to silently
turn uncertainty into authority.

### The analytical turn

Every question uses the same model-driven execution loop, without requiring a
fixed sequence of steps:

~~~text
question -> inspect / interpret -> choose tools or generate SQL/Python
                 ^                                      |
                 |                                      v
                 +--------- observe result <- read-only execution engine
                                    |
                      revise / compute again / finish
                                    |
          answer / qualified or partial answer / clarify / no evidence path
~~~

The model may answer a simple question with one direct tool call, or inspect,
decompose, generate SQL/Python, use a semantic hypothesis, compile a supported
plan, search documents, make a chart, and iterate when the question needs it.
Risk signals tune attention and verification; they never remove a safe route or
force a ceremony before the model can investigate.

### The semantic decision edge

`AnalysisContract` is an intermediate representation, not a forced planner
step. It gives the model a compact place to state its current meaning for the
question, while the runtime grounds names, values, sources, and relationships
against the mounted revision. The contract has four practical outcomes:

1. **Resolve:** the wording and workspace evidence identify one interpretation;
   the model can execute it.
2. **Assume:** one interpretation is reasonable but not mathematically forced;
   the model proceeds and states the assumption in the answer.
3. **Clarify:** two or more supported interpretations would materially change
   the result, and the choice belongs to the user. The model may emit one typed
   `clarification` request with a concise question and optional choices. Fella
   should retain useful candidate calculations or partial results where
   possible; only a definitive conclusion that requires the user's choice is
   pending. The user's reply resumes the analysis with that decision in
   conversation context.
4. **Unsupported:** the workspace cannot answer the requested analysis through
   any reasonable available read-only path. A limitation in one compiler or
   execution route alone is not enough to call the analysis unsupported.

This is the right place for a Jev-like decision model if Fella ever adopts
one: it could cheaply route a grounded question among `resolve`, `assume`,
`clarify`, and `unsupported`, or rank explicit candidate interpretations. It
must not generate the candidates from nothing, replace the analytical model,
or overrule observed data. Fella's current implementation keeps this decision
inside the existing model-directed loop and typed contract, so it does not add
a second model, network dependency, or hidden source of truth before an eval
shows that one is worthwhile. This follows the same division TypeSafe
describes for Jev: typed decisions route work, while code performs exact
operations and a general model handles open-ended reasoning. See
[TypeSafe's Jev overview](https://www.typesafeai.org/jev).

The threshold is intentionally semantic rather than a fixed confidence score:
ordinary aliases and messy labels should be investigated using schema,
observed values, notes, examples, and probes; a clarification is reserved for a
material user-specific choice such as whether income belongs in a spending
total. Low confidence can motivate more inspection or a caveat, but is not by
itself a reason to withhold an answer. A classifier confidence score can inform
this edge later, but confidence alone cannot decide whether two interpretations
have materially different consequences.

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

Fella is an analytics harness: it uses the familiar model-driven
inspect–generate–execute–observe loop of other agent harnesses, specialized by
local data reconnaissance and a read-only analytical engine. It is not a
separate orchestration paradigm or a SQL agent with an evidence panel bolted
on. Its runtime coordinates:

> The model drives. SQL and Python express the analysis. Fella's engine runs it
> safely against the data. Evidence and checks inform the next step. The user
> clarifies genuine choices. Memory compounds.

That closed loop is the harness. It is where Fella's three principles become
technical behavior:

- **Consistency:** contracts and semantic memory preserve meaning across turns.
- **Correctness:** execution and verification are tied to the current workspace
  revision, without turning uncertainty into an automatic refusal.
- **Efficiency:** model-chosen tool calls, bounded observations, fixed tools,
  parallel execution, and cached profiles keep simple questions fast without
  limiting the model on harder questions.

## Design principles

1. **Interpretation may be probabilistic; consequences must be constrained.**
   Natural language cannot be deterministically interpreted in all cases. The
   model must be free to form and revise interpretations, while Fella keeps
   every consequence inside the bounded, read-only execution boundary.
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
6. **The model chooses the route.** A basic lookup should stay one call, while
   a harder question may spend additional calls when that materially improves
   correctness.
7. **Every inferred fact has authority and provenance.** User definitions,
   observed data properties, model suggestions, and confirmed corrections must
   not be treated as equivalent.
8. **No capability is hidden behind a semantic gate.** The structured contract,
   direct SQL, bounded Python, document search, and charts are all legitimate
   model-selected routes. Each carries the verification level it actually
   earned.

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
        Model control plane
      - interpret and decompose
      - choose observations and tools
      - optionally emit a semantic hypothesis
      - revise from tool results
              |
        +-----+-----+------------------+
        |           |                  |
        v           v                  v
   inspect/probe  Analysis        direct tool calls
                  Contract        SQL · Python · docs · chart
        |           |
        +-----+-----+
              v
       Grounding + compiler
       - resolve names and values
       - compile supported plans
       - preserve model-selected alternatives
              |
              v
        Read-only execution
      - compiled SQL
      - model-authored SQL
      - bounded Python
      - document search and charts
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

The contract is an optional semantic intermediate representation of the
question. It is not a permission token, a required preflight, or a replacement
for natural-language conversation. The model can use it to make a hypothesis
legible to the compiler and verifier, then continue with direct tools when the
hypothesis is incomplete or when a different route fits better.

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
  "unresolved": [],
  "clarification": null
}
~~~

Important properties:

- It references semantic objects and catalog IDs, not only raw SQL names.
- It records the population and grain, preventing many denominator errors.
- It distinguishes a user-confirmed value map from a model suggestion.
- It can contain unresolved ambiguity rather than forcing a guess.
- It can carry one typed clarification request when the remaining choice
  belongs to the user; that request is not executable evidence.
- It describes the requested result shape, which helps answer completeness.
- It remains small enough to fit into the existing model loop.

The model can propose a contract, but the backend owns its normalization and
records its status. A non-grounded contract describes uncertainty; it does not
block an otherwise safe tool call. Possible statuses are:

~~~text
grounded       references current data and known definitions
assumed        plausible, but not explicitly confirmed
ambiguous      multiple supported meanings remain
unsupported    the data cannot represent the request
~~~

## Interpretation strategies

Fella should support several model-selected strategies behind one interface.

### Direct interpretation

The model goes directly from question to tools. This is the normal route when
the model already understands the workspace well enough, and it remains valid
even when a semantic hypothesis cannot be grounded.

### Structured interpretation

The model emits an Analysis Contract through structured output or an internal
planning call. Rust normalizes it, grounds what can be grounded, and may compile
it into deterministic execution. Direct model-selected calls remain available.

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

## Attention and verification signals

The runtime can detect signals such as:

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

These signals tune the model prompt, observation budget, and verification
strength. They do not decide that a question must call a contract tool first,
and they do not remove direct SQL, Python, document, or chart access. A simple
question can stay at one call; a difficult question can spend more calls when
that buys a better answer.

## Semantic planning and compilation

The deterministic compiler should cover a common analytical core:

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

Model-authored SQL, Python statistics, document analysis, and unusual queries
are first-class execution paths, not fallbacks. They should carry the
capability and verification notes they actually earned. The compiler is the
deterministic engine for the subset it can express; it is not the product's
interpretation authority.

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

The model/runtime decision policy is:

~~~text
an observation resolves a question  -> use it and continue
a hypothesis is partly grounded     -> keep supported bindings, revise the rest
multiple routes are plausible       -> let the model compare their observations
results materially disagree         -> retry, revise, or clarify
no route can support the claim      -> say the data cannot answer
~~~

## Semantic verification

The existing verifier reruns cited SQL and checks answer figures against
returned data cells, not execution metadata such as elapsed milliseconds or
tool-reported row counts. A numeric claim that is not in evidence triggers one
bounded tool-backed revise-or-retract pass, whatever the result shape; the
model may compute a missing derived value or remove an unsupported claim. The
semantic runtime also checks the contract against the result shape.

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

Verification is a backend quality gate for claims and computed outputs, not a
gate on which tool the model was allowed to try. The default UI should expose
only the relevant assumption or caveat, with the full hypothesis, trace, and
checks available through progressive disclosure.

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
| agent.rs | Model-directed interpretation, tool orchestration, and execution |
| planner.rs / analytics/ | Deterministic logical-plan compilation, profiles, probes, and invariants |
| tools.rs | Fixed read-only execution boundary |
| verify.rs | Evidence, contract, and semantic verification |
| evidence.rs | Contract summary, assumptions, checks, source revision |
| ContextAssembler | Typed context packet for each model turn |
| AnalysisContract | Per-question interpretation IR |
| WorkspaceModel | Durable semantic model for a workspace revision |

The model can drive every enabled read-only capability, but it still cannot
write files, access the filesystem directly, or bypass the read-only data
boundary.

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
3. Make optional model-generated semantic hypotheses available inside the
   existing loop without restricting direct tool use.
4. Add catalog and value grounding.
5. Add ambiguity risk scoring and targeted probes.
6. Add clarification for materially different interpretations.
7. Compile the common analytical core into validated SQL.
8. Add semantic invariants and independent high-risk checks.
9. Make semantic memory versioned, scoped, and evidence-backed.
10. Add import/export adapters for external semantic definitions.

Each phase should be benchmarked against correctness, semantic correctness,
clarification quality, useful tool calls, time-to-correct-answer, and memory
carryover. The model-directed loop remains the product path; contracts and
compiled plans earn authority by improving measured answers, not by replacing
working routes through policy.

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

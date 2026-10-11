# Fella product model

**Status:** Canonical north-star domain model, October 2026. This defines the
objects and invariants the product is being built toward; it is not a claim
that every object or workflow is implemented today. See [Product vision](PRODUCT-VISION.md)
for the user story, [Product](PRODUCT.md) for shipped behavior, and
[Architecture](ARCHITECTURE.md) for implementation boundaries.

## The organizing idea

Fella is an AI analytics harness for analysts working with files. Its durable
unit of work is an **Investigation**: one bounded effort to answer an
analytical problem. A Conversation is how the analyst directs that work; an
Analysis turn is one execution; Evidence supports a Finding; Artifacts make
selected results reusable. The model proposes and adapts, the runtime enforces
scope and computation boundaries, and the analyst steers consequential
meaning and decides whether the result is fit to share.

“Atomic primitive” means one clear, authoritative owner for a fact and its
lifecycle—not that every concept must immediately become a database table or
a new screen. The model distinguishes durable entities from versioned values
and views so Fella does not create duplicate records just to make the UI look
structured.

## Model at a glance

```text
Workspace ──catalogs──> Sources ──observed at──> Source revisions
    │                                              │
    └──bounds──> Investigation ──selects──────────┘
                    │       ├── Brief (versioned value)
                    │       ├── Questions (ordered, editable work items)
                    │       ├── Definitions / decisions
                    │       ├── one Conversation ──> Messages
                    │       ├── Analysis turns (existing execution records)
                    │       │      ├── Steps (readable projection of trace)
                    │       │      ├── Evidence
                    │       │      └── Verification results
                    │       ├── Findings ──answer──> Questions
                    │       │       └── supported by──> Evidence / checks
                    │       └── Artifacts (charts, tables, reports)
                    └── Handoff view/export (projection of current work)

Project ──curates selected Findings, Artifacts, Definitions (future consumer)
Environment / pane / surface ──references existing work; owns no analysis
```

Cardinalities describe the initial desktop model. One Investigation has one
stable Workspace and, for workspace-scoped analysis, exactly one Conversation.
It may span many Questions and Analysis turns. A turn may address one or more
Questions. A Finding answers a Question and may cite multiple Evidence items
and checks. General chat remains unbound unless the user explicitly chooses a
data scope. A future multi-workspace Investigation would require an explicit
product decision.

## Primitive contracts

| Primitive | Canonical responsibility | Ownership and boundary |
| --- | --- | --- |
| **Workspace** | Identifies the explicit local analysis boundary and its current catalog, context, and revision. | Stable folder identity; not a window, environment, or pane. Every data-backed operation names its workspace. |
| **Source** | Identifies a cataloged file, sheet, or supported document within a Workspace. | Use workspace-relative identity plus source kind/sheet as needed; a display name alone is not identity. The source is a reference to user data, not a copy. |
| **Source revision** | Records what was observed about a Source at a specific workspace revision. | Immutable run-time reference to the relevant source identity, file revision/profile, and observed schema/coverage. A changed file creates new observed state; it never rewrites old run lineage. |
| **Investigation** | Owns the identity and lifecycle of one bounded analyst effort. | Durable aggregate with an ID, one Workspace, title, status, timestamps, and links to its Conversation, brief, questions, decisions, turns, findings, and handoff. It indexes linked records by ID; it does not duplicate their payloads. |
| **Brief** | Keeps the goal, audience, desired output, and constraints legible. | Versioned value owned by an Investigation. It may be model-proposed and analyst-edited; changing it never changes the original message or rewrites prior runs. |
| **Source scope** | Says which cataloged sources are intended for this Investigation or Question. | Explicit include/exclude choices. Every Analysis turn records an immutable source-scope snapshot of what it actually used, including relevant exclusions. Selecting a source is not itself evidence that it was read. |
| **Question** | Represents the root request or one answerable part of it. | Durable Investigation child with parent/dependency links only when useful, editable text, order, relevant source mapping, and separate work/coverage state. A model proposal is editable, not an accepted plan by default. |
| **Definition / decision** | Records a meaning or constraint that could change an answer, such as metric grain or treatment of missing values. | Versioned Investigation context with value, scope, authority (observed, model-proposed, or analyst-confirmed), and history. A proposed meaning never silently becomes confirmed. |
| **Conversation** | Holds the interaction through which the analyst asks, clarifies, steers, and reviews. | Owns the message sequence and its presentation state. In the initial workspace model, one conversation is linked to exactly one Investigation; messages and answers are not copied into a second transcript. |
| **Message** | Records one user, assistant, or system interaction. | Conversation-owned. A message may link to a run or result, but the link does not make the message the canonical owner of execution or evidence. |
| **Analysis turn / run** | Records one bounded execution of an analytical request. | The existing persisted `AnalysisTurn` remains canonical for execution: input, workspace snapshot, interpretation/contract, plan, trace, result, evidence, verification, and rerun lineage. Product-level links associate it with an Investigation and one or more Questions without changing the engine loop. |
| **Step** | Makes consequential progress understandable to an analyst. | Initially a concise projection of existing run state and trace, not a second event log or a record of private model reasoning. Persist a separate Step only if it later needs independent identity or lifecycle. |
| **Evidence** | Captures an observation or computation that can support or challenge a claim. | Immutable and linked to its source/revision and producing run/tool. Evidence is not answer prose, and its payload remains owned by the existing evidence/run record. |
| **Verification result** | States what a specific deterministic or review check tested and its outcome. | Linked to the run and target it checks (evidence, Finding, or Artifact). A pass establishes only the stated check; it does not certify interpretation or business truth. Failed checks remain visible. |
| **Finding** | States a concise, reviewable answer to a Question. | Durable, independently addressable claim with supporting run/evidence/check links, caveats, coverage state, and revision history. Editing its wording never severs provenance. |
| **Artifact** | Makes a selected result reusable outside the immediate answer. | A versioned chart, table, or report linked to its Finding(s), run(s), source scope, and relevant evidence. A new result creates a new version; historical output is not silently rewritten. |
| **Readiness assessment** | Summarizes whether observed inputs appear sufficient for a particular Question. | A derived, evidence-linked assessment—not a promise that data is correct or complete. It distinguishes available, unsupported, missing, and uninspected information. |
| **Handoff** | Presents the current answer for a requester or decision-maker. | A generated view/export of the Investigation’s brief, Findings, Artifacts, method, checks, provenance, caveats, and unresolved Questions. It is a projection, not another source of truth. |
| **Project** | Curates reviewed results into durable reusable knowledge. | Optional downstream consumer of selected Findings, Artifacts, Definitions, and open Questions. Existing Projects remain user-authored notes until evidence-backed curation is built. |
| **Environment / pane / surface** | Organizes where existing work appears in the desktop. | Presentation references only. Layout never grants data access, changes workspace scope, or owns analytical facts. |

### Work state is not answer coverage

Question progress needs two related but distinct answers:

- **Work state:** `open`, `ready`, `running`, `waiting_for_input`, `blocked`,
  or `deferred`—what is happening next.
- **Coverage state:** `unanswered`, `partial`, `answered`, or `unsupported`—
  how much of the requested question the available Findings support.

The UI may summarize these into a compact label, but must not confuse a
finished run with an answered Question. `unsupported` means the available
scope or methods cannot support an answer and must include the known reason;
it does not mean the model failed to produce prose.

Investigation lifecycle is separate again. It can be marked **complete** only
when every in-scope Question is answered or its remaining gap is explicitly
recorded and accepted/deferred, no run is still active, and no consequential
decision is silently pending. A complete Investigation may still disclose
partial or unsupported coverage. It can be reopened without erasing its
history. Archiving is organizational, not deletion.

## Cross-cutting invariants

1. **One owner per fact.** Conversation owns messages; AnalysisTurn owns
   execution; evidence/check records own their outputs; Investigation links
   them. Do not copy their payloads into a parallel aggregate.
2. **Scope is explicit and historical.** Source discovery does not grant
   access. Every run records the actual workspace/source revision and scope;
   subsequent edits never rewrite that snapshot.
3. **General is not a hidden workspace.** General chat is outside this model
   unless the analyst explicitly chooses a scope. Whole-disk search is a
   separate opt-in direction, not an automatic extension of mounted-folder
   analysis.
4. **Interpretation is attributable.** Observed, computed, model-proposed,
   and analyst-confirmed information remain distinguishable.
5. **Reruns append.** A rerun is a new Analysis turn linked to its predecessor;
   it preserves the prior scope, method, result, and evidence.
6. **Coverage is honest.** Findings answer named Questions; missing data,
   failed checks, unsupported inputs, and unresolved assumptions remain visible
   in the results and handoff.
7. **The engine stays the execution authority.** Investigation objects and
   their presentation can organize and steer existing runs without changing
   SQL/Python/document execution behavior by implication.
8. **Extensions remain optional.** CLI/FellaDB may provide a future source
   provider. The desktop works without it. Projects consume selected results;
   they do not become required for analysis.

## Issue sequence mapped to the model

These entries map the 42 active implementation issues to the primitive or
invariant they advance. The issue acceptance criteria remain authoritative for
the implementation slice; this map is the shared conceptual contract.

| Issue | Model contribution |
| --- | --- |
| [#182 Durable Investigation record](https://github.com/Avijit-Kumar-GIT/fella/issues/182) | Add the stable, independently loadable aggregate identity and lifecycle metadata. |
| [#183 Source scope per run](https://github.com/Avijit-Kumar-GIT/fella/issues/183) | Preserve the immutable source set and exclusions actually used by each run. |
| [#184 Editable brief](https://github.com/Avijit-Kumar-GIT/fella/issues/184) | Give each Investigation a compact, editable goal/outcome/constraint value. |
| [#185 Brief updates](https://github.com/Avijit-Kumar-GIT/fella/issues/185) | Propose and review a new brief version when direction materially changes. |
| [#186 Separate messages and runs](https://github.com/Avijit-Kumar-GIT/fella/issues/186) | Link canonical AnalysisTurn IDs from an Investigation and open the run record from its transcript message. |
| [#187 Automatic Investigation](https://github.com/Avijit-Kumar-GIT/fella/issues/187) | Automatically resolve/create one Investigation per workspace conversation, keyed by its first user message. |
| [#188 Conversation association](https://github.com/Avijit-Kumar-GIT/fella/issues/188) | Store the stable Conversation ID on its Investigation and navigate back to the canonical transcript. |
| [#189 Editable Question plan](https://github.com/Avijit-Kumar-GIT/fella/issues/189) | Let analysts edit Question text and ordering without rewriting history. |
| [#190 Usable sources in context](https://github.com/Avijit-Kumar-GIT/fella/issues/190) | Show catalog availability/readiness inside the Investigation workflow. |
| [#191 Durable Question records](https://github.com/Avijit-Kumar-GIT/fella/issues/191) | Persist root and decomposed Questions independently of messages. |
| [#192 Investigation definitions](https://github.com/Avijit-Kumar-GIT/fella/issues/192) | Preserve attributable semantic definitions and assumptions. |
| [#193 Data readiness and gaps](https://github.com/Avijit-Kumar-GIT/fella/issues/193) | Surface evidence-backed limits before or during planning. |
| [#194 Per-investigation source choices](https://github.com/Avijit-Kumar-GIT/fella/issues/194) | Make include/exclude decisions explicit without changing historical scope. |
| [#195 Schema and profile inspection](https://github.com/Avijit-Kumar-GIT/fella/issues/195) | Bring relevant existing schema/profile observations into analyst context. |
| [#196 Question source links](https://github.com/Avijit-Kumar-GIT/fella/issues/196) | Connect intended source sets to Questions and actual sources to runs. |
| [#197 Method and evidence](https://github.com/Avijit-Kumar-GIT/fella/issues/197) | Make the existing run method and its evidence inspectable in context. |
| [#198 Analyst steering](https://github.com/Avijit-Kumar-GIT/fella/issues/198) | Make scope, definitions, and rerun intent visible without engine changes. |
| [#199 Question status](https://github.com/Avijit-Kumar-GIT/fella/issues/199) | Track work state and answer coverage distinctly and explain blockers. |
| [#200 Progress timeline](https://github.com/Avijit-Kumar-GIT/fella/issues/200) | Present meaningful run progress from existing runtime state, not raw logs. |
| [#201 Run associations](https://github.com/Avijit-Kumar-GIT/fella/issues/201) | Link existing turns to the Investigation and Questions they address. |
| [#202 Consequential ambiguity](https://github.com/Avijit-Kumar-GIT/fella/issues/202) | Continue with reviewable low-impact assumptions; ask on material choices. |
| [#203 Answers by Question](https://github.com/Avijit-Kumar-GIT/fella/issues/203) | Navigate from each Question to its run-backed answer and coverage. |
| [#204 Visualization provenance](https://github.com/Avijit-Kumar-GIT/fella/issues/204) | Link saved visuals to the Finding and immutable source run. |
| [#205 Durable Findings](https://github.com/Avijit-Kumar-GIT/fella/issues/205) | Save editable, evidence-linked claims independently of message prose. |
| [#206 Focused QA review](https://github.com/Avijit-Kumar-GIT/fella/issues/206) | Review method, grain, filters, joins, and edge cases against stated scope. |
| [#207 Coverage gaps](https://github.com/Avijit-Kumar-GIT/fella/issues/207) | Keep unanswered, partial, unsupported, and deferred Questions visible. |
| [#208 Verification context](https://github.com/Avijit-Kumar-GIT/fella/issues/208) | Show each check beside its target and state exactly what it establishes. |
| [#209 Preserve reruns](https://github.com/Avijit-Kumar-GIT/fella/issues/209) | Append attempts with the exact context and lineage that produced them. |
| [#210 Caveats and assumptions](https://github.com/Avijit-Kumar-GIT/fella/issues/210) | Make Finding limitations editable and carry material ones into handoff. |
| [#211 Finding history](https://github.com/Avijit-Kumar-GIT/fella/issues/211) | Version claims and compare changes in scope, definitions, and method. |
| [#212 Completion by coverage](https://github.com/Avijit-Kumar-GIT/fella/issues/212) | Separate Investigation completion from run completion and disclose gaps. |
| [#213 Operational Sources](https://github.com/Avijit-Kumar-GIT/fella/issues/213) | Make source information useful in context or remove a passive destination. |
| [#214 Provenance-rich export](https://github.com/Avijit-Kumar-GIT/fella/issues/214) | Generate a concise report from current Findings, methods, checks, and caveats. |
| [#215 General boundary](https://github.com/Avijit-Kumar-GIT/fella/issues/215) | Keep unscoped General conversations out of workspace Investigations. |
| [#216 Results view](https://github.com/Avijit-Kumar-GIT/fella/issues/216) | Present the current Investigation clearly without requiring transcript reading. |
| [#217 Provider-neutral source interface](https://github.com/Avijit-Kumar-GIT/fella/issues/217) | Keep desktop source operations independent of optional CLI/FellaDB. |
| [#218 Engine regression coverage](https://github.com/Avijit-Kumar-GIT/fella/issues/218) | Prove product-layer work preserves current execution behavior. |
| [#219 Multi-question validation](https://github.com/Avijit-Kumar-GIT/fella/issues/219) | Test decomposition through coverage, QA, and coherent handoff. |
| [#220 Low-ceremony validation](https://github.com/Avijit-Kumar-GIT/fella/issues/220) | Preserve a direct path for a simple one-question analysis. |
| [#221 Projects boundary](https://github.com/Avijit-Kumar-GIT/fella/issues/221) | Keep Projects a future curator of selected model primitives. |
| [#222 Missing/unsuitable data validation](https://github.com/Avijit-Kumar-GIT/fella/issues/222) | Validate explicit limitations without fabricating readiness or answers. |
| [#223 History migration](https://github.com/Avijit-Kumar-GIT/fella/issues/223) | Associate existing records idempotently without losing or duplicating history. |

## First implementation boundary

The first slice adds only the durable Investigation record: generated stable
ID, explicit Workspace ID, title, lifecycle status, and timestamps, with
independent create/list/load operations. Workspace analysis now automatically
resolves or creates that record using the first user message ID as an
idempotency key; ordinary asks require no setup step. A later Conversation
association slice will make the link explicit and navigable. This sequencing
keeps the engine’s model-directed execution and its stored run payloads
unchanged.

The first shipped slices now also retain the source scope on each completed
analysis turn and an editable current Brief on an Investigation. The Brief is
seeded from the original request when available and stores only a message ID
reference; editing its goal, audience, desired output, or constraints never
edits the Conversation transcript. Clear revision language and explicit source
constraints can surface an editable proposal in the Investigation view. The
analyst applies or dismisses it; neither the brief nor its review cursor changes
until that choice is saved. Routine follow-up questions are not proposals, and
the original source message remains immutable. Investigations now retain only
IDs for their linked AnalysisTurns; conversation messages remain unchanged and
open the canonical execution record by its existing turn ID. Workspace asks
automatically ensure their Investigation from the conversation's first real
question; reopening that same transcript resolves the existing record rather
than creating another one. The Investigation stores the stable Conversation ID
and links back to its existing transcript instead of copying messages. Older
records can recover that link from the first-question message ID. General
conversations remain outside this flow.

Each Investigation now stores an ordered, editable Question plan. Analysts can
edit the main Question and add, rename, reorder, or remove its subquestions
without changing the source message or prior AnalysisTurns. The next workspace
ask receives the latest saved plan as context while its transcript message
remains exactly what the analyst sent; unanswered Questions are explicitly
treated as work to do, not established findings. Legacy Investigations surface
their existing goal as the initial Question when upgraded. Each Question now
has its own stable record in the local Questions store, with an Investigation
ID, parent ID, order, lifecycle status, and timestamps; the Investigation
keeps the ordered IDs and presents the root request separately from its child
Questions. Older flat plans migrate once, using their first item as the root
and linking the rest to it; an empty legacy plan recovers the root from the
current brief goal. Fella can ask the configured model for a decomposition,
but suggestions are editable drafts: only an explicit Save writes child
records. Removed Questions retain their identity and lifecycle state rather
than becoming orphaned history.

Investigation Definitions and Assumptions are now independently persisted,
analyst-authored context rather than transcript text or folder-wide semantic
memory. Each entry has a stable ID, an explicit kind, an active/removed state,
and revision history; edits and removal never rewrite prior AnalysisTurns. The
active set is passed to later workspace runs as interpretive guidance, clearly
separated from evidence and the immediate request. Every Investigation-bound
run stores a self-contained snapshot of the exact entries and revisions it
received, including an explicit empty snapshot, so the run view can show what
context was used after the Investigation changes. Unscoped General asks do not
inherit this context.

The Investigation surface now includes a compact view of its workspace's
current catalog snapshot: queryable tables, readable documents, and files the
catalog skipped. Skipped entries distinguish unsupported formats from files
that could not be read or loaded, using the catalog's recorded reason. An
analyst can inspect a source's type, shape, ingest notes, document synopsis, or
a small table preview inline without leaving the Investigation. This is
readiness context only: it does not select a source scope or claim that a
source has already been used in an Analysis turn; explicit Question-level
source scope and run provenance remain separate primitives.

The readiness view now exposes each tabular source's observed field profile:
type, missing-row count/percentage, distinct count, observed minimum/maximum,
and ingest caveats, alongside the sample preview and skipped-file reasons.
An Investigation-level filter searches source names and paths as well as field
names, types, common values, and ingest notes; matching fields are brought
forward in the selected source's profile. This is a view-only focus, separate
from the persisted source-selection controls.
These are facts from the currently loaded catalog snapshot, not a verdict that
the source covers the requested population or time period. The profile helps
the analyst notice concrete gaps before planning; a Question-specific semantic
assessment still needs to identify which fields and coverage the request
requires, and must cite observed data rather than infer completeness.

Each Question can now inherit the Investigation's file scope or narrow it to
an explicit source set. The engine intersects that mapping with the current
Investigation boundary, so a Question can never expand what the Investigation
allows. An analyst can launch a saved Question directly from the plan; the
canonical AnalysisTurn snapshots that Question ID and effective source set,
while its existing evidence-derived source-scope record identifies which files
the run actually used. Later edits to either mapping do not rewrite earlier
runs. This makes intended inputs and observed evidence inspectable without
claiming that selecting a source proves it supports the answer.

An AnalysisTurn's saved-run view now includes the evidence payload it owns, not
only the conversation's current answer: source/table references, summaries,
queries or calculation code, and recorded result samples stay attached to the
run that produced them. This lets an analyst reopen an earlier run after later
conversation turns without accidentally inspecting the newer turn's evidence.

Question analysis now has an explicit pre-run review. Before any execution, the
analyst can verify the exact Question, broader plan context, effective source
scope and files, and active definitions/assumptions; unsaved Question or
context edits are saved only after confirming the run. The saved AnalysisTurn
continues to expose its intended source-selection snapshot and definition
revisions alongside the evidence of what was actually read. “Rerun” on an old
turn repeats that turn's original request and context; to apply corrected
Question, source, or definition settings, the analyst launches the Question
again from the Investigation, creating a fresh run without changing engine
computation logic.

Each Question now carries a durable coverage state for its latest run: not
started, running, answered, incomplete, blocked, or outdated. Terminal states
come from the AnalysisTurn's actual clarification and verification outcome,
not from optimistic UI completion; incomplete and blocked states retain a
short reason. Editing a Question, source boundary, or saved definition marks
its previous answer outdated, while the canonical AnalysisTurn remains intact
as historical evidence.

Canonical AnalysisTurns are now indexed from both sides of the analyst's work:
an Investigation links its saved runs, and each Question keeps the runs that
address it while retaining a separate latest-run pointer for coverage status.
The Investigation view can reopen a run from its run list or from a Question,
inspect its saved answer, method, trace, and checks, and add or remove links to
other active Questions. A run launched from one Question keeps that original
Question and context snapshot even if it is later linked to additional
Questions. Only persisted AnalysisTurns appear in this history; starting a run
updates live coverage state but does not create a link to a record that has
not yet been saved. Legacy Questions with only a latest-run pointer continue to
surface that run in history.

The analyst's ambiguity policy is also explicit: resolve ordinary uncertainty
from evidence and saved definitions; for a low-impact choice with a reasonable
default, record the assumption and proceed. Ask the analyst only when multiple
plausible choices remain and could materially change scope, population,
measure, method, or conclusion. A run exposes its recorded assumptions apart
from observed evidence. An analyst may promote one into the editable
Investigation context, correct it, and save a new revision for later runs;
the original AnalysisTurn remains an unchanged historical record.

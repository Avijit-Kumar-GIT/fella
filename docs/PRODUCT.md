# Fella product

See [Product vision](PRODUCT-VISION.md) for the north-star story, analyst
workflow, foundational primitives, trust model, and target experience. This
document remains the reference for current shipped scope.

Fella is an opinionated analytics harness built around conversation. Its core
job is to help a data analyst move from a question about an explicitly mounted
folder to an evidence-backed answer. The longer-term product target is to make
the full investigation—from data discovery and decomposition through QA,
visualization, and handoff—feel like one steerable workflow. A person can also
ask a general question without mounting a folder, then use the same
conversation to inspect and analyze local data when a question depends on it.
The workspace organizes sources and context; it is not the product's purpose.

## The analyst's loop

Fella should help a model work the way an analyst approaches unfamiliar data:

1. Inspect the available sources, structure, labels, quality, and relevant
   context.
2. Interpret and, where useful, decompose the question into smaller questions.
3. Investigate the data, refine the interpretation, and repeat inspection as
   needed.
4. Ask the user when an unresolved choice would materially change the result.
5. Compute with an appropriate read-only method, check the result, and explain
   the evidence, assumptions, and limits.

This is an adaptive, model-directed loop—not a required sequence of tool calls.
The model interprets human language and chooses how to investigate; Fella's
runtime provides the observations, execution tools, continuation, and scoped
checks that make that work useful and inspectable.

## Product commitments

- **Consistency:** preserve definitions, source scope, and relevant context
  across turns without treating old prose as new evidence.
- **Correctness:** make a useful, well-supported answer as often as possible.
  A valid query or a successful replay is evidence about execution, not proof
  that the interpretation was right.
- **Efficiency:** reduce irrelevant context and repeated work while spending
  enough reasoning and tool calls to answer well. Few calls are not a goal by
  themselves.
- **Read-only analysis:** the agent can inspect mounted data and calculate
  from it, but cannot edit, move, or delete workspace files. The user may
  explicitly edit the root `fella.md` context file in the app.
- **Local control:** files and workspace state stay on the user's computer.
  When a user asks a connected model a question, that provider receives the
  prompt and any context or tool results used to answer it.

## Conversation and Question contract

A Conversation is the place for ordinary asks and follow-ups. In a mounted
Workspace, its first user request creates the Workspace's Investigation and
links that Investigation to the same Conversation. Later freeform messages
remain ordinary asks in that Conversation: they preserve conversational
continuity, but do not silently create saved Questions, select a Question, or
claim that a Question-specific analysis run happened. The original request and
the saved Investigation context remain available to the model as background;
each reply should address the user's immediate message rather than combine
sibling Questions into one request.

The explicit **Analyze** action on a saved Question is different: it starts a
run linked to that one Question. For example, “What were total net sales in
Q3?” followed by “What about Q4?” is a pair of freeform Conversation asks.
Choosing **Analyze** on “What was total net sales in Q4?” from the saved
Question plan runs that Question and records its Question/run relationship.
General chat without a mounted Workspace remains unbound and does not create an
Investigation. This keeps chat low-ceremony without weakening the durable
Question and provenance model.

## Shipped file and runtime boundaries

The mounted folder is the input boundary. Fella does not edit, move, or delete
source files. Supported spreadsheets are ingested sheet-by-sheet as queryable
tables; supported delimited and JSON records are queryable tables; supported
text and extractable-text PDFs are searchable/readable document sources. A
catalog entry, an analysis input, a UI preview, and an export are distinct
capabilities:

| File/source type | Catalog and analysis | In-app preview | Export or limitation |
| --- | --- | --- | --- |
| CSV, TSV, TAB; JSON, NDJSON, JSONL | Tabular sources are loaded into the local SQLite analytics database; query through bounded read-only SQL. | Tabular source preview shows up to five rows. | Chart data can be downloaded as CSV; source files are not rewritten. |
| XLSX, XLSM, XLSB, XLS | Supported sheets are ingested as individual queryable tables. | A sheet's table can use the same five-row preview; this is not a full workbook viewer. | No workbook round-trip/editing. |
| TXT, TEXT, MD, MARKDOWN, LOG; PDF | Text is searchable/readable as documents. PDF requires an extractable text layer; there is no OCR. | Document reading is through analysis/source details, not a general full-file viewer. | No DOCX or PPTX ingestion or viewer is shipped. |
| PARQUET, PQ | Catalog recognizes the extension, but analysis requires a custom build with the optional DuckDB feature; standard release builds do not include it. | No tabular preview in the standard release. | Not supported in standard release builds. |

Investigation handoff exports a reviewable Markdown report. Chart artifacts can
be downloaded as PNG and their data as CSV. Fella does not currently provide
general-purpose document previews or source-file editing/round-tripping.
Provider calls are made from the local Rust sidecar directly to the provider
selected by the user. Prompts and relevant context or tool results (which can
include document text or data rows) may leave the device. SQL is single-
statement and read-only, query results are bounded, and Python runs in a
sandbox without filesystem, network, or subprocess access. Provider retention
and training practices remain governed by that provider.

The desktop also offers a separate, user-started system terminal at the
selected Workspace. It runs outside Fella with the user's normal system
permissions, is not confined to that folder, and is not available to the model.
Fella does not read its input/output or control the terminal session.

Recipient collaboration is not part of the shipped handoff. The current
portable report and chart/data exports should be tested with actual recipients
first. Hosted sharing, multi-user permissions, and approval workflows remain
deferred unless observed handoff work establishes a concrete need. Likewise,
connectors, unattended scheduling, broad web research, multi-agent
orchestration, autonomous source-file writing, enterprise governance, and a
full code-first notebook remain outside the committed scope absent validated
demand and an explicit product/safety decision.

## Current scope

The maintained desktop app is Electron with a Svelte UI and a local Rust
analytics sidecar. Ask supports model-knowledge answers without a workspace and
model-directed analysis of supported local files. SQL and sandboxed Python are
both first-class computation routes; forecasts and scenarios are allowed when
the available data and stated assumptions support them. Charts, document
search, clarification, conversation continuity, and inspectable analysis
details are part of the same conversation flow.

Web research is **not shipped** and remains on the backburner. Fella does not
silently send workspace contents to web search. The desktop app is not a
general-purpose file manager, shell automation tool, notebook, or multi-agent
orchestrator. A read-oriented analyst CLI, with FellaDB as its proposed local
filesystem search capability, is an exploratory direction and is not shipped.
See the [analyst workflow audit](ANALYST-WORKFLOW-AUDIT.md) for how the current
app maps to the full data analyst lifecycle and where the product experience
still has gaps.
See [Architecture](ARCHITECTURE.md) for the current implementation,
[Roadmap](ROADMAP.md) for open work, and [Stack vision](STACK-VISION.md) for
the proposed path from files to reusable project knowledge.

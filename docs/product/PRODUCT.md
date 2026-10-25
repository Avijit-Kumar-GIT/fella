# Fella product

See [Product vision](./PRODUCT-VISION.md) for the north-star story, analyst
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

## Conversation and Investigation contract

A Conversation is the place for ordinary asks and follow-ups. Creating a new
Conversation does not create an Investigation. The user creates an Investigation
explicitly; that action starts and links a Conversation for the Investigation.
This keeps ordinary chat lightweight while giving longer analytical work a
named, persistent home.

Later freeform messages remain ordinary asks in the same Conversation: they
preserve continuity, but do not silently create saved Questions, select a
Question, or claim that a Question-specific run happened. Investigation context
may inform the answer, but each reply should address the immediate request
instead of merging sibling Questions.

The explicit **Analyze** action on a saved Question starts a run linked to that
Question. Deleting a Conversation also deletes its linked Investigation and
transcript. Explicitly saved analytical results, their supporting run records,
reports, and revisions are retained in the Workspace, as explained in the
confirmation dialog. General chat without a mounted folder remains unbound.

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

Recipient collaboration is not part of the shipped handoff. The current
portable report and chart/data exports should be tested with actual recipients
first. Hosted sharing, multi-user permissions, and approval workflows remain
deferred unless observed handoff work establishes a concrete need. Likewise,
connectors, unattended scheduling, broad web research, multi-agent
orchestration, autonomous source-file writing, enterprise governance, and a
full code-first notebook remain outside the committed scope absent validated
demand and an explicit product/safety decision.

## Current scope

The desktop app is Electron with a Svelte UI and a local Rust analytics
sidecar. Chat supports model-knowledge answers without a folder and
model-directed analysis of supported local files. Notebook is an optional
arrangement of Chat and the active Investigation or document in the same
window. Files and Reports are separate sidebar destinations. Investigations
have Brief, Sources, Questions, Analysis, and Results sections. See the
[interaction model](./INTERACTION-MODEL.md) for current navigation and
terminology.

SQL and sandboxed Python are first-class computation routes; forecasts and
scenarios are supported when the available data and stated assumptions allow
them. Generated presentations use the validated json-render catalog and may
combine narrative, values, tables, and visualizations as appropriate. The
plain answer and saved analytical results remain available if a presentation
cannot be composed or displayed.

Web research is **not shipped**; workspace contents are not sent to a web
search service. Fella is not a general-purpose file manager, shell, notebook
kernel, or multi-agent orchestrator. The former terminal/shell UI was removed.
Provider selection and credentials are user-controlled. See
[Architecture](../architecture/ARCHITECTURE.md) for implementation boundaries
and [Roadmap](./ROADMAP.md) for open work.

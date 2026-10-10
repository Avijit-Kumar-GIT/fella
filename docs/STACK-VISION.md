# Fella filesystem-to-analysis stack

This document records a product direction for connecting filesystem discovery,
interactive analysis, and durable project knowledge. It is exploratory, not a
description of shipped behavior or a release commitment. See
[`PRODUCT.md`](PRODUCT.md) and [`ARCHITECTURE.md`](ARCHITECTURE.md) for the
current product and implementation.

## Product shape

Fella should help an analyst move from files they already have to useful,
inspectable analysis they can keep:

```text
Selected files and folders
        ↓
Fella CLI: find, catalog, profile, preview, query
        ↓
Fella workspace: investigate with a model and inspect the work as it happens
        ↓
Projects: curate findings, visualizations, definitions, and open questions
```

The CLI is a way to use filesystem and analyst capabilities, not another
product layer. A shared source and provenance model should connect the CLI,
desktop harness, and Projects without making their scopes interchangeable.

## Filesystem and analyst CLI

The proposed Fella CLI is a local, analyst-oriented command surface for
read-only operations such as:

- searching paths and file contents;
- cataloging supported, unsupported, excluded, and inaccessible files;
- profiling file structure and data quality;
- previewing a bounded sample;
- running read-only queries against an explicitly selected analysis scope.

**FellaDB** is the proposed persistent filesystem index/search capability
within this CLI, not a separate companion app or a second analytical database.
The initial search-engine direction is FFF. FFF should handle file/path and
content search; Fella's existing catalog, ingestion, profile, and query engine
should remain responsible for understanding supported data formats. Do not
reimplement a general-purpose CSV toolkit or maintain competing indexes when
the existing components can serve the need.

Keep two scopes explicit:

- **Personal library:** locations the user explicitly selects for broad file
  discovery. The CLI can search this scope; the desktop may explicitly invoke
  it for a General conversation without binding General to a repository.
  Whole-disk coverage is an opt-in expansion, not a default.
- **Analysis workspace:** a deliberately selected folder or source set used
  for profiling and computation.

Personal-library search must never inherit the currently focused repository.
The user explicitly selects the personal-library scope for a General question;
General remains unbound, and the selected repository's catalog and memory do
not change. Finding a file must not silently add it to an analysis workspace.
The user chooses when a search result becomes an analytical source. Source
files remain read-only; writing or exporting derived data, if added, must be a
deliberate separate action.

The CLI should support human-readable output and a stable structured format
for scripts and Fella's desktop integration. The desktop should consume a
typed local interface or shared core rather than parse terminal presentation.
The current Rust process is an Electron-managed JSON-lines sidecar, not a
user-facing CLI; making it independently usable is future work.

## Fella analysis workspace

The desktop harness remains the primary conversational analysis experience.
The model continues to interpret questions and direct an adaptive
investigation; Fella's runtime continues to own scope, bounded execution,
evidence, persistence, and verification.

The important product gap is user control during the investigation. The target
is a live, notebook-inspired run view that makes these things visible and
steerable as they happen:

1. the question Fella believes it is answering;
2. the selected sources and fields;
3. assumptions or semantic choices that could change the answer;
4. the queries or calculations and useful result previews;
5. verification findings, caveats, and the conclusion.

Users should be able to interrupt a run, correct a material choice, inspect a
result, and keep a useful analytical step. Routine read-only inspection need
not require approval at every action. This is inspiration from notebooks'
inspectable intermediate work, not a commitment to reproduce Jupyter's
general-purpose kernel, package ecosystem, or cell model.

## Projects and reusable knowledge

Projects are the durable knowledge layer, not a general canvas or an automatic
dump of every conversation. The direction is to let people curate useful
findings and visualizations, each linked to the analysis and sources that
support it. A saved finding should preserve its scope, definitions,
assumptions, and provenance, and make clear whether it is draft, reviewed, or
stale. Projects can later host dashboard-like views for stable metrics with a
clear refresh basis; exploratory answers should not be forced into dashboard
tiles.

Projects are currently local, user-authored notes. This direction does not
imply team sharing, enterprise governance, or refreshable dashboards are
implemented.

## Shared meaning and provenance

The missing connective tissue is a source and semantic layer that can travel
across the CLI, analysis runs, and Projects. It should distinguish automatic
hints from user-confirmed meaning and preserve links such as:

```text
file identity and revision
  → selected fields, rows, and scope
  → confirmed definitions and assumptions
  → query or calculation
  → evidence, verification, and visualization
  → curated Project finding
```

Fella already creates revision-bound source profiles, cautious candidate
field roles and relationships, and workspace-scoped semantic memory. The
direction is to make the useful parts inspectable and reusable, not to treat
an inferred role or a successful query replay as proof of business meaning.

## The current Sources view

The desktop Sources view currently provides a read-only catalog with filtering,
pagination, file metadata, skipped-file reasons, a small table preview, and an
action to open a source beside a conversation. It is more than a static list,
but it does not provide a substantive analysis workflow of its own.

The product direction is to move broad catalog, search, profile, and preview
work into the CLI, where filesystem-level access and analyst commands belong.
The harness should keep source access where it serves the analysis directly:
scope selection, evidence/source links, and contextual previews for a specific
conversation or result. This is a direction to evaluate, not an instruction to
remove the current view before the CLI and handoff experience exist.

## Boundaries and open decisions

- FellaDB, FFF, the CLI, and the desktop must not silently broaden one
  another's filesystem scope.
- Fella's analysis remains read-only with respect to source files.
- Local indexing does not mean model inference is local. A selected hosted
  provider may receive the question and relevant context or file-derived
  results used in an answer.
- Decide how the CLI shares the Rust core and persistent FFF index with the
  desktop, including installation discovery, lifecycle, and compatibility.
- Decide which source/profile metadata is durable, user-editable, and
  revision-bound, and how changes invalidate saved evidence.
- Validate FFF coverage, resource use, exclusions, and packaged behavior on
  supported operating systems before promising whole-disk search.
- Keep embedding-based search optional and additive; it is not required for
  the initial FellaDB capability.

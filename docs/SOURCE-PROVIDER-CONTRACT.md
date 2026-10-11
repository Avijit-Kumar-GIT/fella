# Source-provider contract

**Status:** Proposed compatibility contract, not an implemented plugin API.
The current desktop uses its built-in Rust workspace engine; no CLI, FellaDB,
or external source provider is required to open and analyze a local folder.
This contract records how future discovery tooling could exchange source
metadata without becoming a prerequisite or silently changing the analysis
engine.

## Boundary

A source provider may expose explicitly scoped discovery and read operations:
catalog, source profile, bounded preview, and refresh. Its responses are
structured data, never terminal-formatted text. The current Rust workspace
engine remains the provider used by the desktop and the authority for ingestion,
query execution, evidence, and analysis. A future provider returning search or
catalog results does not thereby become an analytics engine or grant those
results to a conversation.

The UI may show a discovered file as a candidate. The user must explicitly
choose when and under what workspace scope to use it. General conversations
remain unbound unless the user explicitly selects a data scope. Selecting a
result must not mount a folder, widen an existing workspace, or change the
source set of a saved run.

## Provider-neutral data

The wire contract should use versioned, serializable records with these
conceptual fields. Names and encoding may vary by transport, but their meaning
must remain stable:

| Record | Required meaning |
| --- | --- |
| Provider descriptor | Stable provider identifier, protocol version, availability, and the operations it supports. |
| Scope reference | Provider identifier plus an opaque identity for the explicitly selected root or collection. It is not a filesystem permission by itself. |
| Source reference | Scope reference, stable provider-local source identifier, relative display path, format, and optional member such as a sheet. A display name alone is not identity. |
| Catalog snapshot | Scope reference, opaque revision token, observation time, sources, and skipped/unavailable entries with reasons. |
| Source profile | Source reference and revision, observed schema/profile fields, and any interpretation or ingest caveats. |
| Preview | Source reference and revision, bounded structured rows or content excerpt, and truncation/limit metadata. |
| Search match | Source reference, match kind, bounded excerpt or path context, and the index/catalog freshness or revision when known. It is a candidate reference, not a grant to read or analyze the source. |

Revision tokens are opaque and compared only within their provider and scope.
Every profile or preview states the revision it describes. A consumer must not
silently present an old preview as current after refresh or file change. Source
identities and relative paths stay distinct when filenames collide.

## Operations and semantics

A provider can advertise any supported subset of these operations:

- `catalog(scope)` returns a structured snapshot for an already selected scope.
- `search(scope, query, filters?, limit)` returns bounded, typed matches only
  from that scope. Matches identify their sources and disclose index freshness
  where the provider can determine it.
- `profile(sourceRef, revision?)` returns observed metadata for that exact
  source; a stale requested revision is reported rather than silently replaced.
- `preview(sourceRef, limit, revision?)` returns a bounded read-only sample or
  excerpt. The provider enforces a documented maximum and returns the actual
  revision used.
- `refresh(scope)` rescans only that scope and returns the new snapshot and
  revision.

Operations report structured errors, at minimum distinguishing unsupported
format, inaccessible scope/source, missing source, stale revision, unavailable
provider, and transient failure. An unsupported operation is explicit; callers
must not infer that catalog access implies search, preview, or query capability.

Every request is bound to one explicit scope. Providers read only user-selected
locations and remain read-only. Catalogs report skipped and inaccessible items
where possible so partial visibility is not mistaken for complete coverage.
Provider discovery never inherits the currently focused repository or
conversation, and a provider becoming unavailable must not cause a fallback to
a broader filesystem scope.

Query execution is intentionally outside this discovery contract. A future
provider that can supply data to analysis needs a separately specified,
permission-preserving ingestion/execution integration and matching provenance;
this document does not authorize changing the current analytics engine.

## Mapping to the current desktop

The built-in Rust workspace engine already provides the local implementation
for a selected folder. The Electron bridge currently exposes:

| Current operation | Contract role |
| --- | --- |
| `openWorkspace(path)` | User-selected local scope and initial catalog snapshot |
| `getCatalog(workspaceId)` | `catalog(scope)` |
| `describe(name, workspaceId)` | `profile(sourceRef, revision?)` within the mounted workspace |
| `sampleSource(name, rows, workspaceId)` | Bounded `preview(sourceRef, limit, revision?)` |
| `reindex(workspaceId)` | `refresh(scope)` |

This mapping describes semantics, not a claim that the current IPC API is
provider-pluggable. Its workspace IDs and source names remain scoped by the
existing runtime. Future adapters should translate at their boundary instead
of leaking provider-specific CLI output, filesystem paths, or index internals
into Investigation records.

## Compatibility requirements for future adapters

- The desktop must mount and analyze a selected local folder with the built-in
  provider when no optional CLI/FellaDB installation is present.
- Discovery tools are optional and may improve search or cataloging, but
  failure to find or start one must not disable current workspace sources.
- Provider availability and version/capability mismatches are visible; they do
  not silently change the data source or execution route.
- The desktop consumes typed records over an explicit IPC/shared-core contract,
  never by parsing human-readable terminal output.
- Source scope, revision, exclusions, and actual use remain attributable in
  saved run provenance. A provider's index is not evidence that a file was read
  by an analysis.

No adapter, CLI discovery, new dependency, or analytics-engine change is part
of this issue. Revisit the transport and ingestion boundary only when the
optional filesystem tooling is ready to integrate.

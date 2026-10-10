# FellaDB and the Fella CLI

FellaDB is a proposed persistent filesystem search capability within a
Fella-provided CLI, not a separate companion app. The CLI is the analyst-facing
surface for file search and operations such as catalog, profile, preview, and
read-only query. This is product direction, not shipped behavior.

The full product direction, including the analysis workspace, Projects, source
scope, and the proposed role of the desktop Sources view, is recorded in
[`STACK-VISION.md`](STACK-VISION.md).

## Product boundary

FellaDB owns the local index/search capability for user-selected personal-file
locations. It is not a second SQL engine, a full file manager, or a separate
desktop application. Fella's existing catalog and analytics engine remain
responsible for supported-format ingestion, profiles, previews, and analysis.

The initial search-engine direction is FFF for path, filename, and content
search. Do not create a competing search index when FFF can provide that
capability. Embeddings are not required for the initial product; any future
semantic search would be optional and additive.

## CLI capabilities

The proposed command surface should help analysts:

- search selected locations by path, name, and content;
- catalog supported, skipped, excluded, and inaccessible files;
- profile data shape and quality;
- preview a bounded sample;
- query an explicitly selected analysis scope with read-only SQL.

These are read-oriented analyst tools, not permission to rewrite or reorganize
the user's source files. Any export or derived-file creation should be a
separate, explicit operation. Command names and exact output formats are not
decided.

The CLI should provide both human-readable output and a stable structured
interface for scripts and desktop integration. Fella's UI must consume typed
data through a shared core or local interface, not parse terminal-formatted
text. Whether the CLI and desktop share an in-process Rust library or a local
service remains open.

## Explicit scope and privacy

- **Personal library:** the user selects locations for broad discovery. Whole-
  disk search is opt-in, not the default. The CLI can search this scope; the
  desktop may explicitly use it for a General conversation.
- **Analysis workspace:** the user deliberately selects the folder or sources
  that can be profiled and queried.
- General remains unbound when it uses a selected personal-library scope: it
  does not inherit the focused repository or alter that repository's catalog,
  memory, or analysis history.
- A search result does not silently become an analysis-workspace source. The
  user explicitly hands it off when they want to analyze it in a repository
  workspace.
- Search and indexing do not grant access to unselected paths. The CLI should
  report indexed locations, exclusions, unsupported files, inaccessible
  areas, and index freshness.
- Local indexing does not make model inference local. A hosted model provider
  may receive the question and relevant excerpts or analytical results used in
  an answer, under Fella's existing provider data flow.
- The personal library remains distinct from repository workspace catalogs,
  context, memory, and analysis history.

## Not shipped and open work

Fella currently has no user-facing analyst CLI. The Rust sidecar is launched
by the Electron application using its JSON-lines protocol. The desktop Sources
view is a read-only catalog and preview surface; it is not FellaDB or a
general filesystem search tool. Fella works without this proposed CLI
capability.

Before implementation, validate FFF coverage, resource use, exclusions, and
packaged behavior on supported operating systems. Define installation and
desktop discovery, index lifecycle, multiple-location behavior, stable source
identities, and the boundary between search results and analysis workspaces.

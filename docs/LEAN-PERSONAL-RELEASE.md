# Lean personal release scope

**Status:** Implemented on `feat/lean-personal-release`. The experimental branch
and `main` remain unchanged until this pass is reviewed.

## Decision

The next release should ship as a focused local personal analytics application.
Its core promise is:

> Fella mounts a folder, reads supported files, computes answers through
> deterministic tools, uses a BYOK model for reasoning, and shows the evidence
> behind each answer.

The default release should have a small fixed capability set. It should not
ship a plugin marketplace, a user-installed extension system, official MCP
connectors, or agent-created workspace files.

The extension idea remains valid. It will live at the edge of the project
through documentation, custom builds, and a future reviewed implementation.
It will not shape the everyday product or the default runtime.

## What the user gets

The primary experience is:

1. Mount a folder.
2. Ask a question in plain language.
3. Fella chooses a deterministic tool.
4. SQL or sandboxed Python computes the result.
5. Fella can render a chart.
6. The answer shows its evidence and verification state.
7. The conversation can be revisited later.

The lean navigation should contain:

- **Ask** as the default conversation.
- **Workspace** for sources and explicit folder context.
- **History** for previous conversations and complete answer transcripts.
- **Search** through the command palette.
- **Settings** for the provider, model, appearance, folder, and experimental
  analysis capabilities.

The separate Overview, Packs, Augments, and standalone Analyses surfaces are
removed from the main navigation. A useful answer already carries its chart,
evidence, query, and workspace snapshot, so answers remain in conversation
history instead of becoming a second saved-analysis artifact system.

The experimental capability policy provides four local switches for the
existing table, document, Python, and visualization paths. All are enabled by
default and enforced by the engine. This is a small user control surface, not
enterprise policy or a new extension runtime. The future enterprise direction
is recorded in [`CAPABILITY-POLICY.md`](CAPABILITY-POLICY.md).

The normal composer remains the primary way to ask follow-up questions. This
release does not add a general model-driven clarification workflow. Result
presentation controls, such as paging or showing a different time range, can be
added later without turning every large result into a new agent lifecycle.

## Core capabilities that stay

The following are part of the personal analytics product:

- Folder mounting and cataloging.
- CSV, TSV, JSON, NDJSON, spreadsheet, PDF, and text-file support within the
  supported build configuration.
- Read-only SQL through the local data engine.
- Sandboxed WASM/RustPython for calculations that SQL cannot express easily.
- Validated charts for useful time-series and category views.
- Evidence capture and deterministic answer verification.
- Query timeouts, cancellation, ingestion limits, and bounded tool output.
- BYOK provider and model configuration.
- Conversation history and local workspace context.
- Light and dark appearance modes.

The model can reason about the data, but it never gets direct filesystem access
and never writes, moves, or deletes workspace files. The deterministic tools
remain the only data path.

The fixed built-in tool set is:

```text
list_files
inspect_table
run_sql
grep_files
read_file
run_python
make_chart
```

There are no dynamically added tools in the default release.

## Features removed from the default release

### Packs and the plugin ecosystem

The Packs system should be removed from the next release rather than hidden.

That includes:

- The Packs navigation surface.
- `/packs` installation and management commands.
- Pack manifests and enablement state in the normal application.
- Marketplace catalog and installation flow.
- Theme, skill, MCP, and augment pack loading.
- Pack-specific prompt and UI behavior.
- The active `fella-extensions` repository relationship.

The `fella-extensions` repository should be archived rather than permanently
deleted. Archiving preserves its history and experiments without making it an
active dependency of Fella. The next release should remove its links, catalog
assumptions, CI references, and onboarding instructions.

### Augments

Augments should be removed completely from the personal release.

That includes:

- `/note`.
- `/table` and any plural alias.
- Augment tabs.
- The augment manifest and capability registry.
- The augment file-writing implementation.
- Augment IPC methods and frontend state.
- Augment tests and user documentation.

This restores a much clearer product boundary: the agent analyzes the mounted
folder and does not create workspace artifacts.

The Context surface is a separate decision. Editing `fella.md` is an explicit
user action, not an agent write. If the release keeps the in-app Context editor,
the documentation must say that the user may edit context while the agent remains
read-only. A stricter read-only build can make Context read-only or open the file
in the user's own editor.

### MCP runtime support

The default release should not include an active MCP client or connector
implementation.

The following should remain out of the default runtime:

- MCP transport dependencies.
- Remote connector authentication.
- Connector token storage.
- Dynamic MCP tool discovery and registration.
- Connector marketplace or pack installation.
- Automatic outbound connections to MCP servers.

The regular built-in tool registry must contain no MCP tools.

## The experimental MCP boundary

The `/mcp` command may remain as an inert experimental command. It is a signpost
for future extensibility, not a working connector system.

Its only supported behavior is to explain that MCP is currently closed:

```text
MCP is experimental and closed in this release.
No official connectors are enabled.
Custom implementations require a fork or experimental build.
```

It may be listed under experimental commands in the documentation or help
output. It should not appear as a main navigation item or be presented as a
supported data-source feature.

For this release, “MCP scaffolding” means:

- A documented future boundary.
- A clearly named experimental command.
- A place for a future reviewed implementation to connect to the fixed harness.

It does not mean shipping the full protocol client, connector loader, or
authentication system with no official connector to use.

A future connector must satisfy more than MCP protocol validity. Before official
support, it should have an explicit capability declaration covering:

- The data it can read.
- Whether it can write or mutate anything.
- The network destinations it contacts.
- Timeouts and response-size limits.
- Whether its tools are available in normal Ask mode.
- How the user enables and disables it.
- How its results enter Fella's evidence and verification path.

A custom user implementation can exist in a fork or experimental build. A future
installable connector system is a new product decision and should not be rebuilt
implicitly.

## Architectural end state

The intended request path is:

```text
User
  -> Ask UI
  -> one linear agent loop
  -> fixed read-only tools
  -> analytics engine
  -> evidence and verification
  -> answer and structured visual
```

The layers have distinct responsibilities:

- **UI:** presents conversations, sources, context, charts, and evidence.
- **Harness:** manages the model conversation, prompt, fixed tools, cancellation,
  evidence collection, and final answer.
- **Analytics engine:** ingests data, executes read-only SQL, runs bounded
  Python, validates charts, and verifies claims.
- **Workspace state:** owns the mounted folder, catalog, local settings, and
  conversation storage.
- **Provider transport:** sends only the user's model request to the selected
  BYOK provider.

The analytics engine should not know about packs, MCP, augment tabs, Tauri
commands, or frontend state. The harness should not compute figures itself.
The UI should not decide whether a number is correct.

The removal is especially valuable for the central application state. The current
state coordinator carries workspace, conversations, memory, providers, and
updates. Removing the extension surfaces reduces the number of
lifecycle states that the coordinator must manage and makes the core request
path easier to follow.

## Compatibility and migration

Removing a feature from the release should not delete a user's files or
credentials automatically.

If an older installation contains extension records, pack folders, augment
files, or connector tokens:

- The new release should ignore them.
- It should not load their tools or execute their code.
- It should not silently send their tokens anywhere.
- It may leave the files in place so the user can recover or remove them
  manually.
- Any migration should be explicit and non-destructive.

The experimental branch can continue to contain the removed implementations and
their tests. The release branch should not compile or expose them through the
normal application.

## Release acceptance criteria

The lean release is ready when:

- The main navigation contains no Packs or Augments surface.
- `/packs`, `/connect`, `/note`, and `/table` are unavailable.
- `/mcp` is present only as an inert, clearly labeled experimental command.
- The default build has no active MCP connector path.
- No dynamic tools can enter the default agent registry.
- The agent has no workspace write tools.
- The fixed built-in tools still pass their deterministic and sandbox tests.
- Charts, evidence, verification, provider login, folder mounting, and history
  still work.
- Documentation describes the shipped product rather than the future extension
  system.
- The archived extension work remains available outside the release path.

## Future reopening criteria

Extensibility should return only when it has a concrete use case and a design
that keeps the core understandable.

A future proposal should answer:

1. What user problem requires an extension?
2. Why can the core tool set not solve it?
3. What permissions does the extension need?
4. Can it remain read-only?
5. What data can leave the machine?
6. How is it bounded and verified?
7. How does a user disable it?
8. What code stays out of the default build?
9. What new UI and lifecycle state does it introduce?
10. What existing complexity does it remove or replace?

This keeps extensibility available without making it an obligation for every Fella
user or contributor.

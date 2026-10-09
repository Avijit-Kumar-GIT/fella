# Fella workspace and composable-interface architecture

This document is the product and implementation contract for workspace
identity, interaction, and interface composition. It distinguishes what exists
in the current app from the composable surfaces that remain roadmap work.

## Product model

Fella is an analytics harness. Workspaces organize analytical conversations;
they are not the product's primary unit of value. A workspace gives those
conversations a stable scope and continuity without forcing every question to
be about a mounted folder.

There are two workspace kinds:

- **General** is the sidebar/history group for questions and conversations
  that do not belong to a mounted folder. It has no filesystem path, catalog,
  or folder-specific memory. Its analytical request is genuinely unbound
  (`workspaceId: null`); it is not an automatically opened board tile.
- **Repository workspace** is owned by one folder. It has its own catalog,
  source inventory, context, memory, and conversations. The folder may be
  mounted, temporarily closed, or unavailable while its saved conversations
  remain accessible as history.

An Electron app window contains one persistent global shell and a board of up
to four workspace windows. A “workspace window” is a tile inside Fella's main
window, not a separate operating-system window. Each workspace window can
eventually contain up to four composable surfaces. These are separate limits:
four outer workspaces, and four inner surfaces per workspace.

## Architecture layers and ownership

```text
Electron app window
├─ Global shell
│  ├─ Sidebar: Ask, Search, General, and workspace-owned Sources/Project/history
│  ├─ Titlebar, settings, command palette, provider credentials
│  ├─ Conversation tabs: global navigation, each retaining its owner
│  └─ App-level route and workspace-board controls
├─ Workspace registry
│  ├─ General conversations (unbound; no board tile by default)
│  └─ Repository workspace (folder identity, independent of mount status)
├─ Open workspace board (up to 4 workspace windows)
│  └─ Workspace window (up to 4 typed surfaces; composition is being built)
│     ├─ Active conversation surface
│     ├─ Optional chart / analysis / source surfaces
│     └─ Workspace-owned context and project artifacts
└─ Analytics runtime
   ├─ Conversation and turn identity
   ├─ Read/inspect, plan, execute, and answer lifecycle
   └─ Evidence and artifacts tied to the owning workspace/revision
```

| Layer/model | Owns | Does not own |
| --- | --- | --- |
| App shell | Global navigation, titlebar, settings, Search, credentials, app route | Conversation or folder identity |
| Workspace identity | Stable General or folder-backed owner; conversation collection; context/memory; optional project | Open/closed state, or the analysis execution itself |
| Mount session | A repository workspace's live catalog and runtime availability | Workspace identity or archived conversations |
| Workspace window | A workspace's current visible placement and composition | New data, permissions, or a separate backend instance per surface |
| Conversation | Questions, follow-ups, clarification, chosen model, and continuity; exactly one workspace owner | A workspace window or copied transcript |
| Analysis turn | One question's plan, tool actions, evidence, answer, verification findings, and outputs | A conversation or UI surface |
| Surface composition | Typed references to existing conversation/turn/source artifacts | Duplicate analysis, arbitrary executable UI, or cross-workspace access |
| Project/wiki | Optional, user-created local knowledge artifact associated with a repository workspace | A workspace, conversation, or automatically generated requirement |

The implementation keeps four interaction coordinates distinct:

1. **App route** — Ask/board, Sources/Guide, Projects, or Settings.
2. **Focused workspace** — which workspace window receives workspace actions.
3. **Active conversation tab** — the selected conversation and its stable
   workspace scope; the global composer sends to this conversation.
4. **Focused surface** — the selected surface within a workspace composition
   once multi-surface composition is implemented.

Changing one coordinate must not silently change ownership in another. Search,
Settings, and Projects are app-level destinations. Selecting an archived
conversation opens its owning workspace when needed and selects its global
conversation tab. Closing a tab archives it; it does not delete history.

## Interaction contract

- **New conversation** is the default when no repository workspace is focused.
  It is unbound and does not prompt the user to mount a folder. `Ctrl+T` / `Cmd+T`
  opens another conversation tab. Ask creates a conversation in the focused
  repository when one is selected.
- **General** is the sidebar group for unbound conversations, not an open
  workspace window. Selecting it returns to an existing unbound conversation
  (or starts one); opening a repository never claims those conversations.
- **Repository selection** focuses the existing tile or opens that folder as a
  workspace. Its sidebar container owns Sources, an optional Project, and
  conversations; its plus action starts a conversation in that repository.
  General also has a plus action for an unbound conversation.
- **Conversation tabs** switch between independent conversations while keeping
  each folder scope fixed. Switching to a repository-owned tab restores or
  opens that workspace; it never borrows the previously focused folder.
- **Workspace placement** uses drag previews and fixed equal-split layouts.
  Drag a repository into the board to add it, or drag an open workspace header
  to another preview slot to reorder it. Two and three workspaces select a
  predefined arrangement from the drop location; four use a fixed 2×2 grid.
  There are no resize handles or arrangement dropdowns.
- **Capacity** is explicit and non-destructive. A fifth open workspace is not
  substituted for an existing one; the user closes one first.
- **No-repository history** is not a fake repository and not an automatic
  routing bucket. It belongs to General. A missing repository is labeled
  history-only; the conversation is readable but analysis cannot run until the
  original folder is reopened.

The sidebar's “Workspaces” group contains General and folder-backed workspace
containers. The global sidebar and conversation tabs remain available no matter
how many workspace tiles are open. With no folder selected, the composer and
main surface remain a plain New Conversation view.

## Composability contract

“Composable” means a workspace owner can arrange supported, typed views of
artifacts already owned by that workspace. It does not mean that every control
is a tile or that model-generated code can change the interface.

### Eligible surfaces

- **Conversation** — the workspace's active analytical conversation and
  composer; the default and primary surface.
- **Chart** — a reference to a chart produced by one answer/evidence item.
- **Analysis details** — a reference to the trace and evidence for one turn.
- **Source preview** — a source path and catalog revision belonging to the
  workspace.

The registry can grow when a surface has clear ownership, a stable identity,
and a useful independent purpose. Opening, moving, or closing a surface changes
presentation only. It does not rerun analysis, grant a tool, change folder
scope, or delete the underlying artifact. Stale references remain visibly
stale; they are never silently rebound to a newer catalog revision.

### Not composable

The global sidebar, titlebar, settings, command/search palette, dialogs,
provider controls, notifications, and app-level navigation are shell controls,
not workspace surfaces. Arbitrary HTML, user-authored executable widgets, and
model-generated UI are also out of scope. A source, chart, or analysis trace
cannot be moved into another workspace if that would misrepresent its
provenance or grant access to another folder.

### Fixed geometry

There is no free resizing. Every split divides its parent equally. The outer
workspace board and the future inner surface layout use the same geometry:

| Items | Supported layout |
| --- | --- |
| 1 | One full tile |
| 2 | Side-by-side or stacked, each half |
| 3 | Two top / one bottom; one top / two bottom; two left / one right; or one left / two right. A nested split creates two quarters. |
| 4 | Fixed 2×2 grid |

The layout is deterministic and does not evict or resize an existing item to
make room. Four is a product ceiling for each composition, not a recommendation
to keep every slot occupied.

## Persistence, mount state, and isolation

- General's UI ID is `general`, but its analytical workspace ID remains null.
- A repository workspace is identified by its folder path as resolved by the
  backend. Two folders with the same display name remain distinct.
- Open workspace IDs, focused workspace, and outer layout are saved locally.
  Closing a workspace tile does not delete its workspace history or source
  files. Conversation transcripts are archived locally and loaded on demand;
  on launch the workspace board is restored with a fresh conversation slot.
- The sidebar may remember closed repository workspaces for navigation. If a
  folder cannot be mounted, the owner remains intact and is shown as
  history-only rather than silently borrowing another workspace's catalog.
- Provider credentials and app settings are app-scoped. Catalog, context,
  memory, and analysis source revisions are repository-workspace-scoped.
- Every analysis request and resulting evidence remain tied to the owning
  conversation, workspace, and catalog revision. General requests do not read
  the last-mounted folder by accident.
- The backend registry shares app/provider state while keeping an isolated
  analytical runtime/catalog per open folder. It does not launch one sidecar
  per tile.

## Implementation status

### Implemented in the current app

- The Rust sidecar has an app-level registry with isolated workspace runtimes
  keyed by folder identity; workspace-aware requests cover catalog, Ask, SQL,
  context, memory, source preview, replay, reindex, cancellation, and progress.
- The app starts in a new unbound conversation and exposes global conversation
  tabs with `Ctrl/Cmd+T`, close, previous/next, and numbered selection.
- The workspace board supports one to four repository tiles, fixed equal-split
  layouts, drag-placement/reordering preview, persistence, restart restoration,
  and explicit capacity handling.
- General is a sidebar/history group with no folder scope or board tile.
  Opening a repository never claims General conversations.
- Each repository container groups Sources, its optional Project, and its
  conversations. Workspace-level plus actions create scoped conversations.
- The repository workspace owns the live catalog and source list; the global
  composer targets the focused workspace. Only the focused tile renders its
  active transcript; other tiles show a source overview.
- Unavailable repository history remains readable and cannot run analysis
  until its original folder is successfully reopened.

### Not yet implemented

- A workspace-owned registry of up to four independent inner surfaces. Current
  chart/source companion UI is still conversation presentation state, not the
  final composable surface model.
- Independent simultaneous active conversations as multiple workspace
  surfaces; today each workspace has one selected conversation slot, while
  other conversations remain archived and in-flight work retains its owner.
- Independent simultaneous conversations as multiple inner surfaces; outer
  workspace tiles still show one active transcript at a time.
- Unified aggregate CPU/memory budgeting across several open runtimes.
- Workspace-owned project/wiki support for General. Existing project artifacts
  are local and repository-associated.

## Roadmap

1. **Workspace identity and interaction — implemented.** Separate General
   conversation grouping, repository identity, mount availability, open
   workspace windows, global conversation tabs, and conversation ownership.
2. **Outer workspace composition — implemented.** Fixed one-to-four tile board,
   equal split layouts chosen through drag placement, tile reordering, local
   restore, and explicit capacity behavior.
3. **Typed inner-surface registry — next.** Move chart/source/analysis
   presentation references from conversation-only companion state into
   workspace-owned typed surfaces. Preserve artifact provenance; cap each
   workspace at four surfaces.
4. **Complete journeys and hardening — in progress.** Test General and
   repository ownership, Ask/follow-up scope, concurrent work, capacity,
   restart, unavailable folders, stale artifacts, keyboard access, themes, and
   narrow-window layouts. Judge UI behavior separately from analytics
   correctness.
5. **Resource and recovery polish.** Add aggregate runtime budgeting and clear
   relink/reopen paths without discarding local conversation history.

## Validation

- `pnpm test:workspace-layout` — geometry presets and their invariants.
- `cargo test --locked --manifest-path backend/Cargo.toml electron_sidecar_opens_a_workspace_and_runs_a_read_only_query` — sidecar registry and scoped catalog/query behavior.
- `pnpm test:e2e:workspace-board` — renderer/bridge journey for General and
  repository ownership, history selection, scoped Ask, fixed composition,
  capacity, close/free-slot, and restart restore. The bridge is mocked; this
  does not establish Electron packaging or model-answer correctness.
- FQA-Bench and real-model journeys separately measure analytical correctness.

Test failures remain failures. UI journey tests do not grade whether an
analysis answer is factually correct, and analytics benchmarks do not replace
the full app interaction journey.

# Composable interface

This document defines the product contract, current implementation, and
remaining roadmap for Fella's composable interface. The first outer-workspace
slice is implemented: up to four independently mounted repository runtimes can
appear on one fixed-layout board. Typed composition inside each repository is
still future work.

## Product model

Fella is an analytics harness with a persistent app shell and repo-owned
workspaces. Composition should make concurrent work easier to inspect, not
turn the app into an unrestricted desktop canvas or terminal multiplexer.

```text
Fella app (one global shell)
├─ Sidebar: Ask, Search, Projects, repository history and open-state
├─ Global titlebar and app controls
└─ Workspace board (up to 4 repository workspaces)
   ├─ Repository workspace (one mounted folder, its own analytical scope)
   │  └─ Composition (up to 4 typed, workspace-owned surfaces)
   └─ ...
```

The sidebar and app controls never move into a repository tile. A repository
workspace is a live analytical sandbox for one folder, not another name for a
conversation or an arbitrary visual card. It owns its catalog revision,
conversations, context, selected view, and any composed analysis artifacts.
One repository can have several conversations; conversations do not migrate
between repositories when the user changes focus.

General, unbound Ask conversations remain unbound. Opening or focusing a
repository must never silently attach them to that folder. Projects/wiki-like
summaries remain optional, user-created, local artifacts owned by a repository;
they are not created automatically and are not part of the first tiling slice.

Conversation tabs are app-level navigation, not another set of workspace
windows. Each tab retains its repository owner (or `General` when unbound), and
switching tabs restores that conversation's catalog and workspace focus. The
four-slot limit applies to mounted repository windows, not conversation tabs;
tabs are lightweight conversation navigation and may outnumber the four open
repositories. General conversations stay unbound and do not consume a
repository tile. `No repository` in the sidebar is a history group, not a
mounted folder or a new workspace: Ask starts a new unbound conversation (or
reuses the pristine welcome tab), while selecting a history row reopens or
activates that exact conversation.

### Implemented outer-workspace slice

- A Rust registry owns one isolated `EngineState` and analytical catalog per
  canonical folder path, with a four-workspace limit. Provider settings,
  credentials, and archived conversations remain app-scoped.
- Workspace-aware IPC carries the folder identity for catalog, Ask, SQL,
  context, memory, source preview, replay, reindex, and cancellation operations.
  Unbound Ask stays on the app runtime and does not borrow a mounted catalog.
- The board opens/focuses repository tiles by click or sidebar drag, previews
  the resulting fixed layout, persists open paths and layout locally, restores
  open folders on restart, and supports explicit close and capacity handling.
- A tile shows its own source inventory. Only the focused tile currently
  renders the live transcript; the global composer targets that tile. Other
  tiles show a compact source overview rather than a second live transcript.

This is a real multi-repository board, not yet the complete composable
workspace. Conversation selection remains app-tab state, Sources/Guide remain
existing workspace views, and chart/source/analysis panes are not yet entries in
a per-workspace surface registry. Independent simultaneous conversations,
drag-reordering existing tiles, and composing up to four inner surfaces remain
open work.

## What can be composed

Only entries in a built-in, typed registry can be placed inside a repository
workspace. The initial registry is:

- **Conversation** — the workspace's active analytical conversation and its
  composer. It is the default surface and remains the main way to ask Fella.
- **Chart** — a reference to a chart produced by a specific answer/evidence
  item, not a copy or a fresh calculation.
- **Analysis details** — a reference to the trace/evidence for a specific turn.
- **Source preview** — a path and catalog revision owned by that workspace.

The registry can grow when a surface has a clear purpose and stable ownership.
Arbitrary HTML, user-authored executable widgets, model-generated UI, global
settings, dialogs, the command palette, and the sidebar are not composable
surfaces. A chart or source from one repository cannot be dropped into another
repository: its evidence and revision would no longer describe that workspace.

Opening, moving, or closing a surface changes presentation only. It does not
rerun analysis, grant a tool, alter folder scope, or delete the underlying
conversation, answer, or source. A stale source/chart reference is identified
as stale; it is never silently rebound to newer data.

## Fixed tiling and interaction

There is no free resizing. Every split divides its parent region equally. The
same rules apply to the outer repository board and to surfaces inside each
repository workspace:

| Occupied slots | Allowed composition |
| --- | --- |
| 1 | One full tile |
| 2 | Side-by-side or stacked, each half of the board |
| 3 | Two top / one bottom; one top / two bottom; two left / one right; or one left / two right. Each split is 50/50. |
| 4 | Fixed 2×2 grid |

Three tiles use recursive equal splits: one half of the area is a single tile,
and the other half is divided into two quarters. Four tiles each occupy one
quarter. The board never silently resizes an existing tile or evicts a
workspace to make room.

The implemented placement interaction is to drag a repository from the sidebar
onto the board; the preview shows the resulting fixed composition before
release. Clicking a repository focuses its existing tile, or opens it in the
next available slot. Add, focus, close, and arrangement controls are standard
keyboard-accessible buttons/selects. Drag-reordering existing tiles and surfaces
is not implemented. At capacity, Fella explains how to free a slot; it does not
replace a workspace without the user's choice.

Both the board and each repository composition have a hard limit of four.
This is a deliberate legibility and resource boundary, not a recommendation
that all four always be open.

## Ownership, persistence, and isolation

- A mounted folder has one stable local workspace identity, distinct from its
  display name. Different folders with the same basename remain distinct.
- Each request, streamed event, conversation, source, and analysis trace is
  scoped to a workspace identity and the catalog revision it used.
- A workspace keeps its own live catalog and conversation selection while
  another workspace is mounted or running. The model must never see a mixed
  catalog assembled from neighboring tiles.
- Provider credentials, app settings, and global navigation remain app-scoped;
  workspace catalog, context, conversation continuity, and view composition
  remain workspace-scoped.
- Save open workspace identities, tile layout, and component references
  locally. Persist answer/chart data once with its canonical conversation or
  analysis record; layouts store references rather than duplicate payloads.
- Closing a tile removes it from the current board, not from repository
  history or disk. If its folder is missing on restart, preserve history and
  present a clear history-only/relink state; never mount a different folder as
  a substitute.
- Mount progress, errors, stop/cancel, and analysis details are addressed to
  the owning workspace. Concurrent activity in one tile must not overwrite
  another tile's status or cancellation state.

The Rust sidecar now has an app-level registry and explicit workspace-scoped
requests. It shares app/provider state while giving each open folder its own
runtime context; it does not switch one mutable catalog or launch a sidecar per
tile. The registry enforces four open workspaces. A unified aggregate CPU/memory
budget across those runtimes is not yet implemented; per-operation limits and
the four-workspace ceiling are the current resource controls.

## Delivery roadmap

1. **Define fixed layout geometry — complete.** Add a pure, typed layout model for 1–4
   stable tile IDs and tests for every supported composition, exact 50/50
   splits, full coverage, no overlap, duplicate IDs, and the four-tile limit.
2. **Isolate runtime ownership — initial slice complete.** Introduce workspace identities and an
   app-level registry in Rust. Thread the workspace ID through mount/catalog,
   ask/resume, tools, cancellation, progress/events, source preview, memory,
   and persistence boundaries. Prove two same-named, different-data folders
   can be queried concurrently without cross-workspace leakage.
3. **Build the repository board — initial slice complete.** Render one to four actual registered
   workspaces in the fixed layouts. Add drag placement previews plus accessible
   button/select alternatives. Persist and restore open paths and layout locally.
4. **Compose workspace surfaces.** Move from one conversation-owned companion
   reference to a workspace-owned typed surface registry. Preserve source and
   chart provenance, support up to four surfaces, and keep the conversation
   available as the default/primary surface.
5. **Harden complete journeys — in progress.** Exercise real Electron journeys for multiple
   mounts, parallel analysis, workspace isolation, limit handling, restart,
   unavailable folders, stale artifacts, light/dark themes, keyboard-only
   placement, and narrow window sizes. Audit correctness and rendered geometry
   separately; a layout unit test is not an end-to-end workspace test.

### Completion criteria

The feature is ready when two to four folders can remain open and independently
usable at once; a question and follow-up always use the owning folder; each
layout is deterministic, legible, and accessible without a pointer; artifacts
retain exact answer/source provenance; state restores safely; and fifth-item,
missing-folder, cancellation, and concurrent-progress states are explicit and
non-destructive. Tests must cover those contracts before claiming completion.

## Current test surface

Run `pnpm test:workspace-layout` for all fixed geometry presets,
`cargo test --locked --manifest-path backend/Cargo.toml electron_sidecar_opens_a_workspace_and_runs_a_read_only_query`
for real sidecar IPC/catalog isolation, and `pnpm test:e2e:workspace-board` for
the browser-renderer journey with a mocked Electron bridge (mount, drag preview,
unbound and workspace-scoped Ask, capacity, close/free-slot, and restart
restore). The browser journey validates UI/bridge wiring, not the Electron
shell or model correctness; analytics correctness remains the responsibility
of FQA-Bench and real-model journeys.

# Composable interface

This document defines the product and engineering contract for Fella's
composable interface. It is a roadmap, not a claim that multi-repository
workspaces are implemented. The current UI has a rounded frame around one
active work area; the runtime still exposes one mounted workspace at a time.

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

The intended placement interaction is to drag a repository from the sidebar
onto the board (or drag an existing tile/surface to reposition it). Valid
drop regions preview the resulting fixed layout before release. Dragging is
not the only route: keyboard/context-menu actions must offer equivalent
open, move, and close commands. Clicking a repository focuses its existing
tile, or opens it in the next available slot. At capacity, Fella explains how
to free a slot; it does not replace a workspace without the user's choice.

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

The current Rust `EngineState` contains one mutable workspace catalog, and the
Electron IPC commands currently address that catalog without a workspace ID.
The multi-workspace implementation must replace that implicit global scope
with an app-level workspace registry and explicit workspace-scoped requests.
Prefer one Rust sidecar with shared app/provider configuration and isolated
workspace runtime contexts; do not simulate concurrency by repeatedly
switching one global catalog, and do not launch a full copy of the app per tile.
The registry must enforce the four-workspace cap and account for aggregate
ingestion/query resources.

## Delivery roadmap

1. **Define fixed layout geometry.** Add a pure, typed layout model for 1–4
   stable tile IDs and tests for every supported composition, exact 50/50
   splits, full coverage, no overlap, duplicate IDs, and the four-tile limit.
2. **Isolate runtime ownership.** Introduce workspace identities and an
   app-level registry in Rust. Thread the workspace ID through mount/catalog,
   ask/resume, tools, cancellation, progress/events, source preview, memory,
   and persistence boundaries. Prove two same-named, different-data folders
   can be queried concurrently without cross-workspace leakage.
3. **Build the repository board.** Render one to four actual registered
   workspaces in the fixed layouts. Add drag placement previews plus accessible
   menu/keyboard alternatives. Persist and restore board state locally.
4. **Compose workspace surfaces.** Move from one conversation-owned companion
   reference to a workspace-owned typed surface registry. Preserve source and
   chart provenance, support up to four surfaces, and keep the conversation
   available as the default/primary surface.
5. **Harden complete journeys.** Exercise real Electron journeys for multiple
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

## First implementation slice

Start with step 1 only. It is a small, testable foundation and does not pretend
that runtime isolation or multi-repository rendering already exists. The pure
layout function is intentionally independent of React/Svelte and backend
details so both repository tiles and inner surfaces can share one geometry
contract. The next functional milestone is step 2; the layout helper alone is
not a user-facing completion claim.

# Fella environments and composable workspaces

This document defines the interaction model and ownership boundaries for
Fella's desktop interface. It is the contract for the environment-tab redesign
and the workspace composition that is being built on top of it.

## Product model

Fella is an analytics harness. Conversations are the primary way people ask
questions and continue analysis; workspaces provide optional organization and
local data scope. The interface can arrange several workspaces together without
changing which data a conversation is allowed to use.

There are two workspace kinds:

- **General** is an unbound workspace. It has no path, catalog, or folder
  memory. General conversations stay unbound even when another repository is
  open in the same environment.
- **Repository** is identified by a resolved local folder path. It owns its
  catalog, sources, context, memory, optional project, and conversation
  history. If its folder is missing, its history remains readable without
  borrowing another folder's data.

## Architecture layers

```text
Electron app window
├─ Global shell
│  ├─ Sidebar: Ask, Search, workspace containers, Settings
│  ├─ Titlebar: environment tabs and app/window controls
│  └─ App-level views: Ask, Sources, Project, Settings
├─ Environment tabs
│  └─ Environment: a saved arrangement of up to four workspace panes
│     ├─ General pane (unbound; optional in any arrangement)
│     └─ Repository pane(s), each with its own focused conversation
├─ Workspace registry
│  ├─ General conversation group
│  └─ Repository identities and mounted runtime/catalogs
└─ Analytics runtime
   ├─ Conversation and turn identity
   ├─ Model-driven inspect, plan, execute, and answer loop
   └─ Evidence tied to conversation, workspace, and source revision
```

| Model | Owns | Does not own |
| --- | --- | --- |
| App shell | Global navigation, settings, search, provider credentials, window controls | Conversation scope or folder identity |
| Environment | A named-by-composition collection of pane references, active pane, and fixed layout | Data permissions, workspace identity, or analysis execution |
| Workspace pane | One General or repository workspace placement and its selected conversation | A copy of the underlying workspace, catalog, or transcript |
| Workspace identity | General or a folder path; repository catalog, context/memory, project, and conversation collection | Open/closed placement or an environment tab |
| Conversation | Questions, follow-ups, clarifications, model choice, and continuity; one stable workspace scope | Environment ownership or a separate copy per view |
| Analysis turn | Plan, tool actions, evidence, answer, verification findings, and outputs | Workspace layout or other conversations |
| Surface | A typed reference to an existing conversation, chart, analysis, or source artifact | New permissions, duplicated analysis, or model-authored UI |

The active interaction coordinates are distinct:

1. **App view** — Ask/board, Sources, Project, or Settings.
2. **Environment** — the selected titlebar tab and its saved pane arrangement.
3. **Focused pane** — which workspace receives the global composer and
   workspace-level actions.
4. **Conversation** — the thread selected inside that focused pane.
5. **Surface** — an optional chart, analysis detail, or source reference within
   the active conversation/workspace.

Changing the environment or focused pane must not rewrite a conversation's
workspace scope. A General question remains unbound; a repository question
continues using only its owning repository.

## Interaction contract

- **Environment tabs are not conversation tabs.** A tab represents a saved
  arrangement of workspaces. Its label is derived from the arrangement (for
  example `General`, `fella-oss`, or `General +1`). Conversation history stays
  in the sidebar under its General or repository owner.
- **New environment** (`Ctrl/Cmd+T`) opens a fresh General conversation in a new
  environment. Environments can be switched with the titlebar, `Ctrl/Cmd+[`,
  `Ctrl/Cmd+]`, or number shortcuts. The tab strip scrolls horizontally rather
  than compressing into an unusable set of tiny tabs.
- **New conversation** starts a thread inside the focused pane. Ask is also a
  direct new-conversation action. A repository's plus action starts a scoped
  thread there; General's plus starts an unbound thread.
- **General is a composable pane.** It is the default in a new environment and
  can sit beside repository panes. Selecting General does not mount a folder.
- **Workspace containers** in the global sidebar group each repository's
  Sources, optional Project, and conversations. General has its own history
  group. The sidebar remains global while environments change.
- **Repository selection** adds/focuses that repository in the current
  environment. Dragging a repository from the sidebar onto the board previews
  an available fixed placement; dragging a pane header rearranges that
  environment. A workspace can appear in more than one environment, while its
  underlying repository identity/catalog remains shared.
- **Four-pane ceiling per environment.** The fifth pane is not silently
  dropped or substituted. Create/switch environments to work with another set
  of workspaces. Environments themselves scroll and are not artificially
  limited to four.
- **Fixed geometry.** Each split is equal. Two panes are side-by-side or
  stacked; three use one of four recursive half-splits; four use a 2×2 grid.
  Users drag to select placement—there are no resize handles or orientation
  dropdown.
- **Closing a pane** removes its placement from the current environment. It
  does not delete workspace history, sources, or conversations. A repository
  runtime can be released when no environment uses it.
- **Missing-folder history** is compactly marked in the sidebar with a direct
  reconnect action. The user can still open saved conversations; analysis
  remains unavailable until the original folder is mounted.

## What is composable

The outer composition unit is a workspace pane. It references an existing
General or repository workspace and its selected conversation. Inside a
workspace, the longer-term surface registry may arrange up to four typed views:

- the active conversation and composer;
- a chart produced by an answer;
- analysis details for a particular turn;
- a source preview at a specific workspace/catalog revision.

Moving or closing a view changes presentation only. It does not rerun analysis,
change data scope, delete its artifact, or grant another workspace's access.
Stale source/chart references should remain visibly stale rather than being
silently rebound to newer data.

The following remain global shell controls, not tiles: sidebar, titlebar,
settings, command/search palette, dialogs, provider controls, notifications,
and app-level navigation. Arbitrary HTML, executable widgets, and
model-generated interface code are out of scope.

## Visual direction

Use macOS desktop apps as a reference for restraint and hierarchy, not as a
literal skin:

- the sidebar, app chrome, workspace canvas, pane body, and pane header use
  related but distinct surface tones in both light and dark mode;
- selected rows use a quiet filled highlight, not repeated accent rails;
- colored semantic icons use solid silhouettes; neutral utility controls use
  familiar, consistent line icons;
- expanded workspace content is compact and clearly owned without turning the
  sidebar into a second file manager;
- use curvature, restrained tonal contrast, and clear focus states to create
  depth. Do not rely on decorative drop shadows, repeated separators, or
  accent-color flooding;
- the composer sits directly on the workspace canvas, and the pane grid uses
  the available area with consistent outer margins.

## Persistence and isolation

- Environment layouts, active environment, and active pane are stored locally.
  A legacy single-board preference migrates into one environment.
- Each environment has at most four pane references; a pane's conversation ID
  is a reference, not a duplicate transcript. Conversation archives remain
  separately stored by the desktop backend.
- Repository runtimes are shared between environments by resolved path; this
  avoids launching duplicate engines for the same folder.
- General's visual identity never becomes a fake filesystem path. Its
  analytical workspace ID remains null.
- The active conversation scope—not the visible pane arrangement—selects data
  for an analysis request and evidence.
- Provider credentials and app settings are app-scoped. Catalog, context,
  memory, and source revisions are repository-scoped.

## Implementation status

### Implemented

- Environment tabs in the titlebar, including create, switch, close, keyboard
  navigation, and locally persisted arrangements.
- General and repository panes can coexist in an environment; each has a
  separately selected conversation reference.
- A four-pane limit applies per environment. Repositories can be reused across
  environments without duplicating their runtime.
- Drag placement/reordering uses equal-split layouts and a placement preview.
- Conversation history is no longer represented as titlebar tabs; it remains
  grouped under General or its repository in the sidebar.
- Sidebar status for an unavailable repository is compact, with a reconnect
  action; Settings has no ornamental divider and uses a gear icon.
- The board starts at its consistent outer margin without a blank header band;
  inactive panes show a compact recent-conversation preview.
- Workspace, conversation, and evidence scope remain separate.

### Still to build

- Up to four simultaneously visible inner surfaces within a workspace pane.
- A dedicated surface registry and persistence format for chart, analysis,
  conversation, and source references.
- User-controlled environment naming and explicit reorder/duplicate actions.
- Resource budgeting and lifecycle telemetry across many saved environments.

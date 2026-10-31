# Interaction model

This describes the current desktop organization. It is the UX reference for
the user-facing product; the [product model](./PRODUCT-MODEL.md) defines data
ownership and lifecycle.

## App structure

```text
Fella window
├── Native operating-system titlebar and window controls
├── Workspace selector — mounted folder/data scope
├── Sidebar
│   ├── Chat — Conversations, Investigations, and Questions
│   ├── Files — mounted-folder file hierarchy and source status
│   └── Reports — saved report documents
└── Main work area
    ├── Chat — default conversation surface
    └── Notebook — optional Chat + selected Investigation/tool layout
```

The **mounted Workspace** is the folder that scopes source access. The **app
workspace** organizes content in the Fella window and owns its Reports. The
**window layout** is how Chat and a companion surface are arranged. Notebook
does not create a second mounted Workspace, grant data access, or create an
Investigation.

## Chat and Investigations

Chat is the default, lightweight entry point. A new Conversation does not
automatically create an Investigation. The user starts an Investigation
explicitly; that action creates a linked Conversation. Ordinary follow-up
messages remain in that Conversation. The Investigation's title, Brief,
Questions, analysis, and results remain available in its companion pane.

Investigations are persistent analytical work, not a mandatory shell around
every chat. Their inner navigation is:

- **Brief** — goal, scope, and reusable context for this investigation.
- **Sources** — choose and review the sources in scope.
- **Questions** — edit the work plan and inspect coverage.
- **Analysis** — review saved runs, their answers, and supporting evidence.
- **Results** — review findings, limitations, and saved visual results.

Section navigation is progressive disclosure, not a required workflow. The
agent may adapt the work to the question. Do not show unanswered Questions as
findings or expose raw internal reasoning.

## Files, context, and reports

- **Files** is a folder browser and source-status view. A preview does not
  silently change the analysis scope.
- **Workspace context** contains reusable folder guidance and knowledge. It is
  opened from the app's workspace controls, not treated as another label for
  Files or Investigation Sources.
- **Investigation Sources** controls the eligible sources for that
  Investigation. It is distinct from the general Files browser.
- **Reports** contains the app workspace's saved report documents and exported
  analysis. It is available with or without a mounted folder, and a report can
  reference saved results from multiple mounted folders while preserving each
  result's source scope. It can open beside Chat without overflowing the pane.
- The former shell/terminal tool has been removed.

## Layout and interaction rules

- Keep the native operating-system titlebar and window-control placement.
- Keep navigation and content responsive to the available window width. At
  compact widths, prioritize the active surface over squeezed side-by-side
  content. Generated answers must wrap within the chat; avoid page-level
  horizontal scrolling.
- Switching Chat/Notebook or opening a companion surface changes presentation
  only. Preserve the active Conversation, draft, Investigation, source scope,
  and saved results; do not rerun analysis or reload the window.
- Keep Investigation section tabs inside the Investigation pane. They are
  distinct from the left sidebar destinations.
- Keep shared controls and visual treatments consistent with Fella's design
  tokens and shared shadcn-svelte component source. Use progressive disclosure
  for provenance and technical detail.
- A failed load is an error state, not an empty list. Preserve saved content
  and provide a relevant retry or recovery action.

For colors, typography, component usage, and responsive layout guidance, see
[Interface design](../design/DESIGN.md).

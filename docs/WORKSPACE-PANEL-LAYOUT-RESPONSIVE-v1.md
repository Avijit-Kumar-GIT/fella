# Workspace panel layout responsive contract v1

Acceptance criteria are defined before running the focused browser fixture.

## Compatibility

- Omitting `notebook` or passing `false` preserves the existing conversation-left,
  tools-right split, the current width preference, and the existing expanded/compact
  behavior.
- The existing `conversation` and `tools` snippets remain the only content slots.
  Switching layout, resizing, or selecting a narrow-screen pane must not destroy
  and recreate either snippet's rendered content.
- `expanded` continues to show the tools/document surface alone; the notebook
  responsive tabs are not shown in that mode.

## Notebook split

- With `notebook=true`, an open, non-expanded layout at an available width of at
  least 760px places tools/document on the left and conversation on the right.
- The default document allocation targets approximately 62% of the available
  split width. At widths where both minima fit, document remains at least 400px
  and conversation at least 320px.
- The notebook split can be resized by pointer and keyboard. Its remembered
  width uses a separate preference from the legacy conversation/tools split.

## Narrow notebook layout

- Below 760px available width, an open notebook layout presents accessible,
  keyboard-operable pill tabs for Document and Chat. Document is selected first.
- Selecting a tab changes visibility only. Both pane elements and their snippet
  state remain mounted; the inactive pane is hidden from layout and accessibility.
- The layout and its visible content do not create document-level horizontal
  overflow or a horizontally scrollable child.

## Focused browser evidence

The new test mounts the real `WorkspacePanelLayout` with persistent stateful
snippets and checks: the default ordering; notebook ordering and approximate
allocation; minimum widths; `expanded` behavior; tab selection and keyboard
navigation below the breakpoint; retained DOM node identity/state across layout
changes; and horizontal overflow at wide and narrow widths. This is a component
contract test only; it does not exercise the parent route or session wiring.

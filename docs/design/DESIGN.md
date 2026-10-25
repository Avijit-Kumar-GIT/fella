# Interface design

Fella should feel like a calm desktop workspace for analytical work: clear
hierarchy, useful whitespace, quiet chrome, and consistent components. The
canonical tokens live in `src/app.css`; shared UI components live under
`src/lib/components/ui/`.

## Visual language

- Use Fella's established desktop surfaces and restrained curvature. Do not
  turn every section into a rounded card or make the app look like a generic
  dashboard.
- Keep content left-aligned and readable. Let the active answer or analytical
  artifact dominate; keep metadata and technical details secondary.
- Use teal (`--brand: #008080`) for Fella presence and primary actions. It stays
  the same in light and dark modes. Reserve status colors for status and the
  chart palette for data series.
- Prefer type and spacing to extra borders, pills, and icons. Use a surface or
  border when it clarifies grouping, selection, or elevation.
- Use the same shared alignment, spacing, focus, loading, and disclosure
  treatments across views.

## Shared components and tokens

Use the existing shadcn-svelte component source and Bits UI behavior for common
buttons, fields, menus, dialogs, tabs, and disclosure controls. Extend a shared
component when a repeated pattern needs a new variant. Keep analytical
visualizations and domain-specific layouts Fella-owned, and preserve Fella's
tokens instead of introducing stock shadcn colors.

The theme and typography tokens are defined in `src/app.css`. Geist and Geist
Mono provide the type system; spacing follows a 4px rhythm. Keep controls
compact, with consistent icon sizing, accessible names, keyboard focus, and
reduced-motion behavior.

## App hierarchy

- Keep the native operating-system titlebar and window controls. Fella's
  toolbar controls should be compact and mostly icon-led.
- The workspace selector identifies the mounted folder/data scope.
- The left navigation is **Chat**, **Files**, and **Reports**. Keep the
  Investigation's **Brief**, **Sources**, **Questions**, **Analysis**, and
  **Results** tabs within its pane.
- Chat is the default. Notebook arranges Chat with the selected Investigation
  or tool in the same window. Reports opened beside Chat must shrink and wrap
  within the pane.
- Distinguish the mounted folder from the window layout. Switching to Notebook
  or opening a preview changes presentation only; it must preserve scope,
  drafts, and saved analytical state.
- Keep workspace context, Files, and Investigation Sources as distinct
  destinations because they serve different tasks.
- At compact window widths, prioritize the active surface instead of squeezing
  columns. Generated answers, tables, and menus must fit the available content
  width without page-level horizontal scrolling.

## Analytical content

Keep generated answers concise by default and reveal provenance, trace details,
and long result tables on demand. Use a consistent component library for
metrics, tables, charts, narrative, and disclosures. A table should use the
shared table treatment, readable column widths, and responsive overflow
behavior; it should not inherit code-block typography by accident.

The renderer is presentation-only. Keep numbers tied to saved result data and
show review state only when the underlying checks or interpretation warrant it.
Do not show every answer as “Needs review” by default.

## Accessibility and implementation

- Every interactive control needs an accessible name and visible
  `:focus-visible` state.
- Preserve keyboard operation and respect `prefers-reduced-motion`.
- Check both themes and compact window sizes when a design change affects
  shared components.
- Keep one primary action per surface. Use progressive disclosure for
  lower-frequency tools and technical detail.

Global colors, spacing, radii, focus, and motion belong in `src/app.css`.
Shared icons belong in `src/lib/components/Icon.svelte`. Component-specific
layout stays with its owning component when the pattern is genuinely local.
For runtime or domain ownership, see [Architecture](../architecture/ARCHITECTURE.md)
and the [product model](../product/PRODUCT-MODEL.md).

# Desktop UI component migration

Status: in progress. This is the working audit and sequence for standardizing
Fella's desktop interface with shadcn-svelte components and Bits UI
interactions.

## Goal and boundaries

Fella should feel like a considered analytical desktop app, not a generic
component showcase. Use a shared UI layer for repeated controls and interaction
patterns; keep Fella-specific information architecture, analytical content,
charts, and workspace composition purpose-built.

The target is the desktop application. Phone layouts and touch-first behavior
are out of scope. Resizing the desktop window must remain usable, but that is
not a separate mobile redesign effort.

The migration follows the product principles in [Interface design](DESIGN.md)
and the shell/workspace ownership model in
[Composable workspaces](COMPOSABLE-INTERFACE.md). shadcn-svelte is an
open-code source for reusable components, not a drop-in runtime component
package. Its current components use Tailwind CSS and Bits UI. Fella will own
the installed component source under `src/lib/components/ui/`, styled through
Fella's existing tokens. Bits UI should provide interaction behavior where
needed; feature components should not reimplement menus, dialogs, tabs, and
focus management independently.

## Current state

- The app is Svelte 5, SvelteKit, and Vite, rendered inside Electron.
- `src/app.css` already owns Geist typography, light/dark semantic colors,
  spacing, radii, focus treatment, reduced motion, and shared styles. Tailwind
  semantic utilities now map back to those tokens, and its global Preflight
  reset is intentionally not enabled.
- Tailwind v4, shadcn-svelte configuration, Bits UI, and the `cn` helper are in
  place. The first copied-source component is the shared Button, now used by
  the Guide actions. Most of the rest of the interface is still on its existing
  component-local styles; this is an incremental migration, not a wholesale
  visual reset.
- The large feature components combine Fella-specific behavior with repeated
  buttons, menus, tabs, disclosures, and form controls. Migrate those controls
  without replacing domain-specific composition. The root `+layout.svelte`
  only loads global CSS and has no component overhaul planned; the app shell in
  `+page.svelte` is included below because it owns cross-view UI and shortcuts.

## Design-system rules

1. `src/app.css` remains the canonical source for Fella colors, type, spacing,
   radii, focus, and motion. Tailwind's semantic names map to those tokens;
   do not create a second, drifting palette or replace Fella's warm light/dark
   scheme with stock defaults.
2. Preserve the existing `data-color-mode` theme behavior and verify controls
   against both themes. Keep Fella's restrained teal for primary actions and
   presence; status colors retain their semantic meanings.
3. Standardize variants and states: primary, secondary/outline, quiet, and
   destructive actions; normal, hover, focus-visible, disabled, and busy states.
   Icon-only buttons need accessible names and consistent dimensions.
4. Prefer typography, alignment, and spacing over extra cards, borders, pills,
   or decorative icons. Shared components do not mean every view gets the same
   layout.
5. Keep native Electron titlebar behavior, Fella branding, D3 chart rendering,
   workspace data ownership, and domain-specific views intact.
6. Migrate incrementally. Remove legacy local styles only after all consumers
   have moved and behavior is verified.

## Component audit

| Priority | Component | Planned treatment |
| --- | --- | --- |
| 1 | `+page.svelte` app shell | Standardize repeated shell actions and shared interaction states as consumers migrate. Preserve Fella's navigation, keyboard shortcuts, Electron titlebar, and view composition. |
| 1 | `Sidebar.svelte` | Use shared sidebar, collapsible, tooltip, and menu primitives. Preserve the workspace/repository/conversation hierarchy and its actions. |
| 1 | `Composer.svelte` | Standardize textarea, buttons, pickers, popovers, and menus. Split context/model/mode pickers and reference/action controls into focused pieces while preserving composer behavior. |
| 1 | `CommandPalette.svelte` | Use shared Dialog and Command primitives for results, filters, focus, and keyboard behavior. Keep Fella's commands and ranking. |
| 1 | `ProjectDialog.svelte` | Replace the hand-built modal shell and form controls with Dialog, Field, Input, Select, and Button primitives. |
| 1 | `SettingsView.svelte` | Standardize settings rows, switches, choices, disclosures, statuses, and actions. Keep settings organization and semantics specific to Fella. |
| 1 | `EnvironmentTabs.svelte` | Adopt shared tab behavior/styles while preserving environment switching, close controls, scrolling, and keyboard shortcuts. |
| 1 | `WorkspaceView.svelte` | Use shared tabs for Sources and Guide; preserve workspace-pane state and content. |
| 2 | `RunTimeline.svelte` | Use Collapsible/Accordion for run-step disclosure; retain trace ordering and technical detail. |
| 2 | `ReplayStatus.svelte` | Use shared Alert, Badge, Button, and loading/progress primitives for status and rerun states. |
| 2 | `SourcesView.svelte` | Standardize search, source rows/tables, pagination, disclosures, and statuses. Preserve the catalog/detail information architecture. |
| 2 | `SourcePreview.svelte` | Use shared table, scrolling, loading, and error primitives; preserve source-specific preview behavior. |
| 2 | `ContextView.svelte` | Use shared Button and Textarea/Field controls. Keep the file-backed guide workflow. |
| 2 | `ProjectView.svelte` | Standardize controls and typography, but keep the project/knowledge view's evidence-led composition bespoke. |
| 2 | `WorkspaceBoard.svelte` | Preserve the custom multi-pane layout and drag/drop. Standardize pane headers, actions, selected states, and menus. |
| 2 | `CompanionPane.svelte` | Keep the source/chart companion composition; standardize pane actions, scrolling, and status treatments. |
| 2 | `Transcript.svelte` | Keep the conversation and onboarding structure; use shared empty-state, action, and status treatments. |
| 2 | `Message.svelte` | Keep message/evidence rendering; use shared action buttons, tooltips, badges, and disclosures. |
| 2 | `EvidenceBlock.svelte` | Keep the specialized evidence layout; standardize tables, code surfaces, labels, statuses, and disclosure behavior. |
| 2 | `EvidenceSummary.svelte` | Use the shared disclosure pattern; consolidate with the evidence component if it has no distinct behavior. |
| 2 | `PythonCalculationDetails.svelte` | Keep calculation-specific code/details; standardize disclosure, code typography, and status labels. |
| 2 | `Chart.svelte` | Keep the D3 chart renderer and data encodings. Standardize its frame/actions and use shared table primitives for tabular data. |
| 3 | `DataLoader.svelte` | Replace the bespoke loader with a shared Spinner unless the branded treatment proves materially useful. |
| 3 | `Titlebar.svelte` | Preserve OS-specific window controls and draggable regions. Standardize only Fella-level icon buttons, focus, and tooltips. |
| 2 | `Icon.svelte` | Replace the hand-maintained generic glyph catalog with one consistent icon source and sizing convention. Keep product-specific brand marks separate. |
| Keep | `Logo.svelte` | Retain as Fella's branded orb/mark. |
| Keep | `ProviderIcon.svelte` | Retain provider-specific marks; do not substitute generic UI icons. |

## Delivery sequence

### Slice 1: foundation and first adoption

Status: complete.

- Add Tailwind v4 to the existing Vite pipeline without enabling Tailwind
  Preflight globally; Fella already has its own base styles.
- Add shadcn-svelte configuration and the shared `cn` helper.
- Map shadcn semantic color utilities onto Fella's existing light/dark tokens.
- Add the shared Button with a small, explicit variant/size API.
- Migrate the Guide view's action buttons as the first consumer.

Validation: `pnpm check` reports zero errors and warnings, `pnpm build`
succeeds, and the Guide view was inspected in light and dark themes. A keyboard
smoke test focused “Use a template,” pressed Enter, and confirmed the template
appeared and Save became enabled. The browser smoke used temporary in-memory
workspace state and did not write a guide file.

### Slice 2: interaction primitives

Add and adopt Tabs, Dialog, DropdownMenu/ContextMenu, Popover/Combobox/Command,
Collapsible/Accordion, Switch/RadioGroup/Select, Tooltip, ScrollArea, and
standardized form fields as their first real consumers are migrated. Prioritize
environment/workspace tabs, project creation, the command palette, and Settings.

### Slice 3: high-complexity product views

Refactor Composer and Sidebar around the shared primitives. Then migrate source,
evidence, transcript, and project views while retaining their distinct product
layouts. Keep chart geometry and workspace-board composition custom.

### Slice 4: converge and remove duplicates

Replace remaining ad hoc controls, delete unused `.pill`, `.rowbtn`, and
component-local duplicate styles, then audit the complete desktop UI in light
and dark themes.

## First-slice acceptance criteria

- `pnpm check` reports no new Svelte or TypeScript errors, and `pnpm build`
  succeeds.
- The shared Button accepts standard button attributes and has consistent
  primary, outline/secondary, ghost/link, destructive, size, disabled, and
  focus-visible behavior. Only variants needed by the first consumer need to
  be visually exercised in this slice.
- Guide's Choose folder, Use a template, and Save actions preserve their
  callbacks, disabled logic, and keyboard activation.
- Light and dark modes use the existing Fella tokens; no stock palette or
  global reset unexpectedly restyles existing screens.
- Validation targets the desktop application and keyboard operation. No
  phone-specific layout or touch acceptance criteria are introduced.

## Reference documentation

- [shadcn-svelte introduction](https://shadcn-svelte.com/docs)
- [shadcn-svelte Vite setup](https://shadcn-svelte.com/docs/installation/vite)
- [shadcn-svelte theming](https://shadcn-svelte.com/docs/theming)
- [Bits UI introduction](https://bits-ui.com/docs/introduction)
- [Tailwind CSS Preflight](https://tailwindcss.com/docs/preflight)

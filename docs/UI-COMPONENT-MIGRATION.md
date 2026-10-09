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
| 1 | `Sidebar.svelte` | Adopted shared Collapsible, Tooltip, and DropdownMenu for repository disclosure, sidebar hints, and repository actions. Preserve the workspace/repository/conversation hierarchy. |
| 1 | `Composer.svelte` | Standardize textarea, buttons, pickers, popovers, and menus. Split context/model/mode pickers and reference/action controls into focused pieces while preserving composer behavior. |
| 1 | `CommandPalette.svelte` | Adopted shared Dialog and Input for modal/focus behavior and search entry. Keep the Fella-specific result list, filters, commands, and ranking; reconsider Command only if it improves that real workflow. |
| 1 | `ProjectDialog.svelte` | Replaced the hand-built modal shell and adopted shared Dialog, Input, Select, and Button. Form labels, validation, and project creation remain feature-owned. |
| 1 | `SettingsView.svelte` | Adopted shared Switch controls. Settings rows, choices, disclosures, statuses, and organization remain feature-owned until those patterns have multiple consumers. |
| 1 | `EnvironmentTabs.svelte` | Adopted shared Tabs behavior for environment switching and roving keyboard focus. Keep titlebar-specific close/new controls, scrolling, and shortcuts feature-owned. |
| 1 | `WorkspaceView.svelte` | Adopted shared Tabs for Sources and Guide; preserve workspace-pane state and content. |
| 2 | `RunTimeline.svelte` | Adopted shared Collapsible for run-step disclosure; retain trace ordering and technical detail. |
| 2 | `ReplayStatus.svelte` | Adopted shared Alert, Badge, Button, and Spinner for status, retry, and rerun states. |
| 2 | `SourcesView.svelte` | Standardize search, source rows/tables, pagination, disclosures, and statuses. Preserve the catalog/detail information architecture. |
| 2 | `SourcePreview.svelte` | Use shared table, scrolling, loading, and error primitives; preserve source-specific preview behavior. |
| 2 | `ContextView.svelte` | Adopted shared Textarea styling for the guide editor while retaining its file-backed workflow and feature-specific presentation. |
| 2 | `ProjectView.svelte` | Standardize controls and typography, but keep the project/knowledge view's evidence-led composition bespoke. |
| 2 | `WorkspaceBoard.svelte` | Preserve the custom multi-pane layout and drag/drop. Standardize pane headers, actions, selected states, and menus. |
| 2 | `CompanionPane.svelte` | Keep the source/chart companion composition; standardize pane actions, scrolling, and status treatments. |
| 2 | `Transcript.svelte` | Keep the conversation and onboarding structure; use shared empty-state, action, and status treatments. |
| 2 | `Message.svelte` | Keep message/evidence rendering; use shared action buttons, tooltips, badges, and disclosures. |
| 2 | `EvidenceBlock.svelte` | Keep the specialized evidence layout; standardize tables, code surfaces, labels, statuses, and disclosure behavior. |
| 2 | `EvidenceSummary.svelte` | Use the shared disclosure pattern; consolidate with the evidence component if it has no distinct behavior. |
| 2 | `PythonCalculationDetails.svelte` | Keep calculation-specific code/details; standardize disclosure, code typography, and status labels. |
| 2 | `Chart.svelte` | Keep the D3 chart renderer and data encodings. Standardize its frame/actions and use shared table primitives for tabular data. |
| 3 | `DataLoader.svelte` | Removed; loading consumers use the shared, reduced-motion-aware Spinner. |
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

Status: implemented, with one browser-test assertion unresolved.

- Add shared Dialog, DropdownMenu, Input, Select, Switch, Tabs, Textarea, and
  Tooltip components, plus a Collapsible wrapper, all styled from Fella tokens.
- Adopt them in the command palette, project creation, Settings, workspace-pane
  tabs, repository actions, run-step disclosure, and guide editing.
- Keep feature-owned result rendering, settings layout, and domain-specific
  content in their existing components.
- Defer ContextMenu, Popover, Combobox, Command, Accordion, RadioGroup,
  ScrollArea, and broader Field abstractions until a concrete consumer needs
  them. A component inventory is not a reason to add unused primitives.

Validation: `pnpm check` passed with zero errors and warnings, `pnpm build`
passed, and the five pre-existing workspace-board browser tests passed. The
new combined keyboard test passed its palette, workspace-tab, and project
dialog checks, then failed when Playwright could no longer resolve the
repository-action trigger by role after clicking it. A temporary diagnostic
captured the trigger with `aria-expanded="true"` and an open menu containing
the expected `role="menuitem"`; this is not evidence of a user-visible menu
failure, but the browser assertion remains unresolved and is not counted as a
pass. Do not treat this as verified until a distinct, reliable interaction
check covers opening, Escape dismissal, and focus restoration.

### Slice 3: high-complexity product views

Status: in progress. Deliver this broad area in consumer-sized sub-slices, each
with its own validation and commit.

#### Slice 3a: environment tabs

Status: complete.

- Adopt Bits UI Tabs for environment selection, automatic activation, and
  roving keyboard focus.
- Preserve titlebar-specific close/new controls, horizontal overflow and
  active-tab visibility, existing click focus behavior, and keyboard focus
  restoration after environment activation.

Validation: `pnpm check` reports zero errors and warnings, `pnpm build`
succeeds, and all five environment-related workspace-board browser tests pass.
An initial Playwright invocation forwarded its filter incorrectly and exposed
a `ref` binding runtime error; the binding was corrected, then the focused test
and the full environment-related subset passed. The separate repository-menu
assertion caveat recorded under Slice 2 remains unresolved.

#### Slice 3b: Composer mode selection

Status: implemented and browser-verified.

- Replace the bespoke Ask/Check data menu with the shared DropdownMenu radio
  interaction while preserving its Fella-specific descriptions and compact
  Composer styling.
- Leave the multi-source and model pickers custom; they need separate
  interaction designs.

Validation: `pnpm check`, `node --check tests/e2e/workspace-board.spec.mjs`,
and `pnpm build` pass; the build reports a 501 KB client chunk over Vite's
500 KB advisory threshold. The test separately verifies pointer-open Escape
dismissal with focus restoration and keyboard-open selection. After the
user-approved locator correction to match the selected “Check data” accessible
name, the focused browser test passed. The earlier mixed pointer/keyboard
attempts were test interaction issues, not evidence of a product failure.

#### Slice 3c: Composer text entry

Status: implemented.

- Use the shared Textarea primitive for the main prompt and the free-form
  clarification response, keeping their distinct sizing, focus, and resize
  behavior in Composer-owned styles.
- Preserve the main prompt's combobox semantics, autoresize/ref behavior,
  keyboard handlers, and secret-entry masking.

Validation: `pnpm check` reports zero errors and warnings; the focused
General-composer workspace-board browser test passes; and `pnpm build`
succeeds. The build still reports the client-chunk warning at 501.59 KB and
the adapter-static fallback-page notice. There is no dedicated deterministic
browser test for the free-form clarification field, so that flow is not
claimed as browser-verified.

#### Slice 3d: Composer source search

Status: implemented.

- Use the shared Input primitive for source and field search, while retaining
  the Composer's compact search-row treatment.
- Give the search field an explicit accessible name and preserve automatic
  focus, filtering, and source attachment.

Validation: `pnpm check` reports zero errors and warnings; the focused
workspace-board test for filtering and attaching a source passes; and
`pnpm build` succeeds. The build reports a 501.67 KB client chunk above Vite's
500 KB advisory and the adapter-static fallback-page notice.

#### Slice 3e: Composer model search

Status: implemented.

- Use the shared Input primitive in the model picker while preserving its
  compact search-row styling, automatic focus, and filtering behavior.
- Keep model selection and picker dismissal feature-owned.

Validation: `pnpm check`, `node --check tests/e2e/workspace-board.spec.mjs`,
and `pnpm build` succeed. The focused model-search browser test verifies
focus, the empty result state, and filtering back to the available model.
The build reports a 501.69 KB client chunk above Vite's 500 KB advisory and
the adapter-static fallback-page notice.

#### Slice 3f: Sidebar navigation actions

Status: implemented.

- Use the shared ghost Button for Ask, Search, and Settings while preserving
  the Sidebar's compact row dimensions, labels, shortcut behavior, and active
  Settings state.
- Keep icon-only and workspace-specific actions feature-owned for their
  distinct hit areas and hover behavior.

Validation: `pnpm check`, `node --check tests/e2e/workspace-board.spec.mjs`,
the focused Sidebar navigation browser test, and `pnpm build` succeed. The
test verifies shared Button usage, compact row height, Search keyboard access
and focus restoration, and the active Settings marker. The build reports a
501.97 KB client chunk above Vite's 500 KB advisory and the adapter-static
fallback-page notice.

#### Slice 3g: Sidebar icon actions

Status: implemented.

- Use the shared icon Button for sidebar collapse and Add repository while
  preserving their icon-only labels, tooltips, compact hit areas, and actions.

Validation: `pnpm check`, `node --check tests/e2e/workspace-board.spec.mjs`,
the focused icon-action browser test, and `pnpm build` succeed. The test
verifies shared Button usage, both hit areas remain at most 32 px square, and
Add repository opens the board. The build reports a 502.07 KB client chunk
above Vite's 500 KB advisory and the adapter-static fallback-page notice.

#### Slice 3h: Sidebar workspace tools

Status: implemented.

- Use the shared ghost Button for Sources, an existing Project, and Add
  project actions, preserving selected-state styling and compact row geometry.
- Keep repository structure and navigation state owned by the Sidebar.

Validation: `pnpm check`, `node --check tests/e2e/workspace-board.spec.mjs`,
the focused workspace-tools browser test, and `pnpm build` succeed. The test
verifies shared Button usage, compact row height, source navigation, and the
create-project flow. The build reports a 502.25 KB client chunk above Vite's
500 KB advisory and the adapter-static fallback-page notice.

#### Slice 3i: Sidebar conversation actions

Status: implemented.

- Use shared icon Buttons for conversation rename and delete actions, with
  24 px hit targets and the existing neutral/destructive color treatment.
- Preserve inline rename, Escape cancellation, and history deletion behavior.

Validation: `pnpm check`, `node --check tests/e2e/workspace-board.spec.mjs`,
the focused seeded-history browser test, and `pnpm build` succeed. The test
verifies both actions use the shared primitive, Escape leaves the title
unchanged, and Delete removes the history row. The build reports a 502.31 KB
client chunk above Vite's 500 KB advisory and the adapter-static fallback-page
notice.

#### Slice 3j: Sources catalog filter

Status: implemented.

- Use the shared Input for the workspace source filter while keeping the
  compact searchbox presentation and existing filtering behavior.
- Preserve selection when a filter hides and then restores the selected source.

Validation: `pnpm check`, `node --check tests/e2e/workspace-board.spec.mjs`,
the focused Sources filter browser test, and `pnpm build` succeed. The test
checks the empty state and restored selection. The build reports a 502.32 KB
client chunk above Vite's 500 KB advisory and the adapter-static fallback-page
notice.

#### Slice 3k: Project title and wiki editing

Status: implemented.

- Use shared Input and Textarea primitives for the inline project title and
  wiki editor while preserving the title's heading treatment and the wiki's
  quiet, borderless writing surface.
- Keep title commit-on-blur and immediate local wiki updates unchanged.

Validation: `pnpm check`, `node --check tests/e2e/workspace-board.spec.mjs`,
the focused ProjectView browser test, and `pnpm build` succeed. The test
verifies both shared controls, title rename propagation, and wiki retention
after switching to Sources and back. The build reports a 502.31 KB client
chunk above Vite's 500 KB advisory and the adapter-static fallback-page notice.

#### Slice 3l: Project actions

Status: implemented.

- Use shared Buttons for Ask repository, Sources, Mount repository, and Delete
  project while retaining their quiet, compact styling and existing actions.
- Remove the duplicate Mount repository action from the project snapshot; the
  mount note already presents it when the project is not mounted.

Validation: `pnpm check`, `node --check tests/e2e/workspace-board.spec.mjs`,
the focused ProjectView actions browser test, and `pnpm build` succeed. The
focused test verifies shared Button markers, Sources and Ask navigation, the
delete confirmation, and project removal. Two bounded test-only corrections
were needed: the workspace composer has a workspace-specific accessible name,
and the native confirmation must be accepted concurrently with the click.
The behavior assertions were preserved. The build reports a 502.20 KB client
chunk above Vite's 500 KB advisory and the adapter-static fallback-page notice.

#### Slice 3m: Analysis details disclosure

Status: implemented.

- Use the shared ghost Button for the answer's Analysis details control while
  preserving its compact appearance, accessible expanded state, and toggle.

Validation: `pnpm check`, `node --check tests/e2e/workspace-board.spec.mjs`,
the focused Analysis details browser test, and `pnpm build` succeed. The test
verifies the shared Button marker, collapsed and expanded ARIA state, the
evidence body appearing, and the body closing again. The build reports a
502.27 KB client chunk above Vite's 500 KB advisory and the adapter-static
fallback-page notice.

#### Slice 3n: Sources preview action

Status: implemented.

- Use the shared outline Button for the selected source's Open beside action,
  retaining its compact detail-header styling and preview-only behavior.

Validation: `pnpm check`, `node --check tests/e2e/workspace-board.spec.mjs`,
the focused Sources preview browser test, and `pnpm build` succeed. The test
verifies the shared Button marker, source preview in the companion pane, and
that opening the preview does not change the active conversation scope. The
build reports a 502.31 KB client chunk above Vite's 500 KB advisory and the
adapter-static fallback-page notice.

#### Slice 3o: Suggested follow-up actions

Status: implemented.

- Use the shared link Button for suggested follow-up questions, preserving the
  understated inline treatment and direct question submission.

Validation: `pnpm check`, `node --check tests/e2e/workspace-board.spec.mjs`,
the focused suggested-follow-up browser test, and `pnpm build` succeed. The
test verifies the shared link Button and confirms the selected follow-up
appears as a new user turn followed by the mock response. The build reports a
502.34 KB client chunk above Vite's 500 KB advisory and the adapter-static
fallback-page notice.

#### Slice 3p: Workspace starter prompts

Status: implemented.

- Use shared outline Buttons for the three workspace starter prompts, retaining
  their compact, left-aligned prompt-chip presentation.

Validation: `pnpm check` reports zero errors and warnings,
`node --check tests/e2e/workspace-board.spec.mjs`, the focused starter-prompt
browser test, and `pnpm build` succeed. The test verifies the shared Button
marker, prompt submission as a user turn, and the corresponding workspace
response. The build reports a 502.35 KB client chunk above Vite's 500 KB
advisory and the adapter-static fallback-page notice.

#### Slice 3q: Companion pane close

Status: implemented.

- Use a shared compact icon Button for closing the companion pane, retaining its
  30 px target, accessible name, and return-to-conversation behavior.

Validation: `pnpm check` reports zero errors and warnings,
`node --check tests/e2e/workspace-board.spec.mjs`, the focused companion-close
browser test, and `pnpm build` succeed. The test verifies the shared Button
marker, 30 px dimensions, pane dismissal, and return to the same workspace
conversation. The build reports a 502.39 KB client chunk above Vite's 500 KB
advisory and the adapter-static fallback-page notice.

#### Slice 3r: Sources pagination

Status: implemented.

- Use shared compact ghost Buttons for both source-catalog and skipped-file
  pagination, preserving page ranges, boundaries, and disabled states.

Validation: `pnpm check` reports zero errors and warnings, the focused pagination
browser test and `node --check tests/e2e/workspace-board.spec.mjs` pass, and
`pnpm build` succeeds. The test uses 205 sources and 105 skipped files to check
both pagers' ranges, disabled boundaries, and shared Button markers. The build
reports a 502.70 KB client chunk above Vite's 500 KB advisory and the
adapter-static fallback-page notice.

#### Slice 3s: Status and loading primitives

Status: implemented.

- Add token-aware shared Alert, Badge, and Spinner primitives.
- Use them for replay freshness, retry errors, rerun actions, and existing
  loading states; remove the one-off DataLoader component.

Validation: `pnpm check` reports zero errors and warnings, the two focused
ReplayStatus browser tests pass, `node --check tests/e2e/workspace-board.spec.mjs`
passes, and `pnpm build` succeeds. The browser tests verify warning-state
freshness and the shared rerun action, plus the accessible error alert and
retry behavior. The build reports a 504.08 KB client chunk above Vite's 500 KB
advisory and the adapter-static fallback-page notice.

#### Slice 3t: Empty-state, onboarding, and Settings actions

Status: implemented.

- Migrate remaining `.pill` actions in Sources, Transcript, and Settings to
  shared Button variants, including compact provider choices and inline links.
- Remove the obsolete global `.pill` styles while preserving the separate
  custom row-action primitive for Composer and Sidebar.

Validation: `pnpm check`, `node --check tests/e2e/workspace-board.spec.mjs`,
the focused Sources and Settings browser tests, and `pnpm build` pass. The
Sources test initially expected a source heading after picking a folder, but
the captured UI showed the existing behavior opens that folder as a workspace
conversation. With the user's approval, the assertion now checks the opened
workspace and composer instead. This was a test expectation correction, not a
product behavior change. The build still reports the existing Vite chunk-size
advisory and adapter-static notice.

#### Slice 3u: Composer completion and Sidebar conversation rows

Status: implemented.

- Replace the global row button with shared ghost Buttons in the Composer's
  slash-command list and Sidebar conversation history.
- Preserve the listbox's keyboard selection and compact Sidebar row geometry
  with narrowly scoped feature styles, then remove the unused global `.rowbtn`.

Validation: `pnpm check`, `node --check tests/e2e/workspace-board.spec.mjs`,
the focused Composer completion and Sidebar conversation tests, and
`pnpm build` pass. The browser test verifies completion keyboard selection,
input completion, popup dismissal, and shared Button adoption. The build keeps
the existing client chunk-size advisory and adapter-static fallback notice.

#### Slice 3v: Sources rows and skipped-file search

Status: implemented.

- Use shared ghost Buttons for selectable source rows while preserving their
  listbox option semantics, selected state, and compact grid geometry.
- Use the shared Input for filtering skipped files and retain the existing
  search and pagination behavior.

Validation: `pnpm check` reports zero errors and warnings,
`node --check tests/e2e/workspace-board.spec.mjs`, the focused Sources filter
and pagination browser tests, and `pnpm build` succeed. The tests verify the
source row's shared Button marker and selected state, skipped-file filtering,
the page reset after filtering, and existing 105-file pagination boundaries.
The build reports a 504.90 KB client chunk above Vite's 500 KB advisory and
the adapter-static fallback-page notice.

#### Slice 3w: Companion snapshot recovery

Status: implemented.

- Use a shared outline Button for reopening a source preview against the
  current catalog snapshot, preserving the stale-snapshot recovery behavior.

Validation: `pnpm check` reports zero errors and warnings,
`node --check tests/e2e/workspace-board.spec.mjs`, the focused companion
recovery browser test, and `pnpm build` succeed. The test reindexes to a newer
revision, confirms the stale-snapshot notice and shared Button, then reopens
the current source version. The build reports a 505.02 KB client chunk above
Vite's 500 KB advisory and the adapter-static fallback-page notice.

#### Slice 3x: Chart companion action

Status: implemented.

- Use a shared outline Button for opening a rendered chart beside the
  conversation, preserving the D3 chart and linked conversation reference.

Validation: `pnpm check` reports zero errors and warnings,
`node --check tests/e2e/workspace-board.spec.mjs`, the focused chart companion
browser test, and `pnpm build` succeed. The test checks shared Button adoption,
the chart title in the companion pane, and the message's linked-chart state.
The build reports a 505.01 KB client chunk above Vite's 500 KB advisory and
the adapter-static fallback-page notice.

#### Slice 3y: Evidence detail disclosures

Status: implemented.

- Use shared quiet Buttons for model-timing and per-step evidence disclosures,
  preserving their changing labels and expanded state.

Validation: `pnpm check` reports zero errors and warnings,
`node --check tests/e2e/workspace-board.spec.mjs`, the focused evidence
disclosure browser test, and `pnpm build` succeed. The test verifies that a SQL
query can be revealed and hidden. Its initial locator used the closed-state
label after expansion; the captured DOM showed the control correctly changed
to “hide,” so the locator now follows the stable disclosure element and the
label/state assertions remain explicit. The build reports a 505.10 KB client
chunk above Vite's 500 KB advisory and the adapter-static fallback-page notice.

#### Slice 3z: Compact Transcript setup actions

Status: implemented.

- Use shared link Buttons for inline sign-in, model-selection, and connection
  guidance in the compact mid-conversation setup state.
- Preserve sentence flow and dispatch the same setup command.

Validation: `pnpm check` reports zero errors and warnings,
`node --check tests/e2e/workspace-board.spec.mjs`, the focused Transcript setup
browser test, and `pnpm build` succeed. The test confirms the inline action is
a shared Button and still dispatches the sign-in command. The build reports a
505.36 KB client chunk above Vite's 500 KB advisory and the adapter-static
fallback-page notice.

#### Slice 3aa: Composer contextual actions

Status: implemented.

- Use shared Buttons for closing the source picker, adding/removing context
  references, opening the provider settings view, and reopening an unavailable
  workspace.
- Preserve compact picker and reference geometry, existing accessible names,
  and the unavailable-workspace state.

Validation: `pnpm check` reports zero errors and warnings,
`node --check tests/e2e/workspace-board.spec.mjs`, the focused Composer source,
model, and history-only workspace browser tests, and `pnpm build` succeed. The
first combined test run passed the two Composer tests; the history-only test
locator was ambiguous between the workspace tile and the exact in-composer
action, so it was scoped by exact accessible name and passed on rerun. One
immediate rerun could not start Vite; process inspection showed no leftover
server, and the next run passed. The build reports a 505.59 KB client chunk
above Vite's 500 KB advisory and the adapter-static fallback-page notice.

#### Slice 3ab: WorkspaceBoard pane controls

Status: implemented.

- Use shared ghost Buttons for pane focus, close, and inactive-pane preview
  actions.
- Preserve the board's compact header controls, focus styling, and preview
  surface rather than inheriting the shared button's default geometry.

Validation: `pnpm check`, `node --check tests/e2e/workspace-board.spec.mjs`,
the focused WorkspaceBoard browser tests, and `pnpm build` succeed. The first
test invocation caught a duplicate local name in a newly added assertion before
tests ran; the identifier was corrected, and the rerun passed. The browser tests
verify shared Button adoption, 26px close controls, preview and header focus
behavior, and the existing four-pane close/reopen flow. The build reports the
existing client-chunk advisory and adapter-static fallback notice.

#### Remaining in Slice 3

Standardize remaining feature actions and controls around shared primitives
where they improve consistency without obscuring product behavior. Audit
Sidebar reconnect and fallback actions, CommandPalette result rows, and
Settings option groups next. Keep keyboard-driven listbox options, native
window controls, chart geometry, and workspace-board composition custom unless
a shared primitive improves the interaction without changing its semantics.

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

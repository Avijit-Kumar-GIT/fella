# Reference workspace redesign

Reference: `3a8eb590-7680-42fd-b82b-2afcc2a7624d`, supplied by the user.
The React prototype supplies the visual specification; Svelte retains Fella’s real
workspace, conversations, sources, findings, evidence, reports and native actions.

## Acceptance criteria fixed before visual validation

- Inter for UI and conversation text; JetBrains Mono for data, source paths, SQL,
  and the model selector. Both fonts load locally, without a network request.
- Sidebar: 268px; traffic lights and header controls on one row; 36px workspace
  mark; 14px Fella label; 12px workspace label; Chat / Files / Reports pills;
  32px conversation icons, 13px titles, 12px previews, 12px footer padding.
- Conversation: workspace chip centered in the conversation pane, 12px from its
  top; reading width at most 520px; 20px horizontal insets; 13px / 21px body;
  user bubble at most 420px; 16px answer radius, 16px horizontal answer padding.
  The composer spans its pane with 20px horizontal and 16px bottom insets and
  contains context, text, current model, and send/stop. It reflows at narrow widths.
- Details: Investigation and Files default to 380px; Reports to 320px. Resizing
  preserves a usable primary pane. The Investigation header starts at the top of
  the window; its 48px mark and title are centered independently of rename actions.
  All five tabs fit one line. Scrolling a section leaves header and tabs stationary.
- Questions: one add field, numbered rows with truthful status and saved-answer
  excerpts; expand a row to edit, run, inspect sources, or open saved answers.
- Analysis: run selection and recorded execution steps, with SQL disclosed on
  request. Detailed records and finding management remain available separately.
- Results: headline, saved findings or answers, Create report, then detailed
  evidence and coverage. No sample claims, metrics, or approval states are invented.
- Files: heading starts 64px down with 32px side insets; 22px title; rounded search
  and type filters; Name / Rows / Updated / Status columns (72 / 76 / 104px fixed
  secondary columns at full width); file details use a stacked 44px icon, 16px
  filename, and 11.5px monospaced path. Secondary table controls are disclosed.
- Reports: centered article with 600px content width and 48px top spacing, 30px /
  38px title, explicit Edit / Done, and Outline / Review / Sources / History panel.
  Opening a saved report selects Reports. Refreshing its details keeps content visible.
- At 1440×960 and 1024×768: centered content, readable columns, no document-level
  horizontal overflow. At 680×800: navigation and panels adapt without clipping;
  the user can reach Chat and Investigation, enter text, and operate the model picker.
- Section changes retain the live conversation and its draft. Adding a question
  persists the text. New conversation does not create an Investigation. No model
  calls are required to switch sections, inspect saved results, or open a report.

## Validation scope

Run Svelte diagnostics and the production build. Review desktop screenshots with
a disposable local profile and deterministic saved data at the sizes above. Record
visual observations and failures separately from compilation results. Do not run
unrelated model, backend, packaging, or broad application suites for this redesign.

## Validation results

- `pnpm check`: zero errors and zero warnings.
- `pnpm build`: passed; the existing large-chunk advisory remains.
- `node --check electron/main.mjs` and `git diff --check`: passed.
- Focused Linux Electron review: passed at 1440×960, 1024×768 and 680×800.
  The measured sidebar is 268px, Investigation pane 380px, identity mark 48px;
  the workspace chip and identity mark centers differ from their pane centers
  by less than 1px. All five Investigation tabs fit at 1024px. No document-level
  horizontal overflow or browser errors were recorded. Local Inter and JetBrains
  Mono are active. A question added through the UI is persisted, the initial
  four-question plan survives navigation, section scrolling leaves navigation
  stationary, and the chat draft survives Files / Chat navigation.
- Eleven screenshots cover Chat, all Investigation sections, Files, Reports,
  narrow layouts and the model picker. Successful capture artifacts are local:
  `/tmp/fella-reference-redesign-skErZU/`. Screenshots are deliberately untracked.
- Reproduce the focused review with `node scripts/review-reference-workspace.mjs`
  after building the frontend and preparing the Electron engine. It uses a new
  disposable local profile and prints its capture directory and measurements.

### Failures found and corrected

The first capture attempt stopped when an exact-text locator found the hidden
Brief objective rather than the visible question row. The follow-up targets the
visible row and uses a fresh, unopened fixture conversation. No existing app-test
assertions were changed. The failed capture remains at
`/tmp/fella-reference-redesign-d0d3Vl/`.

The fresh-fixture review failed two persistence criteria: the initial plan and
an added question were overwritten by empty drafts during pane navigation.
The failed result remains at `/tmp/fella-reference-redesign-8BEPCd/`. Inspection
showed that an Investigation pane could save its uninitialized draft before its
hydration effects ran. Each editable draft now records the Investigation it was
loaded from; dirty checks and saves require matching ownership. The same fresh
fixture, persistence assertions and geometry criteria then passed unchanged.

The file captures also exposed source modification times interpreted as
milliseconds even though the catalog supplies seconds. Both file-date displays
now use the catalog unit.

### Limits

This validates rendered layout and deterministic saved-work interactions on
Linux Electron, not live model quality or native Windows/macOS behavior. The
fixture has no model credentials, so its genuine connection notice is visible.
No broad app suites, live-model benchmarks or unrelated backend tests were run.

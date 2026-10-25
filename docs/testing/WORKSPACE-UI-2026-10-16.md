# Workspace UI revision — acceptance contract

> Historical desktop validation record. Some labels and surfaces below
> describe the earlier Tools/Shell UI. Use the current
> [interaction model](../product/INTERACTION-MODEL.md) for present navigation.
> The automated test sources and commands cited here have since been
> consolidated under the root `tests/` directory.

Recorded before implementation. This changes presentation and navigation, not
analysis, permissions, coverage grading, provenance, or stored user records.

1. Workspace selection has one primary entry point: the workspace dropdown.
   Chats and Files in the sidebar both belong to the selected workspace.
   General, adding folders, reconnecting history-only workspaces, and global
   search remain reachable. Switching workspaces never moves a conversation.
2. The conversation remains the default. Tools opens/closes predictably, uses
   a keyboard/pointer-resizable split when enough space is available, and can
   expand. At compact desktop sizes, the active Tools surface gets the working
   area instead of becoming a tiny lower pane. Returning to chat preserves it.
3. Every right-pane entry point uses the same pane frame. File/chart previews
   preserve their source/answer identity and freshness protections. Tool tabs
   remain independent, and newly selected tabs scroll into view.
4. Investigation headers are compact. The full goal stays in Brief; long
   titles cannot displace section navigation or the first working control.
   Section navigation remains one line, keyboard accessible, and independent
   of a required analytical sequence.
5. Questions appear before optional definitions. Coverage remains available,
   without duplicating the full question list by default. Completion belongs
   with Results/Share. Analytical work is visible without opening a generic
   history disclosure. An unlinked answer is not relabeled as an answered
   Question, nor is a flagged result silently accepted.
6. Workspace context, Investigation sources, and file browsing have distinct
   names. Context actions fit the pane; an empty guide does not push learned
   knowledge below a large empty editor. Autosave and analytical state survive
   view changes. Review states remain neutral and actionable.
7. Check desktop widths 1440, 1280, 1100, and 960, light/dark appearance,
   workspace/conversation switching, Tools resizing/expansion, multiple tabs,
   Investigation sections, source previews, and Settings. Keep the established
   Fella fonts, colors, curvature, and user-only Shell boundary.

Existing tests are not rewritten to fit this revision. Any baseline failures
or tests targeting intentionally retired navigation are reported separately
from new acceptance tests; no broad-suite pass is inferred from targeted tests.

## Validation

Initial acceptance run: all six cases failed in shared setup because its exact
conversation locator assumed an untruncated title. The actual archive title is
capped at 80 characters. The user approved correcting that setup locator to
the question's distinctive prefix (suite v1.1); the transcript's full original
request assertion is unchanged. The same prefix is used for the sidebar's
negative scope check. Initial failures are retained separately; v1.1 is not
reported as an untouched baseline.

### Changes and final evidence

- Sidebar now contains the current workspace's Chats or Files. The dropdown
  owns workspace switching and Add workspace; global search is retained.
- Shared panel layout supports mouse/keyboard resizing, saved width, expanded
  Tools, and a full working pane at compact desktop widths. It does not unmount
  the conversation merely because Tools takes priority.
- Scoped chart/source entry points use Tools tabs. Chart and older preview
  references retain their original identity and freshness checks. Legacy
  General-only companion archives retain a compatibility rendering path in
  the shared frame; they are not reinterpreted as current workspace sources.
- Investigation title is 18px, at most two lines. The current conversation's
  Open conversation action is omitted. Sources/context/files are distinct.
- Questions precede optional definitions, use readable/editable text, and keep
  coverage details available. Completion is in Results. Recorded analysis
  opens expanded. Existing unlinked runs can be reviewed without marking the
  Question answered or running a new model request.
- Context headers/actions fit their pane, the guide starts at 160px rather
  than 56vh, and secondary model-connection management is disclosed in
  Settings. First-run model setup is available without competing with the
  folder action. New conversation also now leaves Settings correctly.

After the approved locator correction, v1.1 had **5 passes, 1 failure**. The
last case demonstrated a product bug: New conversation created a thread but
left Settings visible. The fix explicitly navigates new conversations to chat;
explicit workspace selection also leaves Settings. No assertion was weakened.

Suite v1.2 added Settings-to-workspace/dark-mode coverage: **7 passed**.
Suite v1.3 added Add workspace, history-only reconnect, picker keyboard, and
cross-workspace search coverage: **9 passed**, 16.4 seconds. Existing cases
kept the same expectations. Screenshots were visually inspected at 1440px
and 960px, light and dark; assertions also cover 1280px and 1100px. This was
Electron + real Rust on WSL2/Linux, not a native Windows/macOS run. The suite
submits no hosted-model requests and uses disposable profiles only.

| Check | Result |
| --- | --- |
| `pnpm check` | Passed, 0 errors / 0 warnings |
| `pnpm build` | Passed; large-chunk advisory remains |
| `pnpm test:e2e:desktop-ui` | 9 passed (v1.3) |
| `pnpm test:workspace-tool-tabs` | 2 passed |
| `pnpm test:investigation-brief-proposal` | Passed, 7 behaviors |
| `pnpm test:investigation-runs` | Passed, 4 behaviors |
| `pnpm test:investigation-report` | Passed |
| `pnpm test:electron-terminal` | Passed |
| `pnpm test:electron-bridge` | Passed |
| `pnpm test:workbench` | 9 passed, 1 failed: the unchanged SSR test expects a global-sidebar History-only label while General is selected; the sidebar is now workspace-scoped. The new real-desktop reconnect journey passed. |
| `pnpm test:chart-renderer` | Failed on unchanged raw-HTML regex `>Open beside</button>`; generated Svelte comments intervene. The new actual file/chart Tools-tab journey passed. |

The full legacy workspace browser suite was not rerun or migrated in this
revision: many cases target removed all-workspace rows, Environment labels,
Data & context labels, always-visible question textareas, or the previous
completion location. This is an explicit coverage limitation, not a full-suite
pass. Its last prior baseline and failures are in the performance audit.
No existing tests or their assertions were edited.

Reproduce after `pnpm electron:build` with `pnpm test:e2e:desktop-ui`.
Local screenshots, traces, and first-run failures are ignored under
`test-results/performance/2026-10-16-ui-implementation/`; media is not committed.

## Investigation and answer-review follow-up

The following follow-up contract and validation record were previously maintained in a separate file. They are included here to keep the dated desktop UI evidence together; their failures, revisions, and test limitations are preserved as recorded.
## Workspace UI follow-up — acceptance contract v1

Defined before implementation, following the October 16 UI/UX audit.

1. Electron mutations accept nested reactive renderer data as plain JSON. Saving
   Question/run associations persists exactly those associations, never rewrites
   the original run or silently promotes Question coverage. A failed update keeps
   the loaded answer and editing controls available with an actionable error.
2. An Investigation can be named at creation and renamed later. Names are trimmed,
   nonempty, at most 120 characters, and persisted by stable Investigation ID.
   Renaming changes no Question, brief, conversation text, status, source scope,
   or historical run snapshot. Tools tabs and the picker reflect the saved name.
3. Returning from context, another Investigation, a closed Tools pane, or Settings
   retains the Investigation's selected section, reading position, and open run.
   State belongs to that tool tab, not whichever chat happened to load last.
4. Saved run details lead with the Markdown answer and review/association actions.
   Technical IDs, scope snapshots, and other provenance remain available through
   disclosure. An existing answer is not represented as a new or verified answer.
5. Chat, Question previews, saved run answers, and Results use the same safe Markdown
   rendering and typographic treatment. Raw HTML and unsafe links remain blocked.
6. Tools retains multi-tab support, adds an accessible open-tabs menu, and clearly
   identifies duplicate workspace-context views as shared information. Compact
   layouts provide a clear return to the linked conversation; expansion/restore
   controls are understandable. No new layout layer is introduced.
7. Sources, Context, and Brief reduce secondary copy/empty scaffolding without
   hiding source restrictions, unresolved coverage, freshness, or review. CSV
   previews retain their 25-row limit, add useful column/text controls, and keep
   technical revision details secondary. Opening a file never changes model scope.
8. Report preview prioritizes the actual deliverable while preserving coverage
   disclosure and supporting records. Optional empty brief fields are not emitted
   as unnecessary report content. Existing Markdown export/provenance remains.
9. Settings has an explicit return to the existing conversation. Sidebar/header
   duplication is reduced while workspace identity stays clear. Preserve existing
   Fella design tokens, curvature, quiet status, and user-only Shell boundary.

## Validation plan

- Rust tests for rename persistence, validation, and unchanged analytical identity.
- Real Electron + Rust desktop tests for naming, tab/scroll continuity, nested
  mutation payloads, successful and failed association saves, safe Markdown,
  answer-first order, tab overflow, file controls, report review, and return paths.
- Retain existing acceptance assertions. Add independent follow-up cases rather
  than weakening existing expectations. Record failures and exclusions honestly.
- Type checking, production frontend build, targeted unit/bridge/report tests,
  and visual inspection at wide/compact desktop sizes in light/dark modes.
- No live model requests or original user profiles are needed. Screenshots stay
  ignored under test-results; no audit media is added to Git.

## Delivered

- Added `investigation_rename` through the Rust store, state lock, stdio command,
  and renderer bridge. It changes only title/activity timestamp, including for a
  completed Investigation; blank, multiline/control, and overlong names reject.
- New Investigations have a naming dialog; existing ones have a pencil beside
  the title. Saved names appear in the picker, Tools tabs, sidebar, and title bar.
  An explicitly renamed conversation can retain its separate custom title.
- JSON command payloads are detached from reactive proxies before Electron's
  structured-clone boundary. Failed Question-link saves retain the answer,
  selections, and retry controls; success explicitly says coverage is unchanged.
- Investigation navigation state is scoped to a stable tool key and saved locally:
  section, per-section scroll position, open run, and definition/history disclosure.
  Normal editable brief/Question/definition autosave is preserved.
- Fixed a related selection race: automatic restoration of the outgoing tab's
  chat must not compete with explicit picker navigation. The old content is not
  left interactive while the requested Investigation opens.
- Saved run details put formatted answers and Question associations first. The
  run chooser and technical details collapse when reviewing a run; provenance,
  recorded steps, evidence, and Finding creation remain available. Linking a
  supporting run is not the same as setting its Question's latest graded answer.
- Shared safe Markdown renders chat, Question previews, saved runs, Results, and
  reports. Added an open-tabs menu, shared-context clarification, visible expansion
  labels, and a compact-mode return to conversation with running-state indication.
- Sources keep scope and unavailable-source indicators visible while technical
  catalog detail is disclosed. Relative paths distinguish similar source names.
  CSV samples have text filtering, column selection (at least one stays visible),
  and an explicit expand action. The limit remains 25 rows, not full-dataset search.
- Context defaults to guide reading, with explicit editing/autosave. Empty
  Investigation-definition scaffolding and the inaccurate 'Only you can edit'
  statement are removed. Optional empty Brief/report fields are omitted.
- Report preview leads with the exported document and coverage; supporting records
  are disclosed below. Long filenames truncate instead of crushing footer status.
  Settings includes Back to conversation; duplicate chrome labels are reduced.

## Validation results and test integrity

The original nine desktop cases retain their assertions. Follow-up cases add
requirements rather than weakening their expected behavior.

1. Initial follow-up run: **16 passed, 1 failed**. The failed cross-Investigation
   section-retention check exposed a navigation issue; its evidence is retained.
2. Freezing the outgoing instance's view key and hiding its stale content did not
   fully fix the failure. The unchanged focused test still failed. Captured state
   showed that the requested second tab had not opened at all.
3. Inspection identified the competing automatic-restore effect. Suppressing that
   effect during explicit picker selection fixed the product; the same focused
   test passed. No locator, wait, assertion, expected section, or grading changed.
4. An additional guide-reading/editing journey brought the suite to **18 passed**.
   Screenshot review then found the long-filename footer defect. After fixing the
   layout and adding footer readability assertions, the final suite remained
   **18 passed (32.7 s)**. This is the final expanded suite, not an untouched baseline.

| Check | Result |
| --- | --- |
| `pnpm check` | Passed, 0 errors / 0 warnings |
| `pnpm build` | Passed; pre-existing large-chunk advisory remains |
| `cargo build --release --locked --manifest-path backend/Cargo.toml` | Passed |
| `cargo fmt --manifest-path backend/Cargo.toml --check` | Passed |
| `cargo test --manifest-path backend/Cargo.toml --lib investigation --locked` | 35 passed; includes rename, coverage, source scope, provenance, and record identity |
| `pnpm test:e2e:desktop-ui` | Final expanded suite: 18 passed |
| `pnpm test:ipc-payloads` | Passed; nested proxy arrays/objects cross a structured-clone transport |
| `pnpm test:investigation-report` | Passed |
| `pnpm test:investigation-brief-proposal` | Passed, 7 behaviors |
| `pnpm test:investigation-runs` | Passed, 4 behaviors |
| `pnpm test:workspace-tool-tabs` | 2 passed |
| `pnpm test:electron-bridge` / `pnpm test:electron-terminal` | Passed |
| All-target `cargo test … investigation --locked` attempt | Failed to compile existing integration fixtures missing `EvidenceItem.error_kind` in `workspace.rs` / `chart_tool.rs`; not presented as a passing full Rust suite |

Temporary integration-fixture compatibility edits were not retained. The focused
library check adds distinct evidence and does not erase the broader compile failure.
Legacy browser/SSR suites targeting retired navigation were not rerun or changed;
their prior failures/limitations remain recorded in WORKSPACE-UI-2026-10-16.md.
Windows/macOS native behavior, live models, correctness benchmarks, full Office
viewing, and report download/OS file dialogs were not validated in this pass.

Visual inspection additionally used a disposable copy of the populated Northstar
profile: 11 captures covering the naming dialog, Questions, answer review, compact
Tools, report, Sources, Context, CSV preview/expansion, light mode, and tab overview.
No model requests were made, copied credentials were removed afterward, and no
original user records were edited. These observations are not accuracy scores.

Local evidence lives under `test-results/performance/2026-10-16-ui-followup-implementation/`:
`initial/`, `navigation-fix/`, `navigation-ownership-fix/`, `final/`, `polished/`,
and `visual/`. Traces, screenshots, failed runs, and diagnostics remain ignored.

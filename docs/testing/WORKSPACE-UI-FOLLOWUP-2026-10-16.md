# Workspace UI follow-up — acceptance contract v1

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

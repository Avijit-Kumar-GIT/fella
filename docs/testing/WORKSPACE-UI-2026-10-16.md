# Workspace UI revision — acceptance contract

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

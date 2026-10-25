# Chat / Notebook layout contract v1.1

The first desktop run stopped in setup in both cases: it attempted to replace
an already-live conversation by writing its archive. The user approved seeding
a fresh fixture conversation instead. All layout/draft/diff/retention assertions
are unchanged. This revised setup is v1.1, not an untouched v1 baseline. Initial
failure evidence is retained at `/tmp/fella-chat-notebook-v1-initial-failures`.

Expected behavior, defined before implementation validation:

- Chat is the initial layout. Notebook is a presentation switch in the same
  window, not a mounted Workspace, a second engine, or another conversation.
- Switching retains the active folder, conversation, results and composer draft;
  it never creates an Investigation, runs analysis or calls a model.
- Notebook places the active document before chat visually. Both surfaces stay
  mounted across layout and compact-surface switches.
- Available pane width, not platform, determines split versus compact behavior.
  Compact Notebook offers document/chat selection rather than squeezed columns.
- The navigation sidebar collapses at small window sizes and can be reopened.
- A folderless Notebook offers an explicit open-folder action without silently
  granting General chat access to a folder.
- Existing Studio storage is preserved, but the primary UI calls these documents
  Reports. Saved results remain reusable without promotion into another engine.
- Notebook Investigations show existing analytical work without six required
  planning screens; canonical records and advanced inspection remain available.
- Analytical changes can be reviewed beside their result, including from chat;
  existing results are never overwritten or auto-adopted by switching views.
- Notebook naming, Files navigation and explicitly creating a report preserve
  the linked conversation and its draft; report creation does not reopen an
  unrelated existing report or perform analytical execution.

Validation is limited to the changed layout/state lifecycle, notebook projection,
analytical diff contracts and relevant renderer checks. No general app-suite run
or live model call is needed to establish deterministic layout behavior.

## Validation record

- Responsive layout browser fixture passed after fixing a real keyboard-focus
  timing issue. Original assertions were preserved; initial setup also required
  loopback-server permission in the sandbox.
- The initial desktop setup failures and approved v1.1 correction are recorded
  above. The next run exposed a product bug: a tab opened after Investigation
  records loaded stayed unbound. Tab-identity reconciliation fixed it; that
  failed run is retained at `/tmp/fella-chat-notebook-v1-hydration-failure`.
- Final real Electron/Rust run: 3 passed, 0 failed. Covered wide/600px layouts,
  retained transcript DOM and composer draft, unchanged document marker (no
  reload during layout changes), saved-result rendering, review/adoption/report
  pinning, Investigation rename, Files navigation and explicit report creation.
- The harness recorded no model, analytics or live-source execution during these
  interactions. Test fixtures use existing canonical records, not model output.
- Frontend check: zero errors/warnings. Production frontend and Rust builds pass;
  the existing large frontend chunk warning remains.

Screenshots remain under `/tmp/fella-chat-notebook-v1-results`, not in Git.
This is Linux Electron coverage, not packaged Windows/macOS coverage. The old
Studio-oriented desktop suites retain their original assertions and were not
run or rewritten for the newly approved report terminology and controls.

## UX contract revision v1.2

The approved Chat/Notebook redesign changes the previous pane-order expectation:
Chat stays on the left and remains the primary surface; the Investigation or
selected tool appears on the right. At compact widths, the pane selector is
named Chat and Tools. The Investigation companion shows its name, objective,
progress and Questions without repeating answer bodies or evidence already
shown in chat. Selecting an answered Question navigates to its matching chat
answer. Low-frequency analytical actions live in the answer actions menu.

When a Question has a saved linked analysis but no latest-run pointer, opening
it uses the newest linked run while preserving the recorded Question status.

The deterministic desktop cases now assert this revised product behavior. The
prior v1.1 setup correction and its original results above are retained as
historical evidence.

Responsive acceptance is based on available content width, not OS. At compact
widths the layout prioritizes one pane at a time with accessible Chat/Tools
selection; at wider widths it preserves readable minimum pane widths. Switching
panes must retain both mounted surfaces and their drafts, and generated content
must not cause document-level horizontal overflow. These criteria consolidate
the former standalone responsive-panel contract into this suite.

### Validation record

- `pnpm check`: passed with zero errors or warnings.
- `pnpm build`: passed. Vite still reports the existing client chunk-size
  warning for bundles over 500 kB.
- The Notebook config ran three cases successfully; the fourth first failed
  because its final selector treated the Chat tab as a button. After correcting
  that role-based locator, the focused case passed. Together, all four Notebook
  cases passed against the rebuilt app. Coverage includes the wide/right-pane
  layout, compact pane tabs, retained chat draft, answer navigation, saved
  result review, report creation, and no model or analytics execution.
- An initial run without the Notebook config used the previous static app build
  and failed on stale controls; it is retained as a setup failure, not product
  evidence.
- One related legacy workspace test was also attempted for the moved “Open in
  Investigation” action. It stopped before the action because its setup still
  expects the removed “Workspaces / Add repository” UI. The current start screen
  shows General, “Choose a folder,” and “Try sample data.” The test assertion
  was updated for the action menu, but this legacy flow remains unverified.
- These are Linux desktop results, not Windows/macOS coverage. No live model or
  analytics run was used.

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

# Tauri / Electron capability parity

Use this checklist before claiming a shell is release-ready. The UI and Rust
engine are shared, but native dialogs, event transport, process lifetime, and
packaging differ by shell. Passing Rust engine tests alone does not prove both
desktop applications work.

## Automated coverage

- `cargo test --locked` exercises the shared `EngineState` and full mock-model
  agent loop used directly by Tauri.
- `pnpm run test:electron-bridge` exercises Electron's JSON-lines client for
  concurrent request correlation, streamed events, engine errors, and shutdown
  of pending requests.
- `pnpm run check`, `pnpm run build`, and `pnpm run test:chart-renderer` cover
  the shared Svelte UI and chart renderer.

These checks do not launch either desktop shell. The Electron bridge test uses
a fake child process; it does not establish provider quality, native dialog
behavior, OS-specific packaging, or visual correctness.

## Manual desktop smoke suite

Run the cases in **each maintained shell** using the same application build
commit, provider/model, workspace fixture, and settings. Run one shell at a
time: Tauri and Electron use the same default application-data directory, so
concurrent launches can race on settings, databases, or conversation files.
Keep the model/provider and exact prompt fixed between shells. Record any
failure without changing the case or expected behavior to match the output.

| ID | Setup and action | Expected behavior | Tauri | Electron |
| --- | --- | --- | --- | --- |
| G1 | Start with no mounted folder; ask a stable general question. | The question is accepted without opening a folder. It is a direct/model-only answer, not a workspace claim. | Not run | **Pass** — local mock and direct OpenAI |
| G2 | Mount the same fixture; ask one simple question whose answer needs a document passage and one whose answer needs a table calculation. | Both requests stream; file-dependent claims have inspectable source/tool evidence. Source files remain unchanged. | Not run | **Scratch matcher failed** — direct review found the answer and provenance correct; the matcher rejected an equivalent paraphrase (details below) |
| G3 | While a longer model response is streaming, stop it. | The run stops promptly; completed evidence remains inspectable and no later response appears in the deleted/closed turn. | Not run | **Partial** — stop/no-late-text passed; preservation of completed evidence was not checked |
| G4 | Request a chart from the fixture, then expand its analysis details. | One chart renders; labels, values, units, and source query agree. Python calculations identify Python; the built-in forecast route identifies a forecast, not a user-written Python script. | Not run | **Pass** — one 24-point line chart matched the independent monthly oracle; one tick-spacing defect fixed |
| G5 | Ask an ambiguous but answerable question that should trigger one clarification; select an option and continue. | The clarification is visible and the resumed answer retains the parent-turn link and selected assumption. | Not run | **Expected clarification not triggered** — the real model stated a net-revenue interpretation and answered directly; clarification/resume UI remains untested |
| G6 | Change model and appearance, save a conversation, restart the shell, reopen it, then delete it. | Settings and transcript persist across restart. Deletion removes the transcript and its canonical analysis records and does not resurrect the row. | Not run | **Partial** — archived general chat reopened in the same process; restart/delete not tested |
| G7 | Repeat the relevant answer/detail interactions in light and dark mode. | Text, source, calculation, and forecast distinctions remain legible without relying on accent color alone. | Not run | **Partial** — basic general answer inspected in both themes; analysis details not tested |

Web research and web-page prompt-injection cases are intentionally absent: the
web tool is not shipped yet (product backlog #2 remains deferred). Do not mark
those routes as passed. When web research is implemented, add separate cases
for visible outbound requests, citation provenance, no local-file disclosure,
and hostile page instructions before enabling it by default.

## Current run record

Automated checks on 2026-10-06 (Linux/WSL; application source at
`70e0e24`, with current test/documentation changes):

- `pnpm check`, `pnpm build`, `pnpm run test:chart-renderer`,
  `pnpm run test:electron-bridge`, and `cargo fmt --all -- --check`: **pass**.
- `cargo test --locked`: **fail**. The 332 library tests passed; in
  `tests/agent_loop.rs`, 21 passed and 1 was ignored, but
  `direct_data_calls_do_not_require_a_contract` and
  `unresolved_contract_defers_direct_data_tools_until_revised` failed because
  they expected `Verified` and received `NeedsReview`. Their expectations were
  not changed. The new prompt-boundary integration test passed.
- `cargo test --locked --test conversations`: **8 passed**, including deletion
  of the transcript and linked analysis records.
- `cargo clippy --all-targets --locked -- -D warnings`: **fail**, with 23 lint
  diagnostics in the existing Rust codebase; none point to the new deletion or
  prompt-boundary code.

Electron desktop run on 2026-10-06:

- `pnpm electron:build`: **pass**. Launched the Electron 42.3.3 Linux GUI under
  WSLg with the release Rust sidecar. Both Electron's Chromium profile and
  `FELLA_DATA_DIR` were temporary directories and were removed after the run.
- G1: **pass**. With no folder mounted, a typed “What is Rust?” went through
  the actual composer and Electron IPC bridge. A localhost-only OpenAI-wire
  mock sent two SSE text fragments; the first appeared before completion, the
  final answer rendered, and exactly one model request was made. No real
  provider, credentials, or workspace files were used.
- The Windows-mounted chart-lab data directory had been missing even though
  its guide and expected-values file remained. Restoring only that directory
  from the deterministic generator made the files match the existing sidecars.
  Electron's actual preload `openWorkspace` bridge then cataloged the mounted
  `/mnt/c/Users/aviji/Documents/fella-chart-visual-lab` path as six supported
  sources (five CSV tables and `README.md`) with zero skips. This confirms
  folder-open transport and inventory, not G2's document/table question or
  G4's chart-generation behavior.
- The saved conversation was listed with `workspace: null` and reopened from
  history. The current sidebar still places this null-scoped conversation in a
  synthetic “No repository” row under Repositories; that is display grouping,
  not an assigned filesystem path, but it remains potentially confusing.
- G7 partial: changed appearance through the actual Settings view and inspected
  the general-answer transcript in light and dark modes. The chart renderer's
  automated suite checks all supported chart families and light/dark chart
  color contrast; no chart/evidence-detail interaction was manually checked in
  the desktop window.
- G6 partial: selected model and appearance persisted across a renderer reload;
  the archived conversation reopened in the same app process. Full app restart,
  post-restart settings validation, and deletion were not tested.
- Measured startup from process spawn to interactive Ask DOM: **764 ms** in this
  WSLg run. At 15 seconds after the response and history reopen, the Electron
  process tree contained 8 processes and summed to **677.6 MiB RSS**, including
  the Rust engine. This is a single Linux/WSLg sample, not a Windows figure or
  a matched Tauri comparison.
- `electron-builder --linux dir` produced an unpacked package of
  **381,927,791 bytes (about 364.1 MiB)**. `app.asar` was 30,567,439 bytes and
  the engine binary 28,300,992 bytes. This is the unpacked Linux directory, not
  a compressed installer or Windows package. The existing `dist-electron`
  directory was not overwritten.
- An exploratory run initially used Electron's default browser profile. I
  removed only the exact mock conversation it created and its tab-index entry,
  preserving all other stored tabs. The reported successful rerun used fresh
  temporary Electron and engine profiles.

Follow-up live-model Electron run on 2026-10-06:

- Used OpenAI directly with gpt-5.6-luna, the actual Electron window, Rust
  sidecar, and the synthetic /mnt/c/Users/aviji/Documents/fella-chart-visual-lab
  fixture. Prompts were submitted through the visible composer; the later
  interaction cases used an actual Enter key event. The API key was read from
  the existing local auth file, stored only in the disposable Fella data
  directory, and not printed. Only the synthetic fixture was sent to OpenAI.
- G1: **pass**. “What is Rust?” returned a direct answer with no folder
  mounted: one model call, 502 reported tokens, 4.9 seconds.
- G2 document: the answer correctly described transaction rows and negative
  refunds. Expanded details showed a grep_files result from README.md line 5
  followed by read_file of that exact file, in two tool steps and with no
  failed steps. A temporary literal-phrase assertion nevertheless reported
  false because the answer said “one sale or one refund” instead of matching
  its narrower “sale or refund” substring. This is retained as a grader
  false-negative concern, not silently counted as an automated pass or used
  to change the expected answer. The live trace reported 2 model calls,
  18,763 tokens, and 3.6 seconds for this short document question.
- G2 table: **pass**. The direct answer was $1,820,345.59 across 1,440 rows,
  exactly matching a separate Python csv/Decimal sum computed before the
  model run. Two tool steps, no failed steps, and transactions.csv provenance.
- G3: stop was available during streaming, after 88 visible characters. The
  run settled about 1.9 seconds after Stop; the UI changed to its “Stopped.”
  state and remained stable during a further 1.8-second observation. No
  post-stop text arrived. The test did not inspect retained tool evidence.
- G4: **pass**. The model produced one line chart with all 24 month points.
  Every rendered tooltip value matched an independently aggregated monthly
  Decimal oracle within one cent; the answer correctly identified August
  2025 ($87,248.30) as highest and April 2024 ($66,419.28) as lowest.
  The answer used two tool steps with no failures. Visual inspection showed
  the last x-axis labels crowded together; tick selection now uses the actual
  plotted point spacing and an SSR regression asserts non-overlapping monthly
  ticks while retaining all exact-value rows.
- G5: the live prompt “Which channel performs best in this data?” did not
  request clarification. It selected total net revenue as the meaning of
  “best,” stated that criterion, and showed revenue, transaction count, and
  units by channel. Preserve the observed result; it does not validate the
  clarification/resume UI, which remains untested.

Tauri G1–G7 remain **Not run**. Electron G6 restart/delete is still untested;
G7 analysis-detail contrast is still partial; G5 clarification/resume remains
unexercised. The live-model run improves confidence in the Electron Ask and
chart paths, but does not establish shell parity or complete backlog #9.

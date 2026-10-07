# Electron release validation

This record covers the maintained Electron shell, Svelte renderer, and Rust
sidecar. Passing engine or bridge tests alone does not prove the packaged
desktop app works. Preserve observed failures and partial runs; do not adjust
the expected behavior to match a candidate response.

## Automated coverage

- `cargo test --locked` exercises the Rust `EngineState` and mock-model agent
  loop independently of the desktop shell.
- `pnpm run test:electron-bridge` exercises Electron's JSON-lines client for
  concurrent request correlation, streamed events, engine errors, and shutdown
  of pending requests.
- `pnpm run check`, `pnpm run build`, and `pnpm run test:chart-renderer` cover
  the shared Svelte UI and chart renderer.

These checks do not launch a packaged desktop app. The Electron bridge test
uses a fake child process; it does not establish provider quality, native
dialog behavior, OS-specific packaging, or visual correctness.

The credentialed Electron clarification flow uses Playwright and the real
OpenAI provider. After `pnpm electron:build`, run
`pnpm test:e2e:clarification:live`. It reads the existing `auth.json` without
printing it, accepts `FELLA_E2E_AUTH_FILE` to select another auth file, copies
it into a private temporary profile, and removes that profile when the run
ends. The test exercises the real Electron window, engine sidecar, and model;
only the native folder-picker dialog is stubbed to mount the synthetic fixture.
It incurs normal provider usage and is intentionally not part of offline CI.

## Manual desktop smoke suite

Run each case against a packaged Electron build using the same provider/model,
workspace fixture, and settings. Record failures without changing the case or
expected behavior to match the output.

| ID | Setup and action | Expected behavior | Electron |
| --- | --- | --- | --- |
| G1 | Start with no mounted folder; ask a stable general question. | The question is accepted without opening a folder. It is a direct/model-only answer, not a workspace claim. | **Pass** — local mock and direct OpenAI |
| G2 | Mount the same fixture; ask one simple question whose answer needs a document passage and one whose answer needs a table calculation. | Both requests stream; file-dependent claims have inspectable source/tool evidence. Source files remain unchanged. | **Partial** — table calculation passed against an independent oracle; document answer/provenance looked correct, but a scratch literal-phrase matcher failed on an equivalent paraphrase |
| G3 | While a longer model response is streaming, stop it. | The run stops promptly; completed evidence remains inspectable and no later response appears in the deleted/closed turn. | **Partial** — live Electron stop/no-late-text passed; a Rust integration test verifies completed evidence survives cancellation, and an SSR regression verifies the stopped transcript exposes it. The full Electron 44 interaction remains unvalidated |
| G4 | Request a chart from the fixture, then expand its analysis details. | One chart renders; labels, values, units, and source query agree. Python calculations identify Python; the built-in forecast route identifies a forecast, not a user-written Python script. | **Pass** — one 24-point line chart matched the independent monthly oracle; one tick-spacing defect fixed |
| G5 | Ask an ambiguous but answerable question that should trigger one clarification; select an option and continue. | The clarification is visible and the resumed answer retains the parent-turn link and selected assumption. | **Fail** — the earlier UI run resumed prose through the main composer and lost the requested breakdown. A later frozen FQA episode also failed: the model assumed a category scope instead of emitting a typed clarification. The typed card markup and mock-model continuation have automated coverage, but live Electron 44 interaction remains unvalidated. |
| G6 | Change model and appearance, save a conversation, restart the app, reopen it, then delete it. | Settings and transcript persist across restart. Deletion removes the transcript and its canonical analysis records and does not resurrect the row. | **Pass for a single-turn analytical conversation** — provider/model/theme and chart transcript survived restart; reopen worked; sidebar deletion removed the transcript and canonical turn record, still absent after a second restart |
| G7 | Repeat the relevant answer/detail interactions in light and dark mode. | Text, source, calculation, and forecast distinctions remain legible without relying on accent color alone. | **Partial** — chart exact-value table and source/calculation details inspected in both themes; the plot was not fully framed in screenshots. Forecast detail markup has automated SSR coverage, but no full visual forecast-details pass has been captured in Electron 44 |

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
- An initial G6/G7 pass only tested renderer reload and same-process history;
  its limitations are superseded by the follow-up below.
- Measured startup from process spawn to interactive Ask DOM: **764 ms** in this
  WSLg run. At 15 seconds after the response and history reopen, the Electron
  process tree contained 8 processes and summed to **677.6 MiB RSS**, including
  the Rust engine. This is a single Linux/WSLg sample, not a Windows or macOS
  figure.
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
  clarification/resume path. A separate explicit clarification smoke is
  recorded below; it exercises only a prose choice followed in the main
  composer.

**Clarification continuation smoke (2026-10-06):** a separate explicit prompt
asked the live OpenAI gpt-5.6-luna model to ask which of three channel metrics
to prioritize. The model returned a natural-language question with three
numbered choices, not a structured clarification card. Automation read the
visible list items, placed the first choice (“Net revenue — total transaction
revenue, including refunds as signed negative amounts”) into the main Ask
composer, verified its value before submission, and sent it with Enter. The
choice appeared as a user message in the same conversation. The model resumed,
but answered only with the workspace-wide net transaction revenue
($1,820,345.59), not a comparison by channel; this continuation **failed** the
requested task. The amount was not independently checked. This does not pass
the structured clarification card or `clarification_continuation` trace path.

The first automation tried to parse list numbering from rendered text, but the
browser's `innerText` omits ordered-list markers. A corrected in-process runner
still stalled despite the completed response and visible options; its cause is
unresolved. A separate CDP driver attached to that same disposable Electron
window completed the prefill/Enter/resume sequence. Treat the interaction as
observed, but the standalone runner is not yet reliable. No benchmark gold or
test expectation was changed. The temporary app profile and auth copy were
removed after the run.

Cancellation evidence-retention follow-up (2026-10-07):

- A Rust agent-loop integration test now completes a read-only SQL query, waits
  until the next model request is in flight, then cancels the conversation.
  The run returns `Stopped.` while preserving SQL evidence in both the returned
  `Answer` and its `answer_done` event. This passed with the local loopback mock
  provider.
- The chart-renderer SSR suite now renders a stopped assistant message carrying
  completed evidence and checks that its Analysis Details disclosure, operation
  label, and source remain visible. This is renderer contract coverage, not a
  replacement for visually exercising the Electron 44 window.
- The same renderer suite asserts that a typed clarification displays its
  question, choices, and free-form reply control. This does not simulate
  clicking a choice or prove the Electron bridge resumes the parent turn; the
  Rust mock-model continuation test covers that backend contract, while G5's
  live model behavior remains failed.
- Forecast method, baseline-comparison, and error-band detail markup are
  asserted in the renderer suite. G7 remains partial because the visual
  forecast-details state and fully framed chart have not been inspected in
  Electron 44.
- A local Linux installer-name validation attempt did not complete: the
  sandboxed `electron-builder` run failed to resolve `github.com` while fetching
  its packaging helper, and the retried run did not finish before it was
  stopped. No installer artifacts were produced; the empty temporary output
  directory was removed. The release-name checker is covered by fixture tests
  and cross-checked against updater asset names, but the new name gate still
  needs confirmation from a successful platform packaging job.

Electron persistence and theme follow-up on 2026-10-06:

- Used the actual Electron window and Rust sidecar with a disposable app-data
  directory and browser profile. The OpenAI key was read from the local auth
  file and copied only into that disposable app-data directory. Provider and
  model were selected through the app's `/login` and `/model` command flow;
  the actual question was submitted with Enter. The run used OpenAI
  gpt-5.6-luna, not Gemma/Ollama, and no workspace files were involved.
- One preliminary runner attempt set the engine's OpenAI settings directly
  while the fresh UI conversation still had its default `gemma4:31b` model.
  It consequently sent `gemma4:31b` to OpenAI and received a 404. This was a
  test-setup/configuration failure, not a model-quality result; it is recorded
  rather than counted or hidden. The runner was corrected to select provider
  and model through the app, verify that the composer and saved settings
  agreed, wait for command completion, and start a clean conversation before
  evaluating the answer.
- The initial general-answer pass visually inspected its answer and Basis
  details in both themes. A separate chart pass used the synthetic
  `fella-chart-visual-lab` fixture and an independent `csv`/`Decimal` oracle
  calculated before the model call: 1,440 transaction rows, 24 month buckets,
  high $87,248.30 in August 2025, low $66,419.28 in April 2024. The actual
  Enter-submitted gpt-5.6-luna answer produced one line series and 24 markers;
  all 24 exact-value rows matched the oracle to the cent, and it named the
  expected high/low months. Chart context and expanded analysis details both
  identified `transactions.csv`.
- G6: **pass for this single-turn analytical conversation**. OpenAI provider,
  gpt-5.6-luna model, and dark appearance survived a full Electron process
  restart. The chart conversation and canonical analysis turn reopened. Sidebar
  deletion removed the transcript from `conversations_list` and made
  `analysis_turn_load` fail for its former turn ID; after a second full restart,
  both remained absent. Multi-turn deletion was not covered.
- G7: **partial**. The chart's exact-value table and source/calculation details
  were visually inspected in both themes. The details screenshot showed the
  source steps and checks legibly; the table screenshot showed its top rows, and
  all 24 rows were independently compared in the DOM. The chart screenshot was
  scrolled too far down to show the full plotted line. This is a test-capture
  limitation, not a chart correctness failure; the exact values, one-series
  structure, and source text checks passed. Forecast-specific detail states
  remain untested.

Electron G6 passed for the tested single-turn analytical conversation; G7
remains partial as described. G5 is **not passed**: the earlier UI run used
prose in the main composer and lost the requested comparison; a later FQA
episode showed the model did not emit a typed clarification at all. The
parent-linked continuation path passes a mock-model integration test, but its
visible card-and-selection flow has not been exercised in Electron 44. The
live-model runs improve confidence in Electron Ask and chart paths, but do not
complete backlog #9.

## Initial Electron 44.5.1 upgrade check (2026-10-06)

The maintained package is now pinned to Electron 44.5.1. `pnpm check`,
`pnpm build`, all four Electron bridge tests, all eight updater tests, all chart
renderer checks, and `cargo fmt --check` pass. `cargo test --locked` remains a
failure: 324 unit tests and 21 agent-loop integration tests pass, one test is
ignored, and the two existing agent-loop cases named above still expect
`Verified` where the runtime returns `NeedsReview`. Their expectations and
answers were not changed. `cargo clippy --all-targets --locked -- -D warnings`
also fails on 22 lint diagnostics across existing library and test code.

`pnpm electron:build` and `electron-builder --linux --x64 --dir` both pass.
The unpacked package is 318,982,396 bytes; `app.asar` is 1,784,888 bytes and
the packaged x64 Rust sidecar is 20,979,488 bytes. Its resources contain only
the architecture-specific sidecar (no stale generic duplicate). Running that
exact packaged binary with the JSON-lines `ping` request returned `pong`.
This validates packaging and the sidecar protocol, not the Electron GUI.

This Linux environment does not have Electron's `libnspr4` and `libnss3`
runtime libraries; installing system packages was denied by the managed
environment. Therefore Electron 44 could not be launched here. The earlier UI
observations above remain Electron 42.3.3 evidence—not a claim that the new
major has passed packaged GUI smoke tests. The repository's pre-existing
`dist-electron/` validation artifact was not overwritten.

## Release-gate recheck (2026-10-07)

- `pnpm check`, `pnpm build`, `test:electron-bridge`, `test:electron-update`,
  `test:chart-renderer`, the FQA adapter consistency check, and its independent
  fixture/oracle validation pass. Rust formatting and Clippy pass with
  `-D warnings`.
- The clarification parent-linked continuation integration test passes with a
  mock provider. The FQA answer key and numeric gold were not edited to fit
  model output; the generated adapter was refreshed only to carry typed-resume
  metadata. A live OpenAI gpt-5.6-luna run of that unchanged task failed twice:
  the model selected rent, utilities, and maintenance as its scope, reported
  $4,892, and emitted no typed clarification. The expected clarified total is
  $4,647. The app correctly rejected the attempted continuation because no
  pending clarification existed. A general prompt rule treating source-noted
  missing definitions as unresolved did not change the result.
- The first live attempt exposed an evaluator weakness: its intermediate
  string-only rubric could count clarification-like prose without a typed
  clarification. The runner now requires a typed request on the parent turn
  before grading the clarification step or sending the continuation. This
  changes no task text, answer key, or numeric expectation. The task's
  intermediate substring rubric remains a documented grader limitation for
  maintainer review; the case is still marked **failed**, not passed.
- `cargo test --locked`: all 324 library tests pass. In `tests/agent_loop.rs`,
  21 pass, two fail, and one is ignored. The failures are
  `direct_data_calls_do_not_require_a_contract` and
  `unresolved_contract_defers_direct_data_tools_until_revised`; both expect
  `Verified` but receive `NeedsReview`. Their expectations were not changed.
  Both mocked prompts ask to compare sales “over time,” while the mocked SQL
  returns one scalar sum across all rows. That result does not satisfy the
  requested time grain, so the current `NeedsReview` appears defensible; these
  assertions need owner review rather than being silently weakened.
  The full command stops at this failing test binary; the other integration
  binaries were run separately and passed 113 tests, with the manual 5,000-file
  performance probe ignored. The 28 `agent_eval` example tests and eval-feature
  Clippy also pass.
- `pnpm electron:build` and an Electron 44.5.1 Linux x64 unpacked package build
  pass in an isolated `/tmp` output directory. The package is 318,981,740
  bytes, includes one x64 engine sidecar, and that exact packaged sidecar
  answers the JSON-lines `ping` request with `pong`.
- The Electron 44 GUI still cannot launch in this Linux environment: `ldd`
  reports missing `libnspr4`, `libnss3`, `libnssutil3`, and `libsmime3`. No
  system packages were installed. Windows and macOS packaged GUI launches
  remain untested.

The release gate therefore remains **blocked**. The Rust suite is not green,
G5 fails on the real model, G3/G7 are partial, and no Electron 44 packaged GUI
smoke has passed on a native OS.

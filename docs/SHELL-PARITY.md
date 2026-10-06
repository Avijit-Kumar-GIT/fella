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
| G1 | Start with no mounted folder; ask a stable general question. | The question is accepted without opening a folder. It is a direct/model-only answer, not a workspace claim. | Not run | Not run |
| G2 | Mount the same fixture; ask one simple question whose answer needs a document passage and one whose answer needs a table calculation. | Both requests stream; file-dependent claims have inspectable source/tool evidence. Source files remain unchanged. | Not run | Not run |
| G3 | While a longer model response is streaming, stop it. | The run stops promptly; completed evidence remains inspectable and no later response appears in the deleted/closed turn. | Not run | Not run |
| G4 | Request a chart from the fixture, then expand its analysis details. | One chart renders; labels, values, units, and source query agree. Python calculations identify Python; the built-in forecast route identifies a forecast, not a user-written Python script. | Not run | Not run |
| G5 | Ask an ambiguous but answerable question that should trigger one clarification; select an option and continue. | The clarification is visible and the resumed answer retains the parent-turn link and selected assumption. | Not run | Not run |
| G6 | Change model and appearance, save a conversation, restart the shell, reopen it, then delete it. | Settings and transcript persist across restart. Deletion removes the transcript and its canonical analysis records and does not resurrect the row. | Not run | Not run |
| G7 | Repeat the relevant answer/detail interactions in light and dark mode. | Text, source, calculation, and forecast distinctions remain legible without relying on accent color alone. | Not run | Not run |

Web research and web-page prompt-injection cases are intentionally absent: the
web tool is not shipped yet (product backlog #2 remains deferred). Do not mark
those routes as passed. When web research is implemented, add separate cases
for visible outbound requests, citation provenance, no local-file disclosure,
and hostile page instructions before enabling it by default.

## Current run record

Code-level run on 2026-10-06 (Linux/WSL; working tree changes for backlog #9):

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

No native desktop window was launched in this WSL session, so all manual
Tauri/Electron cells remain **Not run**. Fill the table with commit, OS, shell
build, model/provider, fixture, result, and failure notes after a real desktop
pass; do not replace these cells with a generic “works” claim.

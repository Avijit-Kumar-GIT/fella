# Desktop performance audit — 16 October 2026

> Historical diagnostic for the 16 October build. It includes the removed
> terminal and earlier Tools/Workbench surfaces; do not use its timings as a
> current startup baseline. The referenced screenshots were local-only and are
> not included in this repository.
> Test source files cited in this report are no longer included in the public
> checkout.

## Protocol v1 (registered before measurement)

Test revision: `c942baa32db9a2161775daf5d01d4ba804374ba5`.
This is a local diagnostic of the current application, not a cross-platform
performance claim or a representative analytics benchmark. Application code
and existing test expectations remain unchanged during measurement.

Measure these costs separately:

1. Complete `pnpm electron:build`; frontend production builds and release Rust
   up-to-date checks with existing dependency/compiler caches; packaging to an
   unpacked desktop application. Retain wall time, resource use, exit codes,
   build output sizes, and logs. Do not label these as clean Rust compilation.
2. Three fresh-profile launches and three returning-profile launches of the
   current packaged application. Measure launch request to visible composer,
   working engine IPC, browser navigation/paint timings, and desktop process
   memory. OS disk caches are not cleared; “fresh” describes app state only.
3. Workspace preparation through the actual folder-picker interaction, then
   repeated engine reopen/index requests. Cover the small housing fixture,
   the fictional nested Northstar folder, and a generated fixture of 500 CSVs
   with 2,000 rows each plus 20 Markdown files (1,000,000 rows in total).
   Count prepared/skipped sources and rows; retain progress events.
4. UI interactions: Tools open/close, Investigation creation and sections,
   Data & context, independent shell tabs and shell readiness, workspace and
   conversation switching, and saved-history reload. Record readiness for
   the actual intended destination rather than click-handler duration alone.
5. Live model turns using existing credentials copied into disposable profiles:
   the Windows app's configured `ollama-cloud/gemma4:31b`, and the repository's
   established `openai/gpt-5.6-luna` route. Freeze the same prompts for both.
   Time submit-to-visible-user-message, first visible response text, complete
   answer, model calls, local tool execution, known input/output tokens, and
   persisted analysis. Provider/model failure is recorded and stops further
   calls to that route; it is not a numerical-answer failure.
6. Existing deterministic workspace E2E and desktop bridge/terminal/release
   checks. Run each selected suite once with its intended configuration and
   preserve all failures. Investigate failures without changing expectations.

Live tasks, defined before candidate output:

| ID | Request / prerequisite | Objective screen |
| --- | --- | --- |
| general | `What is 17 multiplied by 23? Reply with just the number.` in General | 391; no workspace evidence required |
| housing-total | `How much did rent and utilities total in Q1 2024? Exclude repairs and maintenance.` in the housing fixture | USD 4,647, supported by the fixture |
| housing-chart | `Show monthly rent plus utilities totals for January, February, and March 2024 in a chart. Exclude repairs and maintenance.` as a follow-up | A rendered, inspectable chart with January 1,545; February 1,542; March 1,560 (tolerance USD 0.01) |
| northstar | `What was total net merchandise revenue for Northstar Outfitters in Q3 and Q4 2025, and how much did it change? Use the current metric policy: merchandise by order date, posted refunds by refund date; deduplicate order and refund IDs, exclude cancelled orders and pending refunds, include posted orphan refund adjustments in the refund period, and exclude tax and shipping.` | Compare both quarters and change against an independently calculated fixture oracle, computed before the model runs; retain caveats and evidence |

The numerical screens above are narrow development checks. A successful
render or completed run does not establish analytical correctness. Record
unnecessary clarification, partial coverage, errors, and missing chart/evidence
separately. Do not revise the oracle to match a model result.

Operational timeouts: 30 seconds to launch, 120 seconds per folder preparation,
180 seconds per model turn. Treat these as measurement bounds, not acceptable
UX targets. Repeated deterministic timings report every sample, median, and
range. Single live turns are individual observations, not percentiles.
Run models and indexing sequentially so they do not compete with builds or
the full browser suite. No OS cache flush, user-history deletion, or production
profile changes are part of this audit. Screenshots and full logs stay outside
Git; the report links their local artifact directory.

## Results

The packaged application starts quickly with an empty profile, and the local
analytics engine is fast on the measured workloads. The larger delays are
workspace preparation after restarting, shell/view initialization during
navigation, model round trips, and the development launch build. The run also
found incomplete requested deliverables and one incorrect financial answer.
Those results matter more than reporting a fast answer alone.

No production application code or existing test assertions were changed.
This audit adds diagnostic scripts, this report, and an ignore rule for local
performance artifacts. The application under test remained at `c942baa`.

### Environment and interpretation

| Item | Configuration |
| --- | --- |
| OS | Linux under WSL2, kernel `6.6.87.2-microsoft-standard-WSL2`; WSLg desktop |
| CPU | AMD Ryzen 5 5600X; **4 logical CPUs exposed to WSL** |
| RAM | Approximately 11.7 GiB exposed to WSL |
| Runtime | Node 24.20.0, pnpm 11.24.0, Cargo 1.98.0, Electron 44.5.1, Vite 8.3.0 |
| Build | Release Rust sidecar; default SQLite engine with PDF/XLSX support; current production frontend |
| Desktop | Newly built, unpacked Linux package; Playwright attaches for observation |
| Data | Fictional fixtures on the Linux filesystem; an identical Northstar copy on Windows accessed through `/mnt/c` |
| Models | `ollama-cloud/gemma4:31b` (the Windows app's configured route) and `openai/gpt-5.6-luna` |

These are **not native Windows or macOS measurements**. WSL filesystem access,
WSLg graphics, machine load, compiler caches, provider/network conditions, and
Playwright attachment all affect the observations. A fresh app profile is not
a cold OS disk cache. Three startup/build samples do not support a p95 claim.
No original user app profile, conversations, or source data were edited.
Existing credentials were copied into disposable profiles for live calls and
removed from those profiles afterward; no credentials are in the report.

### Build and development costs

| Operation | Measured wall time | Meaning |
| --- | ---: | --- |
| First complete `pnpm electron:build` | **50.95 s** | Existing dependency caches; frontend build plus a 32.55 s incremental Rust compile/relink and helper preparation |
| Standalone frontend production builds | **25.38, 17.61, 17.44 s** | Median 17.61 s; no source edit between samples |
| Subsequent Rust release up-to-date checks | **0.23, 0.17, 0.21 s** | Median 0.21 s; these did not compile changed Rust code |
| Unpacked Linux packaging | **14.93 s** | Successful attempt, including Electron artifact download; not installer compression/signing |
| First packaging attempt | **Failed after 34.45 s** | Sandbox DNS failure resolving GitHub; preserved separately from the successful network-enabled retry |
| Vite server readiness | **698 ms** | Vite's own ready message, before desktop navigation |
| First Vite-backed desktop launch | **4.352 s** | Server already running; first app navigation compiles development modules |
| Later Vite-backed launches | **1.325, 1.276 s** | Same server, new profiles/browser processes |

The frontend build reported about **1.56–1.64 GiB maximum process RSS** across
the timed build invocations. This is `/usr/bin/time`'s process high-water
measurement, not the sum of every build process's simultaneous memory.

The development script currently runs a production frontend build before
starting Vite. The Electron window then loads the Vite URL, so that production
frontend build is a substantial avoidable cost on the development path.
See [package scripts](../../package.json) and
[the development launcher](../../electron/dev.mjs). The 50.95 s build is not a
measurement of the complete `electron:dev` command; do not add overlapping or
separately observed phase timings and label the sum a measured launch.

No clean Rust rebuild, initial dependency installation, Windows installer,
macOS package, signing, or update download was measured.

### Desktop startup

Readiness means Playwright can find the visible main composer and a real
renderer → Electron → Rust `ping` returns `pong`. It does not mean a model is
authenticated and has generated a response. Paint entries are recorded
separately, relative to browser navigation.

| State | Samples | Median | Range |
| --- | --- | ---: | ---: |
| Packaged, fresh profile | 802.4, 503.3, 499.7 ms | **503.3 ms** | 499.7–802.4 ms |
| Returning, selected million-row workspace, credentials removed during original cleanup | 4,705.8, 4,761.8, 4,706.1 ms | **4,706.1 ms** | 4,705.8–4,761.8 ms |
| Returning, same selected workload, credentials restored before each supplemental launch | 5,361.4, 4,875.6, 4,841.5 ms | **4,875.6 ms** | 4,841.5–5,361.4 ms |

In the original returning samples, browser content painted around 308–312 ms
after navigation, but the scoped `Ask about …` composer was not available
until roughly 4.7 seconds after the launch request. The final ping took only
about 3 ms. This separates an early painted shell from readiness for the
restored analytical workspace.

[Startup restoration](../../src/lib/commands.ts) opens pending workspace
runtimes before restoring the active scope. A new Rust process has an empty
[workspace registry](../../backend/src/stdio.rs), so a saved folder is prepared
again when reopened. The supplementary navigation run also showed a first
million-row mount taking 4.378 s, followed by 43–45 ms switches in the same
process. This is workspace preparation, not a slow trivial engine ping.

The early-startup screenshot shows contradictory state: a “Preparing data”
banner while the composer says the workspace is closed/unavailable and the
center offers “Choose a folder.” The original retained profile had no copied
credentials at that point, which also explains the connect-provider prompt.
The separate authenticated samples above retain the startup delay.

### Folder loading, queries, and caching

| Workload / action | Data | Observed time |
| --- | --- | ---: |
| Housing folder, actual picker → scoped composer | 1 CSV + 1 Markdown file; 8 rows; 703 bytes | **85.4 and 80.4 ms** |
| Northstar folder on Linux, actual picker → scoped composer | 28 files; 26 prepared sources, 19 tables, 10,750 rows; 813,549 source bytes | **239.3 and 236.5 ms** |
| Identical Northstar folder through WSL `/mnt/c` | Same 28 files and byte-for-byte directory fingerprint | **1,353.2 ms**, one sample |
| Larger generated folder, first preparation | 500 CSVs × 2,000 rows + 20 Markdown files; 25,901,990 bytes | **3,941.2 ms** |
| Larger folder, explicit full reindex | 1,000,000 rows | **3,861.4 ms** |
| Aggregate across all 500 prepared tables | Expected total 1,250,000; actual 1,250,000 | **71.8 ms** |
| Northstar repeated `openWorkspace`, already in the same Rust registry | 3 requests | **2.8–3.1 ms** |
| Million-row repeated `openWorkspace`, already in the same registry | 3 requests | **10.7–11.2 ms** |
| Northstar catalog/model IPC | 5 requests per endpoint | Approximately **2–3 ms** |
| Large catalog/model IPC | 5 requests per endpoint | Approximately **9–13 ms** |

Repeated open requests return the existing catalog; they **do not reread and
reindex the folder**. Calling their 3–11 ms result “warm ingestion” would be
misleading. The full reindex result is the appropriate comparison for preparing
data again. The million-row SQL result establishes this simple summation
workload only, not general join or statistical-analysis throughput.

The Windows-path observation measures Linux reading Windows-hosted files
through WSL. It does not predict what a native Windows Fella binary will do.
Both Northstar copies have directory SHA-256
`70c9476bbab94890ddef59921645f9384c611cc09c59454b2d90b3df96761448`.
The larger fixture fingerprint is
`f499ec6b15f461988155aa1d0b8af22aef88f52fed8d161a075db8990e15cbb4`.
Inventory hashes were collected after measurement; the generated fixture
definition, oracle, tool traces, and unchanged benchmark files are retained.

### Interaction timings and the Shell control

These include Playwright's click/actionability/readiness checks. They describe
the observed user route, not an isolated JavaScript handler's execution time.

| Action | Observation |
| --- | ---: |
| Open / close Tools | 33.5 / 40.5 ms |
| Create an Investigation and show its section navigation | 160 ms |
| Switch Investigation sections | 43.2–53.5 ms across the six sections |
| First Data & context open, including loaded guide/knowledge | 90.1 ms |
| First Shell open to terminal content | 732.4 ms; includes the installed shell's startup/profile |
| Switch between existing Data & context / Shell tabs | Medians 55.6 / 41.7 ms, five samples each |
| Open saved housing / Northstar conversations | 89.5 / 66.3 ms; existing answers reappeared |
| Switch to housing | Median 49.5 ms, five cycles |
| Switch to the already prepared million-row workspace | Median 40.6 ms, five cycles |
| Switch to Northstar with a saved Shell tab | **Median 745.4 ms**, range 731.2–869.8 ms |
| Switch to Northstar after closing that Shell tab | **Median 52.7 ms**, range 50.9–66.5 ms |

The Shell control used the disposable profile and the same app/fixtures, and
did not submit a model request. It supports shell/view lifecycle as the source
of the repeated Northstar navigation delay. It does not isolate shell spawning,
user shell-profile work, xterm setup, focus handling, and rendering from one
another. [WorkspaceView](../../src/lib/components/WorkspaceView.svelte) mounts
terminal views for the active workspace's saved tabs; changing workspace
recreates those views, and terminal `active` does not account for the Tools
pane being closed. Retaining a Shell tab can therefore add work to navigation
even while that pane is hidden.

Every raw switch sample is retained. In the control's first cycle, the
million-row backend runtime had not yet been opened and took 4,377.6 ms; its
next four switches took 42.9–45.0 ms. That first sample is not silently removed
or described as a cached switch.

### Live model runs: latency, usage, and outcome

All eight primary turns completed through the real desktop chat UI and the
configured hosted providers. Each task/provider pair was run once. These
observations are not an accuracy score or a general provider ranking.

The times below use the UI MutationObserver's completion timestamp, starting
before filling/submitting the prompt. The raw outer `elapsed_ms` also includes
polling and persistence inspection, which is why it can be slightly longer.

| Task | Gemma: UI completion | Luna: UI completion | Outcome |
| --- | ---: | ---: | --- |
| General arithmetic | **0.440 s** | **2.163 s** | Both returned 391 |
| Housing total | **2.381 s** | **5.396 s** | Both returned the correct USD 4,647 with evidence |
| Housing chart follow-up | **3.895 s** | **8.660 s** | Both returned correct table values, but no requested chart |
| Northstar quarterly revenue and change | **15.839 s** | **16.349 s** | Gemma overstated Q3; Luna got both quarters right but omitted the requested numerical change |

| Task | Gemma model calls | Gemma prompt / output tokens | Luna model calls | Luna prompt / output tokens |
| --- | ---: | ---: | ---: | ---: |
| General | 1 | 491 / 4 | 1 | 460 / 4 |
| Housing total | 2 | 21,055 / 339 | 2 | 18,776 / 277 |
| Housing chart | 4 | 45,890 / 568 | 4 | 42,491 / 522 |
| Northstar | 8 | 146,172 / 1,897 | 5 | 79,815 / 1,524 |
| Total | 15 | 213,608 / 2,808 | 12 | 141,542 / 2,327 |

Token counts come from provider-reported usage retained in model-call traces.
They are not invoices, cache-adjusted costs, or estimates of local compute.
The eight turns used 27 model calls in total.

The Northstar Gemma trace spent 15.413 s in model calls and 0.213 s in local
tool steps. Luna spent 16.083 s in model calls and 0.105 s in local tool steps.
The dominant latency on these questions is outside local SQL execution.
Even the eight-row housing sum used roughly 19–21k prompt tokens over two
calls; this is a useful concrete context-efficiency investigation target.

Submitted user messages appeared after roughly 15–24 ms. In these small
responses, the first prose observed in the assistant DOM arrived near the
end, only about 4–34 ms before the recorded completion. The engine supports
streaming, so this observation alone does not prove that streaming is broken
or identify where text was buffered. The audit did not separately instrument
provider first-token arrival, Electron event receipt, and physical display
presentation. Add those timestamps before attributing a perceived streaming
delay to one component.

#### Chart deliverable failure

The independent monthly housing values are January **1,545**, February
**1,542**, and March **1,560**. Both models calculated and reported them
correctly. In both runs, `make_chart` returned:

> these values are all within a couple percent of each other -- a chart won't
> show anything a sentence wouldn't. Answer in prose or a small table instead
> of charting flat data.

This is a deterministic rule in
[chart validation](../../backend/src/engine/analytics/chart.rs), and it
overrode an explicit request for a chart. Both final answers were internally
`verified`, despite the requested deliverable being absent. The registered
chart-task screen therefore failed for both routes. Correct figures in prose
do not convert that task into a pass. The extra attempts also increased model
round trips and prompt usage.

#### Northstar correctness and completion failure

The frozen JavaScript oracle was independently checked with Python CSV and
`Decimal` arithmetic. Both implementations agree:

| Value | Oracle | Gemma answer | Luna answer |
| --- | ---: | ---: | ---: |
| Q3 net merchandise revenue | **45,187.61** | 45,714.32 | 45,187.61 |
| Q4 net merchandise revenue | **46,139.54** | 46,139.54 | 46,139.54 |
| Q4 − Q3 | **+951.93** | No amount supplied | No amount supplied |

The fixture contains two order rows with ID `NS-202509-0003`. Gemma joined
the order extract directly to its line items without deduplicating order IDs.
That duplicated **526.71** of merchandise, exactly explaining its Q3 error.
The prompt explicitly required deduplication. Its query replay and
“every number … came from the data” checks still passed: those checks establish
repeatability/citation, not that the query implements every requested semantic
requirement. The overall answer status was `needs_review`, not `verified`.

Luna deduplicated with `ROW_NUMBER()`, obtained the correct quarter totals,
and included the orphan Q4 refund. Its final prose explicitly declined to
state the difference because it was not present in the validated query result.
The query did not include a difference column. Its overall status was also
`needs_review`. The numeric verifier's evidence-cell grounding rule is a
plausible contributing mechanism, but these traces do not preserve every
intermediate verifier draft, so this audit does not claim to have proven the
exact instruction that caused the omission. The requested analysis nevertheless
remained incomplete.

Correct general answers also retained internal `insufficient_data` status in
these records because no workspace evidence was produced. They passed the
registered General task; this is a separate status-model consistency point
to inspect before using that field as a universal completion gate.

### Resource use and process cleanup

| Snapshot | Full-tree summed RSS | Proportional set size (PSS) |
| --- | ---: | ---: |
| Fresh packaged startup, three profiles | 636–642 MiB | **342–346 MiB** |
| Workspaces/results loaded, Shell open | 836 MiB | **456 MiB** |
| Million-row workspace prepared | 778 MiB | **461 MiB** |
| Original returning samples | 661–667 MiB | **357–362 MiB** |

Summed RSS double-counts shared pages; PSS allocates shared pages across
processes. These are short snapshots, not steady-state or peak whole-session
memory measurements. Some transient shell-profile children exited between
the process enumeration and PSS read, so the Shell snapshot's PSS is partial.
Most recorded memory belonged to Electron/renderer/GPU processes; the Rust
analytics process was about 20 MiB RSS in the loaded-with-Shell snapshot.

Across the 15 launches with explicit shutdown observation (source preflight,
primary audit, authenticated startup control, and dev startup), no uncaught
renderer exception or surviving known owned process was observed after the
350 ms cleanup grace period. This includes the primary journey that opened
the shell. It is evidence for these closes, not a long-duration leak test or
proof about every possible shell child lifecycle.

Build outputs: the release sidecar is **21,929,144 bytes**; production frontend
files total **1,640,915 bytes**; the largest page JS module is **1,129,341 bytes**;
all JS totals **1,260,956 bytes**, or **355,816 bytes** when files are individually
gzipped. The unpacked Linux application occupies approximately **333.8 MB**
(decimal). The large page chunk advisory remains. It does not, by itself,
demonstrate a startup failure, but there is substantial eagerly bundled UI to
consider when isolating development and rendering cost.

### Regression checks and failures

| Check | Untouched result | Interpretation |
| --- | --- | --- |
| Frontend type-check | Passed, zero errors/warnings | Current app compiles under Svelte checking |
| Production/frontend/Rust release builds | Passed | Shipping default-feature build completes |
| Workspace browser E2E | **106 passed, 2 failed** (108 total, about 3.6 minutes) | Failure detail below |
| Full `cargo test --locked` | **Failed to compile** | `tests/workspace.rs:1228` and `:1331` omit `EvidenceItem.error_kind`; no full-suite pass claimed |
| Separate `cargo test --lib --locked` | **385 passed** with socket access | Initial sandbox run: 377 passed, 8 HTTP fixture tests blocked by `PermissionDenied`; both results retained |
| Electron bridge, terminal, updater | Passed | Existing deterministic script checks |
| Release configuration / artifact checks | Passed | Existing scripts; not proof that every OS installer was built |
| Chart renderer script | **Failed** | Raw-HTML regex expects `>Open beside</button>`; actual Svelte output contains `Open beside<!----></button>`, with the button present |
| Workbench script | **9 passed, 1 failed** | Isolated Sidebar rendering lacks the required `Tooltip.Provider` context |
| Workspace tool-tab, brief-proposal, run-link, report scripts | Passed | Existing deterministic checks |
| Selected real-Electron cancellation / clarification / layout / charts / history journeys | **1 passed, 4 failed**, about 7.5 minutes | Typed clarification passed; failures and unestablished behavior detailed below. Local deterministic HTTP providers, not additional hosted model calls |
| Current packaged-app smoke | **Passed**, 2.97 s wall time | Real UI, Rust bridge, installed-shell detection and shell command round trip |

The workspace E2E failures were not edited or rerun to obtain a pass:

1. **Tools chooser / tab selection:** the test expects one “Data & context”
   tab after adding it again. The captured DOM contains **Data & context 1**
   and **Data & context 2**, as the recently approved multiple-instance design
   allows. Its broad locator is ambiguous. This is a stale test expectation,
   preserved as a failure; the existing dedicated multiple-tab Shell test
   passed in the same full suite.
2. **Workspace dropdown keyboard focus:** after reopening the menu, pressing
   Home then ArrowDown does not leave the expected second item focused. The
   code has custom Home/End DOM focus handling alongside Bits UI's managed
   focus. The failing assertion is real; exact event-order attribution remains
   unproven without a focused keyboard event trace. Keep this as an unresolved
   keyboard interaction failure, not a passing dropdown test.

The five selected real-Electron journeys also ran once, without assertion
changes:

1. **Typed clarification passed:** the choice appeared in the composer and
   resumed the same conversation.
2. **Chart-family journey failed:** all 11 families rendered their expected
   marks and passed their recorded containment checks. Each then failed on
   the obsolete `details.values` locator: the current exact-values control is
   a `Collapsible.Root`, not a native `details`. Exact-value expansion, later
   export checks, and completion of the entire journey were therefore not
   established. This is separate from the real-model flat-data chart refusal
   reported above.
3. **Cancellation journey failed:** after waiting for the initial prose, the
   test could no longer find the Stop button. The final DOM already contained
   the fixture's delayed token, `LATE_TOKEN_SHOULD_NOT_APPEAR`. No Stop click
   occurred, so this does not demonstrate that an attempted cancellation was
   ignored. The test's slow response lasts 2.8 seconds; whether its wait misses
   that window because of final-text publication, scheduling, or another cause
   remains unresolved. Cancellation/evidence retention is not proven by this
   run. The existing composer still has an accessible Stop control while busy.
4. **Compact-layout journey failed before viewport checks:** it targets the
   removed `.workspace-preset-conversation` layout wrapper. The captured DOM
   contains the current Conversation region and scoped composer, but that
   presence alone does not prove the intended geometry at every tested size.
5. **Missing-folder history journey failed:** it expects `aria-expanded` on
   the workspace-opening button. Expansion now belongs to a separate toggle;
   the opening button uses `aria-pressed`. The screenshot/DOM does show the
   History-only status, but the remainder of retry/recovery is unestablished.

The Vite dependency scanner also intermittently reports a parse error in
`ContextView.svelte` virtual-module scanning and falls back without dependency
prebundling. Production builds and the measured desktop journeys completed.
The warning is retained; this audit does not reclassify it as an app crash or
silently ignore it in the raw logs.

### Recommended work, based on these observations

1. **Catch grain/deduplication mistakes before presenting financial results.**
   The demonstrated error is a duplicate-key join despite an explicit user
   requirement. Replay of the same SQL cannot establish that semantic
   requirement. Add meaningful duplicate/grain diagnostics and repair support,
   rather than a blanket requirement to use a particular SQL keyword.
2. **Keep explicit requested deliverables in completion checks.** The flat-data
   rule prevented requested charts, and the revenue-change request finished
   without its numerical change. Guide another valid tool step or present a
   specific unresolved deliverable; avoid a generic `verified` success signal
   for an incomplete task.
3. **Fix saved Shell tab lifecycle during workspace navigation.** The measured
   median fell from 745 to 53 ms after closing the shell in the control. Avoid
   repeatedly initializing a hidden terminal view and starting its shell when
   simply selecting the workspace. Preserve the user-only terminal boundary.
4. **Make returning readiness independent of a full folder rebuild.** Restore
   the conversation/scope promptly and prepare data with accurate progress;
   evaluate reusable catalog/data caches and incremental refresh. Do not call
   a workspace “closed” while it is actively being prepared.
5. **Separate the development launcher from production asset building.** The
   observed frontend build alone costs about 18–25 seconds, then Vite compiles
   development modules again. Keep release validation, but provide an ordinary
   development start that only prepares what its running app will consume.
6. **Inspect model context and repair round trips.** The simple eight-row sum
   used 19–21k prompt tokens; Northstar used 80–146k. Measure tool-schema,
   workspace metadata, history, and verifier context separately before trimming
   or claiming an efficiency improvement.
7. **Repair regression-suite drift and the keyboard failure.** Update stale
   fixture schemas/component wrappers/selectors through normal review, retain
   these baseline failures, and verify actual UI behavior rather than fragile
   literal server-rendered HTML.
8. **Add stage timestamps before making streaming/first-token claims.** Preserve
   provider first-token, bridge receipt, first assistant DOM text, first paint,
   completion, and per-model-call role. Existing aggregate call durations do
   not establish all of those stages.

These are findings and proposed follow-ups, not app changes made by this audit.
The baseline does not justify an across-the-board analytics-engine rewrite.

### Reproduction and retained evidence

Scripts:

- [Desktop profiler](../../scripts/profile-desktop.mjs): fresh/returning startup,
  fixtures, live turns, engine IPC, memory, and shutdown evidence.
- [Supplemental navigation profiler](../../scripts/profile-desktop-navigation.mjs):
  actual workspace/conversation/tool interactions, query/reindex, early startup,
  the Shell control, and optional Windows-path mounting through WSL.

Example Linux commands (use a new output directory for a new experiment):

```sh
pnpm electron:build
pnpm exec electron-builder --dir --publish never
node scripts/profile-desktop.mjs \
  --executable dist-electron/linux-unpacked/fella \
  --output test-results/performance/<new-run> \
  --startup-runs 3 --live --auth-file <existing-auth.json>

node scripts/profile-desktop-navigation.mjs \
  --executable dist-electron/linux-unpacked/fella \
  --profile test-results/performance/<new-run>/profiles/ollama-cloud \
  --output test-results/performance/<new-navigation-run>
```

The `--live` route uses the two pinned models above and incurs provider usage.
Use only profiles generated by these diagnostics for `--returning-profile` or
the navigation profiler: they are disposable test state, and the latter can
close saved Shell tabs when explicitly invoked with `--close-shell-first`.
The Linux full-process memory sampler reports unavailable on other platforms;
use the existing Windows process-tree probe for native Windows memory.

Local artifacts are intentionally ignored by Git:

- Primary raw records, full synthetic answers/SQL/traces, fixture oracles,
  per-file hashes, and original harness snapshot:
  `test-results/performance/2026-10-16-packaged-v1/`.
- Authenticated returning samples:
  `test-results/performance/2026-10-16-returning-authenticated/`.
- Development samples:
  `test-results/performance/2026-10-16-vite-development/`.
- Navigation samples and Shell control:
  `test-results/performance/2026-10-16-navigation-v1/` and
  `test-results/performance/2026-10-16-navigation-shell-control/`.
- Timed build/test stdout and `/usr/bin/time -v` evidence:
  `test-results/performance/2026-10-16-build-and-tests/` (copied from
  `/tmp/fella-performance-20261016-A4y4U2/`). This directory also retains the
  browser and Electron failure contexts.
- Eleven-family chart journey, including every pre-failure visual check:
  `test-results/e2e-chart-families/2026-10-16T06-26-18-163Z/journey.jsonl`.

Local-only screenshots were intentionally excluded from Git. Their capture
paths were under `test-results/performance/2026-10-16-navigation-v1/` and
`test-results/performance/2026-10-16-packaged-v1/screenshots/` on the machine
where this audit was run; they are not available in a fresh checkout.

Source/first-start timing is one diagnostic observation; live tasks are one
observation each; the larger load is one primary observation plus one reindex.
The protocol specified USD 0.01 chart tolerance, whereas the diagnostic's
numerical membership check actually uses USD 0.02. No reported outcome hinges
on that discrepancy: table values agree exactly, chart outputs are absent,
the Q3 error is USD 526.71, and both numerical changes are missing. This is a
scorer limitation retained explicitly, not a revised gold answer or a newly
claimed pass. A future version should align the implementation and protocol
before making new comparisons.
No long-running memory soak, network-failure/cancellation against these live
providers, blind quality evaluation, first-install experience, native Windows
or macOS execution, or universal performance/quality claim is established.

### Registered supplemental diagnostic

After the first runtime measurements, a separate navigation probe was added
to investigate returning startup and fill the following measurement gaps.
It submits no model turns and does not revise the original protocol or
results: five switches among the three already-open workspaces; opening the
saved housing and Northstar conversations; five Data & context / Shell tab
switches; summing the million-row fixture (expected USD 1,250,000) and a full
reindex (expected 1,000,000 rows); observing the returning UI before workspace
restore finishes; and mounting the existing fictional Windows sample through
WSL. The last measurement describes WSL filesystem access, not a native Windows
binary. Run this probe after the regression suite to avoid competing workloads.

Two follow-up controls are registered after observing the first navigation
results: close the saved Shell tab in the disposable Northstar profile and
repeat the same five workspace switches; and repeat returning-profile startup
three times with the copied credentials restored before each launch. The
original v1 returning runs had credentials removed during cleanup and remain
reported as such. Harness v1 is retained with its raw artifacts; v1.1 adds the
authenticated returning-startup option and does not change task gold or grading.

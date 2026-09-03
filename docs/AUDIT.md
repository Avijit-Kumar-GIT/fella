# Audit: Fella vs. the "microharness" philosophy

Assessed 2026-08-27, at commit `609f878`. The brief being audited against is the
general-purpose **AI agent microharness** spec (task → agent loop → artifact;
filesystem + Python tools; workspace as security boundary; Task/Work/Result UI;
permissions; harness usable by GUI/CLI/API alike).

## The headline

**Fella is a well-built local-first *analytics Q&A* tool. The spec describes a
general-purpose *task agent*.** They share about 40% of their architecture and
diverge on the other 60% by design, not by accident.

| Shared DNA | Where they part ways |
|---|---|
| Local-first, no backend | Fella is analytics-only; spec is general-purpose |
| Workspace as the unit of context | Fella: **chat REPL**; spec: **task → work → result**, explicitly *not* a chatbot |
| Deterministic Python tool | Fella's tools are **read-only**; spec wants read/write/move/copy/delete + artifacts |
| Harness separated from UI, testable headless | Fella has **no permission layer**; spec gates dangerous actions |
| Evidence trail / observability | Fella produces **answers**, not **artifacts** (files the user keeps) |
| Anti-bloat instinct, small tool set | Fella bundles **DuckDB + Arrow** right for analytics, bloat for a microharness |

## Verdict by principle

| # | Principle | Verdict | Note |
|---|---|---|---|
| 2 | UI is a thin client of the harness | ✅ aligned | `engine/` is `pub mod`, 24 tests run without Tauri, `commands.rs` is a thin adapter |
| 3 | Simple agent loop | 🟡 partial | Has max-iterations, tool-error feedback. **No cancellation. No context/token limit. Only `run_python` has a timeout.** |
| 4 | Small, explicit tool set | 🟡 partial | 7 tools, clean schemas. But: **no `write_file`/`move_file`/`copy_file`/`delete_file`/`read_file`.** Tools are analytics-shaped (`run_sql`, `search_docs`, `describe_schema`). |
| 5 | Central but modular registry; relevant tools only | 🟡 partial | Registry + `Tool` trait ✅, schemas from the impl ✅. **All 7 tools go into every context** no relevance filtering. |
| 6 | Command execution as a mediated primitive | 🟡 partial | `run_python` = `python3 -I`, temp cwd, rlimits, 20s timeout, env cleared. **cwd is a temp dir, not the workspace → can't produce artifacts the user keeps; network not restricted; no permission gate.** |
| 7 | Context contains only what's needed | 🟡 partial | No auto file injection ✅. **System prompt embeds the *entire* catalog (every table + every column) every turn.** Full history kept (bounded only by 12 steps). |
| 8 | Token efficiency | 🟡 partial | Tool results are capped (30 rows / 500 chars / 6 KB) ✅. System prompt + no history trimming + no token accounting ✗. |
| 9 | Minimal model-provider interface | 🟡 partial | Ollama + any OpenAI-compatible endpoint, in **one struct that branches on `is_openai()`** not a trait. Anthropic's native API only via an OpenAI shim. |
| 10 | Local-first by default | ✅ aligned | In-process DuckDB + SQLite, no server, no Docker; default model is localhost Ollama. |
| 11 | Hosted tier possible without core changes | ✅ aligned | `/model` supports BYO key; no accounts/billing in core. |
| 12 | Workspace as the security boundary | 🟡 partial | Enforced for the analytics tools (`run_sql` views + `grep_files`/`read_file` scoped to the folder; the DuckDB file-reading table functions `read_csv`/`read_parquet`/`read_json`/… are now on the read-only guard's banned list). **Still porous for Python** (`open('/etc/…')` works, network not blocked). No `Results/` artifact area. |
| 13 | Permissions: safe vs. dangerous, confirm when needed | ❌ **gap** | **No permission system at all.** The agent runs arbitrary Python with zero confirmation. No "the agent wants to delete 14 files → [Cancel] [Allow]". |
| 14 | Thin UI, ~3 states (Task/Work/Result) | ❌ diverges | Fella is a single scrolling **chat transcript**. Streaming tool progress and the evidence fold are *work*/*result* fragments, but the structure is chat, not task-first. |
| 15 | Not chatbot-centric | ❌ diverges | The primary (only) interaction *is* a chat box. This was a deliberate call from the earlier "fx / OpenCode feel" steer which is itself chat-shaped. |
| 16 | General-purpose, not coding- (or here, analytics-) only | ❌ diverges | Fella does data-analysis Q&A + doc search. It can't organise files, clean-and-save a CSV, extract 400 PDFs to a spreadsheet, write a report file, or render a chart image. |
| 17 | Extensions at the edges; core works with Model + Workspace + small tools | ✅ aligned | Adding a tool = one `Box::new(..)`. Browser/search would slot in behind the `Tool` trait with no loop change. |
| 18 | MCP is an optional adapter, never a core dependency | ✅ aligned | MCP *client* shipped for opt-in user connectors (2026-08-29); still not a core dependency, none bundled. |
| 19 | Errors are recoverable; enforce retry/timeout/iteration/resource limits | 🟡 partial | Tool errors fed back for model self-correction ✅; iteration limit ✅; `run_python` rlimits + timeout ✅. **No `run_sql` timeout; no retry counter separate from the step budget.** |
| 20 | Lightweight observability | 🟡 partial | Per-call `EvidenceItem` (tool, args, sql, rows, ms, error, output) ✅. **No task id, no token-usage tracking, no run metrics.** |
| 21 | Performance as a feature | 🟡 partial | Tauri + Svelte + static SPA = fast startup, small JS ✅. **Rust dep graph is heavy (duckdb + arrow + calamine + pdf-extract + reqwest/hyper/rustls + tokio + nix); 15–20 min cold builds.** |
| 22 | Anti-bloat: every dep/feature justifies itself | 🟡 partial | Tools are small and shared. **DuckDB + Arrow is ~half the build and a large slice of the binary** load-bearing for *analytics* Fella, removable for *microharness* Fella. |
| 24 | Spec's MVP checklist | 🟡 partial | Have: UI, workspace, NL input, provider, loop, registry, Python, error recovery. **Missing: file tools, permissions, artifact handling, cancellation.** |
| 26 | Harness testable without the UI, deterministic mocks | ✅ aligned | Mock model + mock embedder; agent-loop, workspace-isolation, python-tool tests. 24 green. |
| 27 | Complexity in capabilities, not the core | ✅ aligned | The core (`agent.rs` + `tools.rs` + `state.rs`) is ~900 lines and readable. |

Tally: **7 aligned, 11 partial, 4 diverge.**

## The four real divergences

1. **Chat vs. task.** (§14, §15) The spec's central abstraction is a *task* with a
   three-state flow. Fella is a REPL. Note the tension in the guidance itself:
   "extremely lightweight, like fx / OpenCode" (chat-shaped tools) vs. "do not make
   the primary interaction a chat interface." These need reconciling before the UI
   direction is settled.

2. **Read-only vs. read-write + artifacts.** (§4, §12, §24) Fella can look at data
   and answer. It cannot *change* the workspace or hand back a file. `run_python`
   runs in a temp dir that is deleted afterwards, so even files it writes are lost.
   "Clean this CSV and save it", "organise these files", "make a chart" none
   complete.

3. **No permissions.** (§13) `run_python` executes arbitrary code with no gate. This
   is currently the highest-risk capability and the least controlled. The spec wants
   a low-risk / high-risk split with UI confirmation for the dangerous side.

4. **Analytics-specialised vs. general.** (§16) DuckDB-as-the-engine is the whole
   value proposition of analytics-Fella and dead weight for a general microharness
   where Python/pandas would read CSV and Excel.

## The fork

The good bones (local-first, harness/UI split, deterministic tools, evidence,
tests, anti-bloat core) support any of these:

**A. Keep Fella as the analytics product; adopt the portable principles.**
No rewrite. Add: cancellation, a `run_sql` timeout, relevant-tool selection, trim
the system-prompt catalog to names only (fetch columns on demand), token
accounting in evidence, a permission gate on `run_python`. ~1–2 days.

**B. Pivot toward the microharness.** Large. Add generic `read_file` +
`write_file`/`move_file`/`copy_file`/`delete_file`; make `run_python` run *in* the
workspace with a persistent `Results/` area; add a permissions layer + UI
confirmation; restructure the UI to Task / Work / Result; extract `Model` as a
trait; demote DuckDB to an optional "query this data" tool (or drop it and let
pandas handle CSV/Excel). Effectively a second product on the same foundation.

**C. Hybrid.** Keep the analytics engine as *one capability* inside a
task-oriented harness that also has the general file/Python tools and the
permissions layer. Widest scope.

## Recommendation

If the goal is genuinely the microharness in this spec, **B** and the sooner the
UI and tool surface change, the less analytics-shaped code accumulates. If Fella
staying an analytics tool is acceptable, **A** captures most of the spec's value
for a fraction of the cost. **C** only if both products must ship from one binary.

The decision that unblocks everything: **is Fella "talk to your data" or "give the
computer a task"?**

## Resolution (2026-08-27)

Direction chosen: **A keep the read-only analytics core, reframed for regular
people.** Not the general task agent.

- **"Personal analytics", not analyst analytics.** The user is a non-developer
  pointing Fella at their own messy folder (statements, health exports, notes) and
  asking about their own life. Fella must cope with a real-world folder, not expect
  a clean dataset.
- **Read-only. No code-writing, no file changes, no generated artifacts.** SQL and
  Python stay as internal engines the person never sees. This makes the spec's
  write/move/delete tools, permission dialogs, and `Results/` area **out of scope**
  the read-only boundary *is* the safety story.
- **The audit's "portable principles" list was judged over-engineering.** No
  cancellation token, no `run_sql` timeout, no token-accounting UI, no per-task
  tool-relevance selection. Keep only what a small personal tool genuinely needs.
- **Keep the single-pane REPL**, but strip the terminal-roleplay: sans-serif for
  the conversation (mono only for data/SQL/tables), no `┃`/block-cursor, no
  `v0.1 · /help` dev-speak in the header, plain-language everywhere. Slash commands
  remain as a power-user escape hatch, not the advertised path.
- **Text-only informative onboarding.** No wizard. The empty state explains what
  Fella is and offers one **Choose a folder** button (+ drag-drop). Any real
  customisation comes from editing the open-source repo, not in-app settings.
- **The microharness spec's *principles* are the standing design constraints:**
  UI is a thin client, local-first, token efficiency, smallest useful tool set,
  models interchangeable, extensions at the edges, testable headless, anti-bloat.
  Its *feature list* is not a target.

Changes made under this resolution: dropped `inspect_source` (7→6 tools); the
system prompt now sends table names + shape only (columns fetched on demand);
sans/mono typography split; folder-picker onboarding; plain-language copy.

Open for later, if ever: `run_sql` statement timeout, a mid-run "stop" control.

**Update (2026-08-27):** the DuckDB question was answered `cargo bloat` put it at
likely >half the binary and ~all the cold-build time. Fella now defaults to a
**SQLite** data engine behind a `DataEngine` trait; DuckDB is `--features duckdb`.
pandas was rejected as the replacement (heavy mandatory Python runtime). See
`PERFORMANCE.md` for the before/after.

**Update (2026-08-27):** both "open for later" items landed they were cheap
enough to justify. (a) **`run_sql` timeout:** a watchdog thread calls
`interrupt()` on the read-only connection after `QUERY_TIMEOUT_SECS` (15 s
default, `FELLA_QUERY_TIMEOUT_SECS` overrides); the query returns a
"narrow it down" error and the engine stays usable. (b) **Mid-run stop:** an
`AtomicBool` on `EngineState`, flipped by a `cancel` command; `agent::run`
races each `llm.chat` against it (dropping the future closes the HTTP
connection) and checks it after every tool call, returning a `Stopped.` answer
with whatever evidence was collected. UI: the pulsing composer dot becomes a
Stop button, and `Esc` stops a run before it falls through to collapsing
evidence. Covered by `tests/sql_timeout.rs` and
`agent_loop::cancel_stops_an_in_flight_run`.

**Update (2026-08-29):** extensibility direction set. "Any real customisation
comes from editing the repo" (Resolution, above) is superseded. The base stays
a minimal single binary with nothing bundled, but users can install vetted
themes, skills, and MCP connectors themselves, distributed through a
GitHub-reviewed catalog. This ships an MCP *client* (amending principle 18) and
rescopes "nothing leaves the machine" to the base an installed connector talks
to its own service by the user's choice. Full note: `docs/EXTENSIBILITY.md`;
decision: `docs/DECISIONS.md` 2026-08-29.

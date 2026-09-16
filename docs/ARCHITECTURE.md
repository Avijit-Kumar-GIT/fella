# Fella Architecture

This document is the maintained reference for how Fella is built. Update it in the same
commit as any change that alters a design decision here.

## What Fella is

A local-first desktop app for **enterprise-grade personal analytics** a regular person points it at
their own folder of files (statements, health exports, notes, logs) and asks questions
about their own life in plain language. Not a tool for analysts; the audience is people
who don't write SQL or Python. Answers are grounded in **deterministic computation**
(SQL, or Python when SQL isn't enough) and are **fully auditable** every answer
carries the steps, queries and rows behind it.

**Read-only agent.** The agent reads the folder; it never writes, moves or deletes
anything, and it produces answers, not files. The read-only boundary is the safety
story, and it is structural there is no write tool to disable. An opt-in
`augment` pack (`EXTENSIBILITY.md`) adds a tab where *you* save a note or table you
typed; that is a user keystroke writing one named file, never the model.

The full set of positive commitments this implies is
[`PRINCIPLES.md`](PRINCIPLES.md); what Fella deliberately doesn't do is
[`NON-GOALS.md`](NON-GOALS.md). Both are referenced from [`WHY.md`](WHY.md),
the reasoning behind them.

The microharness principles in `AUDIT.md` (thin UI, local-first, token efficiency,
smallest useful tool set, interchangeable models, extensions at the edges, testable
headless, anti-bloat) are the standing design constraints. `HARNESS.md` is the
engineering log for the reasoning loop what's been measured and changed, and why
each choice holds across weak and strong models.

## Stack

| Layer | Choice | Why |
|-------|--------|-----|
| Shell | Tauri 2 | Small binary, Rust backend, system webview (no bundled Chromium) |
| UI | SvelteKit + Svelte 5 + TS, `adapter-static`, SSR off | Static SPA, no server; compiles small |
| Data engine | **SQLite** (`rusqlite`, `bundled` + `window`) behind the `DataEngine` trait | Already bundled (+0 crates); covers personal-analytics SQL. DuckDB was ~2/3 of the binary and ~all the build time (`docs/AUDIT.md` / `PERFORMANCE.md`). |
| Data engine (opt-in) | DuckDB (`--features duckdb`) | Parquet, faster on large files, `SUMMARIZE`. Adds ~30 MB. |
| App state | SQLite (`rusqlite`) | Settings, source cache, installed packs separate `fella.db` |
| MCP client (`--features mcp`, default on) | `rmcp` (client + base Streamable-HTTP transport; our own `reqwest` backend) | Connect an `mcp` connector pack to a remote MCP server; ~10 small crates, MSRV 1.88 |
| CSV/JSON import | `csv` crate + `serde_json`, own type sniffer (`data/sqlite.rs`) | DuckDB's `read_csv_auto` replacement; reuses the Excel type-inference idea |
| HTTP | `reqwest` (rustls, `ring` provider, no HTTP/2) | Talk to Ollama / OpenAI-compatible APIs; `ring` avoids the aws-lc cmake/NASM build |
| Excel (`--features xlsx`, default on) | `calamine` → typed rows → `DataEngine::add_rows` | Pure Rust; ~8 crates |
| PDF (`--features pdf`, default on) | `pdf-extract` | Pure Rust text extraction (scanned/OCR out of scope); ~28 crates |

### Cargo features

```
default = ["pdf", "xlsx", "mcp"]   # the shipped build (MSRV 1.88, for rmcp)
--no-default-features              # CSV/JSON/SQL + agent only; no PDF/Excel/MCP
--features duckdb                  # swap SQLite → DuckDB (CI-only; OOMs a laptop)
```

`mcp` pulls `rmcp` for connector packs; dropping it still installs/lists `mcp`
packs but connecting reports "no connector support".

Frontend config note: this SvelteKit version carries adapter config in
`vite.config.ts` (via the `sveltekit()` plugin options), not a separate
`svelte.config.js`.

## Process & module layout

```
src/                         SvelteKit frontend presentation only
  routes/+layout.ts          export const ssr = false; prerender = true
  routes/+layout.svelte      global CSS, key handling
  routes/+page.svelte        the single REPL view
  lib/ipc.ts                 typed wrappers over invoke() + Channel events
  lib/components/            Transcript, Message, EvidenceBlock, Composer, StatusBar

src-tauri/src/
  lib.rs                     tauri::Builder, managed state, command registration
  commands.rs                #[tauri::command] IPC surface thin adapter, no logic
  engine/
    state.rs                 EngineState { data: Mutex<Box<dyn DataEngine>>,
                                           sqlite: Mutex<Connection>, inner: Mutex<Inner>,
                                           http: reqwest::Client, secrets: Secrets,
                                           data_dir, cancel: AtomicBool }
    catalog.rs               walk workspace (depth ≤ 8), classify, slugify names, dedupe;
                             honour .fellaignore; skip a root fella.md
    analytics/                the engine: deterministic compute + verification, no LLM
                             calls, no Tauri/IPC, no conversation state. The one seam
                             back into the rest of the app is `AnalyticsSource`
                             (`catalog()` + `run_sql()`); `EngineState` implements it.
      mod.rs                 AnalyticsSource trait + module doc (the four-sentence
                             contract this module is held to)
      data/
        mod.rs                 DataEngine trait + shared read-only guard, quote_ident
        sqlite.rs              default engine: type sniff → CREATE TABLE + bulk INSERT
        duck.rs                #[cfg(feature="duckdb")] engine: read_*_auto views
      pyexec.rs               run_python's best-effort subprocess guardrails; stdlib pearsonr/linregress
                             preamble (no scipy/numpy dependency)
      chart.rs                ChartData/Series/ChartKind + validate() (flat/degenerate
                             data refused before it reaches the UI)
      verify.rs               ten deterministic post-answer checks against `&dyn
                             AnalyticsSource` (re-run cited SQL, catalog/column
                             sanity, text-aggregate and case-filter traps, a NULL
                             date GROUP BY, a value attached to the wrong column's
                             name in a multi-aggregate row) + one cost-gated
                             second-opinion re-ask
    ingest/
      docs.rs                pdf-extract / plain text → extract() (no chunking)
      excel.rs               calamine → typed rows → DataEngine::add_rows
    llm.rs                   LlmClient (one struct; branches on the provider `wire`)
    provider.rs              PROVIDERS registry (one row per provider)
    secrets.rs               Secrets → auth.json (0600); API keys + connector tokens
    sqlite.rs                fella.db: settings, sources cache, recent_workspaces,
                             extensions (installed packs)
    extensions.rs            packs: theme / skill / mcp manifest, install, enable
    mcp.rs                   #[cfg(feature="mcp")] rmcp client + our HTTP backend
     agent.rs                 the interactive harness: reasoning loop + system prompt
                              (`PromptProfile`); owns no compute of its own and calls
                              into `analytics::*`
    evidence.rs              EvidenceItem / Answer / AskEvent types
    tools.rs                 Tool trait, Registry, JSON-Schema export; the 7 built-ins
    memory.rs                per-folder learned notes (memory.md); FELLA_MEMORY
```

**Harness vs. engine, explicitly:** `agent.rs` is the interactive harness — it
owns the reasoning loop, the system prompt, and the model turns for a product
answer, and has no compute of its own. `engine/analytics/` is the engine — deterministic SQL/
stats/chart/verification logic with no knowledge that a model or a loop
exists. The engine supplies the harness, never the reverse; `AnalyticsSource`
is the one seam between them. See `docs/GOALS.md` and `docs/LIGHTWEIGHT.md`
for the philosophy and scope behind that split.

## Data layer

`engine/analytics/data/` a `DataEngine` trait with a **SQLite** impl (default) and a
**DuckDB** impl (`#[cfg(feature = "duckdb")]`). The trait is the only seam;
`analytics::verify`, `tools.rs`, `catalog.rs`, `llm.rs` are backend-agnostic. Shared
free functions live in `data/mod.rs`: the read-only guard (`ensure_read_only`),
`quote_ident`.

**Catalog scan** (`catalog.rs`): walk the chosen folder, depth ≤ 8, skip dotfiles,
honour an optional `.fellaignore`, and skip a root `fella.md` (that is user
context, not data see `EXTENSIBILITY.md`). Classify by extension. Each tabular
file becomes a table named after the slugified stem (collisions get a numeric
suffix); recorded in `fella.db` `sources`.

- CSV/TSV → `csv` crate → per-column type sniff (Int/Float/Bool/Text) → `CREATE TABLE`
  + bulk `INSERT` in a transaction. (DuckDB backend: `CREATE VIEW … read_csv_auto`.)
- JSON/NDJSON → `serde_json` → same sniff-and-insert.
- Parquet → **DuckDB backend only**; the SQLite build lists the file and returns a
  clear "rebuild with `--features duckdb`" on query.
- XLSX → `calamine` reads each sheet → inferred rows → `DataEngine::add_rows`.

`describe` (the `inspect_table` tool): SQLite composes `count(*) / count(col) /
count(DISTINCT col) / min / max` per column; DuckDB uses `SUMMARIZE`.

**Documents** (`ingest/docs.rs`): `.pdf` / `.txt` / `.md` / `.log` are catalogued
but not loaded as tables. There is no index and no embedding step: the agent
reads them directly with `grep_files` (case-insensitive regex over the extracted
text, returns file + line) and `read_file` (full text of one file, capped
~12k chars). Works identically on every model provider. (This replaced an
embed-and-cosine pipeline see `docs/DECISIONS.md`, 2026-08-29.)

**`run_python`** reaches the data through `PythonBridge`: the SQLite backend hands the
subprocess a read-only path to `analysis.db` and the `sql()` helper uses the Python
**stdlib `sqlite3`** (returns a pandas DataFrame if pandas is installed, else a list
of dicts) no `pip install` needed. The DuckDB backend hands `read_*` expressions and
needs `pip install duckdb`.

## AI layer

`LlmClient` (`llm.rs`) one struct, branching on the provider's `wire`:

- **Ollama wire** → `POST {base}/api/chat` with `tools`, `stream: true`
  (the harness forwards deltas over a Tauri `Channel`). Default `base =
  http://localhost:11434`, no key.
- **OpenAI wire** → `POST {base}/chat/completions`, streamed as SSE. The client
  reassembles content and split tool-call fragments, then hands the normalized
  reply to the same harness path.

Providers are one row each in `provider.rs` `PROVIDERS` (`id`, `display`, `auth`,
`base_url`, `wire`, …); adding an OpenAI-compatible endpoint needs no other Rust
change. Provider, base URL, key and model live in SQLite settings, edited via
`/model`; keys and connector tokens live in `auth.json` (`Secrets`), never the
DB. Transient model failures retry with backoff; a partial answer is kept. If
the provider is unreachable, `ask` returns a clear message and the status bar
shows a red dot.

## Agent loop (`agent.rs`)

```
run(question):
  msgs = [system_prompt(catalog, user_context), user: question];  evidence = []
  # no workspace and no connector → no tools offered (a plain "hello" stays one turn)
  loop up to max_steps() (MAX_STEPS = 20, FELLA_MAX_STEPS overrides):
    resp = llm.chat(msgs, tool_schemas)            # raced against a cancel flag
    if not resp.tool_calls:
     return finish(resp.content)                  # verify + AnswerDone
    for call:
      out = registry.run(call.name, args)          # built-in, then MCP; the only data access
      evidence.push({ tool, args, note, sql?, rows, result_summary, output, ms, error })
      msgs.push(assistant tool_call); msgs.push(tool result)
  # out of steps: one last turn with no tools, telling the model why, for a hedged answer
  return finish(last_turn.text or "I ran out of analysis steps …")

finish(text): verification = verify(text, evidence); emit AnswerDone
```

The evaluation-only `EngineState::ask_once` and `ask_once_usage` helpers call
the same `LlmClient` directly for judge/baseline measurements. They have no
tools, workspace context, evidence fold, or product UI path; they are not an
alternative interactive harness.

**System prompt** (`agent.rs`, sections gated by `PromptProfile` — droppable
via `FELLA_PROMPT_DROP` for eval ablation): never state a figure not returned
by a tool; prefer `run_sql`; look before you leap; for documents use
`grep_files` / `read_file`; if the data cannot answer, say so; one optional
`Background:` line of general knowledge is allowed (no figures); lead with the
headline and pick whatever shape fits. **`depth_rule`**: for a change/trend/
correlation/comparison question, check the data's shape before answering —
decompose a change, verify a correlation actually holds, state how many
points it's based on and hedge under ~8. **`aside_rule`**: `depth_rule`'s
complement, for the plain single-figure lookup it explicitly skips — one
bounded follow-up query (not a blanket cost) when the question is a segment
of a larger total, and one added short sentence only if that comparison
turns up something genuinely notable. A "Your context" block from `fella.md`
+ enabled `skill` packs is prepended; a line about `connector__tool` names is
added when an `mcp` pack is connected.

**Verification pass** (`analytics::verify`, deterministic; one bounded
corrective re-ask only when a cited SQL rerun changes or fails — `FELLA_VERIFY_REASK`): re-execute any SQL cited in
the answer and confirm the headline value is unchanged; confirm every table
named in cited SQL exists in the catalog; flag numerals in the answer that
appear in no tool result; flag a `SUM`/`AVG` over a text column, and an
exact-case filter on a column whose values differ only in capitalisation;
flag wording that implies an aggregate no cited query used, or a column
named in the question that no cited query touches; flag a question naming a
shared join column answered from one table alone; flag a date/time
`GROUP BY` that collapsed to a NULL key; flag a value in the answer sitting
next to a different column's name than the one it actually came from (a
query that packs several aggregates into one row). Rendered as a ✓/⚠
checklist in the evidence block.

## Tools

Seven built-ins (`tools.rs`), plus any namespaced `connector__tool` from an
enabled `mcp` pack (`mcp.rs`, held in a separate `Registry.mcp` list).

| Tool | Args | Returns / guardrails |
|------|------|----------------------|
| `list_files` | | workspace files: kind, row count / size, which table each maps to |
| `inspect_table` | `name`, `rows=5` | per column: type, null %, distinct, min/max; plus the first `rows` rows (0-50). Merged `describe_schema` + `sample_rows` (2026-09-08) |
| `run_sql` | `sql` | columns + rows (capped), row_count, ms. Read-only guard: single SELECT/WITH statement; rejects DDL/DML/`ATTACH`/`COPY`/`INSTALL`, `read_text`/`read_blob`/`glob`; a watchdog interrupts a runaway query (`FELLA_QUERY_TIMEOUT_SECS`, 15 s) |
| `grep_files` | `pattern`, `max_hits=30` (max 100) | matching lines (file + line) from every catalogued document, case-insensitive regex. No index |
| `read_file` | `name` or `names` | extracted text by catalogued name, capped 12k chars per document and 16k combined for a multi-file call |
| `run_python` | `code` | stdout / stderr. `python3 -I` in a fresh temp cwd, env stripped, 20 s wall-clock timeout, `RLIMIT_AS`/`RLIMIT_CPU`/`RLIMIT_FSIZE` on Unix (best-effort guard rail, not a hostile-code sandbox; filesystem reads outside the workspace and network access remain possible; temporary files are removed afterward). Preamble exposes `sql(q)` → a DataFrame (SQLite backend uses stdlib `sqlite3`, no `pip`) and stdlib `pearsonr(x, y)` / `linregress(x, y)` (no scipy/numpy) |
| `make_chart` | `kind`, `labels`, `series` | a validated bar/line chart (`analytics::chart`) — refuses flat/degenerate data server-side rather than rendering a useless chart |

Every tool call takes an optional plain-language `note` (shown in the evidence
panel). Every call and result is captured as evidence whether or not the model
cites it. An `mcp` tool the server marks non-read-only is withheld; an
un-annotated one is offered but flagged.

## IPC surface (`commands.rs` thin adapters; registered in `lib.rs`)

`open_workspace(path)` · `get_catalog()` · `describe(name)` · `run_sql_direct(sql)`
· `reindex()` · `get_settings()` / `set_settings()` · `list_providers()` /
`set_api_key(provider, key)` / `logout(provider)` · `ask(conversation_id,
question, channel)` streams `assistant_delta` / `tool_start` / `tool_end` /
`notice` / `answer_done` · `cancel()` · `ollama_health()` / `probe_ollama()` ·
`archive_conversation(id, body)` / `conversations_info()` · **packs**
`packs_list` / `packs_add` / `packs_remove` / `packs_set_enabled` /
`packs_install` / `packs_theme` · **connectors** `mcp_set_token` /
`mcp_clear_token`.

## Packs (extensions)

`engine/extensions.rs` + `engine/mcp.rs` + `engine/augment.rs`. A pack is one of
four kinds `theme` (CSS-token JSON), `skill` (Markdown into the system
prompt), `mcp` (a `connector.json` for a remote MCP server), or `augment` (an
`augment.json` that switches on a first-party capability `buffer`/`grid` and
binds it to a slash command; the tab saves a user-typed file into the open
folder). Installed under `<app-data>/extensions/<id>/`; tracked in the
`extensions` table. Browsed on an external website, installed by id
(`/packs install`), hash-checked. Connectors use `rmcp` behind the `mcp`
feature, connected lazily per `ask`, token in `auth.json`. The agent's tool set
is unchanged by any pack. Full design: `docs/EXTENSIBILITY.md`.

## UI

One window: an informative empty state, a scrolling **Transcript**, a bottom
**Composer**, a one-line **StatusBar** (workspace · model · Ollama up/down dot ·
last answer time). Plain-language and sans-serif; monospace only where data
lines up (tables, SQL). Light/dark via `prefers-color-scheme`, plus optional
`theme` packs (CSS-token overrides on `<html>`, `prefs.svelte.ts`). No routes,
no sidebars the "data workspace" is the `/files` output. Assistant answers
render as markdown (`marked`, raw HTML stripped); user/system lines stay plain
text. The evidence block is collapsed by default. `Ctrl+K` opens a command
palette; `↑` recalls input; `Esc` stops a run or collapses evidence.

## Build milestones

- [x] **0** Repo init, `.gitignore`, README + this doc.
- [x] **1** SvelteKit scaffold → adapter-static, SSR off, blank REPL + StatusBar.
- [x] **2** Data layer: DuckDB + SQLite in managed state; `open_workspace` scans a
  folder and creates views for CSV/TSV/Parquet/JSON; `/open` `/files` `/schema`
  `/sql` no AI.
- [x] **3** Excel via `calamine` → DuckDB appender (one table per sheet).
- [x] **4** LLM + agent loop: `LlmClient`, streaming `ask`, tool registry,
  evidence capture, Transcript + collapsible EvidenceBlock.
- [x] **5** Verification pass.
- [x] **6** Documents: `extract()` + `grep_files` / `read_file` (originally an
  embed pipeline, replaced 2026-08-29).
- [x] **7** Python tool.
- [x] **8** OpenAI-compatible provider + `/model` command + Ollama health dot.
  Config is command-driven; no settings modal.
- [x] **9** Polish: keybindings, `Ctrl+K` palette, light/dark, transcript in
  `localStorage`; a fresh conversation on restart, old ones archived to files.

MVP (0–9) delivered. Since then: SQLite default data engine (`DataEngine`
trait), Vercel AI Gateway, `run_sql` timeout + mid-run stop, markdown answers,
the **packs** system (`theme` / `skill` / `mcp`, see `EXTENSIBILITY.md`), and
the **analytics module** (`engine/analytics/` — SQL, stats, charts, and
verification pulled behind one `AnalyticsSource` seam, `depth_rule` /
`aside_rule`, and the value-attribution verification check). Notable choices
are logged in `docs/DECISIONS.md`; the harness's own dated engineering log is
`docs/HARNESS.md`.

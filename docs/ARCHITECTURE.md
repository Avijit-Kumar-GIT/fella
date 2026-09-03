# Fella Architecture

This document is the maintained reference for how Fella is built. Update it in the same
commit as any change that alters a design decision here.

## What Fella is

A local-first desktop app for **personal analytics** a regular person points it at
their own folder of files (statements, health exports, notes, logs) and asks questions
about their own life in plain language. Not a tool for analysts; the audience is people
who don't write SQL or Python. Answers are grounded in **deterministic computation**
(SQL, or Python when SQL isn't enough) and are **fully auditable** every answer
carries the steps, queries and rows behind it.

**Read-only.** Fella reads the folder; it never writes, moves or deletes anything, and
it produces answers, not files. The read-only boundary is the safety story.

## Non-goals ("why not X")

- **Not a task agent.** No write/move/delete tools, no generated artifacts, no
  permission dialogs see `AUDIT.md`. Fella answers questions; it doesn't do chores.
- **A fixed, small tool set in the base.** Adding a built-in tool or a file-format
  parser is a code change, not a plugin. Beyond the base, users can install vetted
  themes, skills, and MCP connectors themselves see `EXTENSIBILITY.md`.
- **MCP is opt-in, not bundled.** Fella ships an MCP client so a user can connect an
  external source (Notion, a notes repo); no connector ships by default and none is a
  core dependency. (Reversed 2026-08-29; was "No MCP".)
- **No server / cloud / Docker / Redis.** One desktop process. SQLite for app state,
  in-process DuckDB for analysis.
- **No generic agent framework.** One reasoning loop, purpose-built, ~one file.
- **No terminal roleplay.** It's a REPL, but sans-serif and plain-language; monospace
  only where data lines up.
- **LLM never touches data directly.** It can only call deterministic tools; all
  numbers in an answer must come from a tool result.
- **SQL first.** Python is the escape hatch, not the default.

The microharness principles in `AUDIT.md` (thin UI, local-first, token efficiency,
smallest useful tool set, interchangeable models, extensions at the edges, testable
headless, anti-bloat) are the standing design constraints.

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
    catalog.rs               walk workspace (depth ≤ 3), classify, slugify names, dedupe;
                             honour .fellaignore; skip a root fella.md
    data/
      mod.rs                 DataEngine trait + shared read-only guard, quote_ident
      sqlite.rs              default engine: type sniff → CREATE TABLE + bulk INSERT
      duck.rs                #[cfg(feature="duckdb")] engine: read_*_auto views
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
    agent.rs                 reasoning loop + deterministic verification pass
    evidence.rs              EvidenceItem / Answer / AskEvent types
    tools.rs                 Tool trait, Registry, JSON-Schema export; the 7 built-ins
    verify.rs                re-run cited SQL, check every figure came from a tool
```

## Data layer

`engine/data/` a `DataEngine` trait with a **SQLite** impl (default) and a
**DuckDB** impl (`#[cfg(feature = "duckdb")]`). The trait is the only seam;
`verify.rs`, `tools.rs`, `catalog.rs`, `llm.rs` are backend-agnostic. Shared free
functions live in `data/mod.rs`: the read-only guard (`ensure_read_only`),
`quote_ident`.

**Catalog scan** (`catalog.rs`): walk the chosen folder, depth ≤ 3, skip dotfiles,
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

`describe` (the `describe_schema` tool): SQLite composes `count(*) / count(col) /
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
  (tokens forwarded over a Tauri `Channel`). Default `base =
  http://localhost:11434`, no key.
- **OpenAI wire** → `POST {base}/chat/completions`, buffered (their streaming
  fragments tool calls); the assembled reply is handed to the same delta hook.

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
      return finish(resp.text)                     # verify + AnswerDone
    for call:
      out = registry.run(call.name, args)          # built-in, then MCP; the only data access
      evidence.push({ tool, args, note, sql?, rows, result_summary, output, ms, error })
      msgs.push(assistant tool_call); msgs.push(tool result)
  # out of steps: one last turn with no tools, telling the model why, for a hedged answer
  return finish(last_turn.text or "I ran out of analysis steps …")

finish(text): verification = verify(text, evidence); emit AnswerDone
```

**System prompt** (`agent.rs`): never state a figure not returned by a tool;
prefer `run_sql`; look before you leap; for documents use
`grep_files` / `read_file`; if the data cannot answer, say so; one optional
`Background:` line of general knowledge is allowed (no figures); lead with the
headline and pick whatever shape fits. A "Your context" block from `fella.md` +
enabled `skill` packs is prepended; a line about `connector__tool` names is
added when an `mcp` pack is connected.

**Verification pass** (no extra LLM call): re-execute any SQL cited in the answer and
confirm the headline value is unchanged; confirm every table named in cited SQL exists
in the catalog; flag numerals in the answer that appear in no tool result. Rendered as
a ✓/⚠ checklist in the evidence block.

## Tools

Seven built-ins (`tools.rs`), plus any namespaced `connector__tool` from an
enabled `mcp` pack (`mcp.rs`, held in a separate `Registry.mcp` list).

| Tool | Args | Returns / guardrails |
|------|------|----------------------|
| `list_files` | | workspace files: kind, row count / size, which table each maps to |
| `describe_schema` | `name` | per column: type, null %, distinct, min/max |
| `sample_rows` | `name`, `n=10` | first N rows as JSON |
| `run_sql` | `sql` | columns + rows (capped), row_count, ms. Read-only guard: single SELECT/WITH statement; rejects DDL/DML/`ATTACH`/`COPY`/`INSTALL`, `read_text`/`read_blob`/`glob`; a watchdog interrupts a runaway query (`FELLA_QUERY_TIMEOUT_SECS`, 15 s) |
| `grep_files` | `pattern`, `max_hits=30` | matching lines (file + line) from every catalogued document, case-insensitive regex. No index |
| `read_file` | `name` | full extracted text of one document, capped ~12k chars |
| `run_python` | `code` | stdout / stderr / created files. `python3 -I` in a fresh temp cwd, env stripped, no network, wall-clock timeout, `RLIMIT_AS`/`RLIMIT_CPU` (best-effort not a hostile-code sandbox; the user analyses their own data). Preamble exposes `sql(q)` → a DataFrame (SQLite backend uses stdlib `sqlite3`, no `pip`) |

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

`engine/extensions.rs` + `engine/mcp.rs`. A pack is one of three kinds
`theme` (CSS-token JSON), `skill` (Markdown into the system prompt), or `mcp`
(a `connector.json` for a remote MCP server). Installed under
`<app-data>/extensions/<id>/`; tracked in the `extensions` table. Browsed on an
external website, installed by id (`/packs install`), hash-checked. Connectors
use `rmcp` behind the `mcp` feature, connected lazily per `ask`, token in
`auth.json`. Full design: `docs/EXTENSIBILITY.md`.

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
and the **packs** system (`theme` / `skill` / `mcp`, see `EXTENSIBILITY.md`).
Notable choices are logged in `docs/DECISIONS.md`.

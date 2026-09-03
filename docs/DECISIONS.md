# Decisions

Dated one-line entries for notable choices. Newest first. Entries are kept as
written; some early ones use names that later changed reading older entries,
take `/extensions` to mean the shipped **`/packs`** command, `fella-ai` to mean
this app repo (now **`fella`**; `fella-ai` is a private pre-v0.1 archive),
`fella-marketplace` to mean the browse-site half of the **`fella-web`** repo,
and any `CODE_OF_CONDUCT.md` mention as folded into `CONTRIBUTING.md` (§Conduct).

- **2026-09-02** **Hosted pack marketplace paused until there's demand; the
  in-app feature ships as-is.** The pack system in the app is done and tested and
  stays: `/packs add` (local folder, offline), `/packs install <id>` (by id,
  every file SHA-256-checked against the catalog), `/packs enable|disable`,
  `/connect`, and the three kinds `theme` / `skill` / `mcp`. What is **deferred**
  until real demand for customisation appears (or a contributor picks it up):
  deploying the browse site (`fella-web/marketplace/`), making `fella-extensions`
  public with a populated catalog, the install-counter proxy
  (`fella-web/packs-proxy/`), and a copy-paste pack scaffold + "write your first
  pack" tutorial. Rationale: a pack is inert data (CSS-token JSON, ≤16 KB
  Markdown, or a connector URL) with no per-platform surface, and the retrieval
  path is ~50 lines behind a hash check, so nothing rots by waiting — the real
  cost is hosting ops, human review of submitted packs, and support, none of
  which pay off before there's an audience. This does **not** amend a locked
  constraint (unlike the 2026-08-29 MCP entry): the design is unchanged, only the
  rollout is held. Tracked in [`ROADMAP.md`](ROADMAP.md); becomes a GitHub issue
  when the repo is public.

- **2026-09-02** **`catalog.json` is generated; first-party pack URLs track
  `main`; install counts will come from a Fella endpoint, not the app.** In
  `fella-extensions`, `catalog.json` stopped being hand-edited — a build script
  (`scripts/build-catalog.mjs`, only `node:*` + a ~120-line vendored Markdown
  renderer) derives every `sha256` / `size_bytes`, the `published` / `updated`
  dates from git, and `readme_html` (the pack `README.md` rendered to sanitised
  HTML, so the browse site keeps its "no build, no JS deps" rule). Contributors
  commit the output; CI rebuilds with `--check` and fails if it's stale. Two
  changes of note: (1) **URL-pinning** — the stated rule was "every catalog URL
  pins a commit SHA". Packs now live in `fella-extensions/packs/<id>/`, so their
  URLs track `main` (content is still SHA-256-locked and a maintainer owns
  `main`); the commit-SHA rule still applies to any pack hosted in an outside
  repo, which can force-push. (2) **Install counts** — a local-first, no-phone-
  home app can't count its own installs. Intent: route pack downloads through a
  Fella-controlled proxy (`fella-web/packs-proxy/`, a Cloudflare Worker — code
  written, **not deployed**, no domain yet) that increments a per-id KV counter
  and injects the counts into the `catalog.json` it serves. No request body, no
  user-identifying data, one integer per pack — the "one outbound call" framing
  stays honest because the *proxy* counts, not the app. Until it exists,
  `installs` is `null` and the site shows a "New" badge. `catalog.json`'s shape
  as the app sees it is unchanged (`id` + `files[]`), so no app code changed.

- **2026-09-02** **Renamed Woody → Fella.** Project-wide rename: source, config,
  docs, the Tauri identifier (`dev.woody.app` → `dev.fella.app`), the SQLite
  settings file (`woody.db` → `fella.db`), the user-context file convention
  (`woody.md` → `fella.md`, hard switch), all `WOODY_*` env vars, the
  `localStorage` keys, and the three GitHub repos (`fella-ai` / `fella-web` /
  `fella-extensions`). A one-time migration (`migrate_from_woody` in
  `src-tauri/src/lib.rs`, plus a `woody.db` rename in `open_or_recover`) moves an
  existing `dev.woody.app` data dir on first launch, so no one loses keys or
  saved conversations. Earlier dated entries were rewritten to "Fella" in the
  same pass they describe the same decisions.

- **2026-08-31** **First-party web tools deferred to the roadmap.** Considered
  `web_search` / `web_fetch` built-in tools (off by default, key-gated) so the
  model can answer what the folder cannot. Deferred: it needs a deliberate
  reversal of "the base makes one network call, to the model the user chose" and
  grows the base's fixed tool set. Recorded in [`ROADMAP.md`](ROADMAP.md) under
  "Would need a positioning decision"; a future pickup ships with its own entry
  here, like the 2026-08-29 MCP amendment. General-knowledge answers when the
  files genuinely cannot help (marked "not from your data", no measured figures)
  are a smaller, separate change that does not need this reversal. Also started
  [`ROADMAP.md`](ROADMAP.md): a themed wish-list of improvements that each reuse
  an existing dependency or are frontend-only, so the base does not grow.

- **2026-08-29** **`mcp` connector packs: `rmcp`, Streamable HTTP only, a
  `mcp` build feature.** The `mcp` pack kind connects to a **remote** MCP
  server (a URL + a pasted token, no local subprocess stdio deferred as a
  developer workflow). Client is the official **`rmcp`** SDK with
  `features = ["client", "transport-streamable-http-client"]` the pluggable
  base transport, so Fella supplies its own `reqwest 0.13` backend
  (`engine/mcp.rs` `FellaHttp`) and does **not** pull rmcp's `reqwest 0.12` +
  quinn tail (~10 small crates instead). This is a **default cargo feature**
  (`mcp`) so the shipped app and the routine `cargo test` include it;
  `--no-default-features` drops it and connector packs then report no support.
  **MSRV 1.85 → 1.88** (rmcp). Connect is **lazy per-`ask`** no background
  tasks on `EngineState`; a connector with no token, or that won't connect, is
  skipped with a `Notice`. **Read-only policy:** `readOnlyHint: true` offered
  normally, no annotation offered but flagged "effects not declared",
  `readOnlyHint: false` / `destructiveHint: true` withheld with a notice. Tool
  names are namespaced `<id>__<tool>` (sanitised to `^[A-Za-z0-9_-]{1,64}$` for
  OpenAI-compatible endpoints). `Registry` gains a separate `mcp: Vec<McpTool>`
  (the runtime tools can't impl the `&'static str` `Tool` trait). Token stored
  in `Secrets` under `mcp:<id>:<VAR>`; entered via a dedicated **`/connect`**
  command, not `/login`.
- **2026-08-29** **Pack marketplace: browse on an external site, app installs
  by id.** The app never renders the catalog. Discovery is a website
  (`fella-marketplace`, its own repo, deployed to a Fella domain) that renders
  `catalog.json` from `fella-extensions`; `/packs browse` just opens that URL,
  `/packs install <id>` fetches the entry, SHA-256-checks each file against the
  catalog, and writes it to `<app-data>/extensions/<id>/`. Catalog URL is a
  `const` (`FELLA_CATALOG_URL` overrides). Three repos: `fella-ai` (app, depends
  only on the shape of `catalog.json`), `fella-extensions` (catalog + rules +
  example packs, where pack PRs go), `fella-marketplace` (the site, not bound by
  the app's minimalism). Two scopes only: `<app-data>/extensions/` (installed)
  and workspace `fella.md` Pi/fx's `~/.agents/skills/` + ancestor-walk +
  trust-prompt model is rejected as coding-agent machinery Fella's audience
  doesn't need.
- **2026-08-29** **Packs are exactly three kinds; broader contribution is the
  normal PR flow.** Refines the extensibility entry below. A "pack" is a
  `theme`, a `skill` (Markdown context/vocabulary into the system prompt), or an
  `mcp` connector nothing else. No "recipe" kind (a skill's Markdown can list
  suggested questions). New file formats and new built-in tools are **app code**,
  contributed to the `fella-ai` repo like any other feature, not packs.
  Distribution is one curated marketplace repo (`fella-extensions`) holding a
  `catalog.json` of hash-pinned entries plus the `fella-pack.json` schema and
  the per-kind rules; the in-app surface is `/packs`. Not a package-manager
  ecosystem. Two contribution lanes documented in a new root `CONTRIBUTING.md`
  (plus `CODE_OF_CONDUCT.md` and `.github/` templates): the app repo for code,
  the marketplace repo for packs. Full reference: `docs/EXTENSIBILITY.md`.
- **2026-08-29** **Extensibility: minimal base + vetted, user-installed packs
  and MCP connectors.** Revisits the 2026-08-27 resolution ("customisation
  comes from editing the repo") and amends the "No MCP" non-goal. Anti-bloat is
  now explicitly **about the base**: Fella still ships as one binary with
  nothing bundled, and default answer quality never depends on an extension; a
  user may extend their own install. Two layers. **Inert packs** themes
  (`:root` CSS-token JSON), context/vocabulary packs (Markdown into the system
  prompt), recipe packs (saved question sets): data the app reads, nothing
  executes. **Connectors** Fella ships an MCP client; a connector is a vetted
  MCP server (e.g. Notion, a personal notes repo) plus config, off by default,
  its tools shown in the evidence panel like the built-ins, non-read-only tools
  flagged or withheld, credentials in `auth.json`. Distribution is **GitHub,
  not a package manager**: develop it, open an issue/PR against a catalog index
  in the repo, the core team reviews the source, merge lists it; inert entries
  are hash-pinned (same approach as this repo's `skills-lock.json`). One in-app
  command (`/extensions`) discovers and installs into `<app-data>/extensions/`;
  users may also side-load, and anything outside the catalog is marked
  "unverified" with a note on what it can reach. No auto-update, no launch-time
  nagging. Base guarantees are unchanged: the workspace stays read-only and
  unconditional, and "nothing leaves the machine" now describes **the base**
  an installed connector talks to its service by the user's choice. The "no
  plugins" lines in `README.md` / `CLAUDE.md` / `docs/ARCHITECTURE.md` are
  reworded to match. Full reference: `docs/EXTENSIBILITY.md`. Nothing is built
  yet this entry settles the direction; the MCP client, the `/extensions`
  command, the catalog schema and the pack loader are follow-ups, the first
  slice being a per-workspace `fella.md` context file.
- **2026-08-29** **Agent step cap raised 12→20 and made configurable; the
  forced final turn now tells the model why.** A slower/less tool-efficient
  model (tested: `nemotron-3-super`) hit the old hard-coded 12-step cap before
  it felt confident enough to stop calling tools, surfacing the canned "ran
  out of analysis steps" line. `MAX_STEPS` in `agent.rs` is now 20 by default,
  overridable via `FELLA_MAX_STEPS` (same pattern as `FELLA_QUERY_TIMEOUT_SECS`
  / `FELLA_MODEL_RETRIES`); the loop still exits the moment the model stops
  calling tools, so this only matters for a model that needs more room, not a
  fixed floor. Separately, the forced no-tools final turn now pushes one more
  user-role message explaining that tool-calling steps ran out and asking for
  a best-effort (hedged if needed) answer, instead of just yanking the tool
  list with no explanation the canned fallback string is now a last resort
  for when the model still returns nothing.
- **2026-08-29** **Document search dropped embeddings for read-only filesystem
  tools.** `search_docs` (embed the query, cosine-similarity over a `doc_chunks`
  table) silently indexed nothing on a provider without an embeddings endpoint
  the likely default for a non-technical user and the model would still see
  documents listed, call the only tool that could read them, and get a hard
  error; a "summarize what's been happening in my life" question over a notes
  folder came back built entirely from tabular data, the notes silently
  dropped. Replaced with two tools in the existing `Tool`/`Registry` system:
  `grep_files` (regex search over every catalogued document, case-insensitive)
  and `read_file` (full text of one document, capped at ~12k chars) both
  scoped to catalogued sources only, so there's no path to escape the
  workspace. Works identically on every provider, no indexing step, no
  embeddings dependency. FTS5/BM25 was also considered and rejected: a keyword
  index can't help a query that shares no literal terms with the notes it
  needs a model that can decide to just read the files, the way a coding
  agent greps/reads its way to an answer. Removed: `reindex_docs`,
  `EngineState::search_docs`, the `doc_chunks`/`doc_cache` SQLite tables,
  `DataEngine::put_doc_chunks`/`search_docs`, and the chunking code in
  `ingest/docs.rs` (kept `extract`). Kept as dormant, reusable infra (not a
  wholesale provider-registry deprecation): `LlmClient::embed()` and the
  `embeddings`/`default_embed_model` provider-registry fields nothing
  currently reads them for a real feature; dropping `embed_model` from
  `Settings`/`/model` entirely is a reasonable follow-up if it never finds a
  use.
- **2026-08-28** **Restart starts a fresh conversation; ended conversations are
  archived to files.** The transcript is no longer restored on launch. When a
  conversation ends (app restart, `/clear`, Ctrl+L) it is written to
  `<app data dir>/conversations/conv_<ms>_<id>.json` (pretty JSON, kept forever,
  written once per id) by the new `archive_conversation` command; `/history`
  prints the folder path and count. There is no in-app history browser open the
  folder. `localStorage` (`fella:conversation`) is now only the crash-safety
  buffer for the *current* conversation and is dropped once its archive is on
  disk. Supersedes the 2026-08-27 "enough to survive a restart" note the
  never-built `conversations`/`messages` SQLite tables are still not built.
- **2026-08-28** **Dropped OpenRouter and the OAuth PKCE browser sign-in;
  Vercel AI Gateway is the hosted provider.** Reasons: OpenRouter's `:free` tier
  (≈50 req/day, shared-pool 429s) plus a whole loopback-OAuth flow (`oauth.rs`,
  `tauri-plugin-opener`, `LoginEvent`, `login`/`cancel_login` commands,
  `browserLogin`, `session.loggingIn`) ~400 lines for one provider. AI
  Gateway is a pasted key (never expires), one key → hundreds of models with
  clean `creator/model` ids, **and it has an embeddings endpoint** (so document
  search works on a hosted provider, which OpenRouter never allowed). Its
  free-tier `429`s are handled by the retry/backoff added the same day. The
  `vercel` row has no `default_model` (ids drift) `/login` then `/model`.
- **2026-08-28** **Model calls retry transient failures.** `llm::send()` retries
  429 / 502-504 / timeouts / connect errors up to `FELLA_MODEL_RETRIES` (default 3)
  with backoff, honoring `Retry-After`; a new `AskEvent::Notice` streams
  "retrying in Ns…" to the transcript; a model-call error *after* a tool step
  returns the partial answer + evidence instead of dropping the question.
- **2026-08-27** **SQLite is the default data engine; DuckDB moved behind
  `--features duckdb`.** `cargo bloat` showed DuckDB was likely >half the binary and
  ~all the cold-build time (`AUDIT.md`, `PERFORMANCE.md`). SQLite (already bundled)
  covers personal-analytics SQL. **pandas was rejected** as the replacement it would
  make the app depend on the user having Python + pandas + numpy + pyarrow + openpyxl
  installed, which the non-technical audience does not.
- **2026-08-27** `DataEngine` trait (`engine/data/`) is the single seam. SQLite impl
  imports CSV/JSON as typed tables (own sniffer + `csv` crate); `describe` composes
  aggregates instead of `SUMMARIZE`; doc search computes cosine in Rust over `BLOB`
  embeddings. Parquet is DuckDB-only.
- **2026-08-27** `run_python`'s `sql()` uses the Python **stdlib `sqlite3`** against a
  read-only `analysis.db` no `pip install` to query the data.
- **2026-08-27** HTTP: dropped reqwest's `http2` feature (removes `h2`); switched
  rustls to the `ring` provider (`rustls-no-provider` + explicit install), removing
  `aws-lc-sys` (1.3 MB + its cmake/NASM build dep). `panic = "abort"` in the default
  release profile. `pdf` / `xlsx` are default-on Cargo features (~36 crates opt-out).

- **2026-08-27** Positioning locked as **personal analytics for non-developers**,
  read-only. The general-purpose task-agent spec is a *principles* reference, not a
  feature target (`AUDIT.md`). No write tools, no permission dialogs, no artifacts.
- **2026-08-27** Tool set trimmed 7 → 6: `inspect_source` dropped (`list_files` +
  `describe_schema` already cover path / kind / columns). "Smallest useful set."
- **2026-08-27** System prompt sends table **names + row/column counts only**;
  the model calls `describe_schema` / `sample_rows` for columns. Keeps context small
  for messy folders with many files.
- **2026-08-27** UI: single-pane REPL kept, terminal-roleplay removed. Sans-serif
  (system stack, no web font) for the conversation; monospace only for tables / SQL /
  output. Folder-picker button + drag-drop as the primary way in; `/open <path>` and
  the other slash commands are a power-user shortcut.

- **2026-08-27** No settings modal. LLM configuration is the `/model` command
  (`/model <name>`, `/model provider|base_url|model|embed_model|key <value>`) —
  keeps the single-pane REPL aesthetic the product owner asked for.
- **2026-08-27** Transcript persistence is `localStorage` (per-browser, best
  effort), not the SQLite `conversations`/`messages` tables. Enough to survive a
  restart for the MVP; a real conversation store is post-MVP.
- **2026-08-27** `run_python`'s `sql()` helper rebuilds an in-process DuckDB in
  the subprocess by re-reading the workspace files. Spreadsheet-derived tables
  (materialised in the main process) are the one thing Python can't see.
- **2026-08-27** Dev profile optimises only `libduckdb-sys` / `duckdb` /
  `libsqlite3-sys` (the bundled C/C++); `opt-level = 2` for *all* deps made
  incremental builds unusably slow.

- **2026-08-27** Excel rows are bulk-loaded into a real DuckDB **table** (via the
  appender) with per-column type inference (BIGINT / DOUBLE / BOOLEAN / VARCHAR;
  dates as ISO strings), one table per sheet. A CSV/Parquet source stays a zero-copy
  view; a spreadsheet is materialised because there is no `read_xlsx` without the
  network extension.
- **2026-08-27** `duckdb`/`rusqlite` build with the `bundled` feature (compile from
  source). `pnpm tauri dev` and `cargo build`/`cargo test` can invalidate each other's
  build fingerprints, forcing a DuckDB C++ rebuild so use `cargo test` for routine
  checks and run `pnpm tauri dev` deliberately.

- **2026-08-27** MVP scope locked with product owner: ingest CSV/TSV + Parquet/JSON +
  Excel + PDF/text; SQL **and** Python compute; local-first Ollama by default with
  OpenAI-compatible endpoints as an opt-in override; UI is an extremely lightweight
  single-window REPL (OpenCode / `fx` feel), evidence inline and collapsible.
- **2026-08-27** Excel via `calamine` → Arrow → DuckDB rather than DuckDB's `excel`
  extension: the extension autoloads from the network, and Fella must work fully
  offline.
- **2026-08-27** Document similarity via DuckDB's built-in `array_cosine_similarity`
  over a plain `FLOAT[N]` column; no VSS extension. Brute force is fine at MVP corpus
  sizes and keeps the dependency surface flat.
- **2026-08-27** `run_python` is sandboxed best-effort (temp cwd, stripped env, no
  network, wall-clock timeout, Unix rlimits) but is explicitly **not** a hostile-code
  boundary: the user is analyzing their own data on their own machine.
- **2026-08-27** Verification pass is deterministic (re-run cited SQL, check table
  existence, flag stray numerals) with no extra LLM call in the MVP.
- **2026-08-27** SvelteKit scaffolded with `sv create` (Svelte 5 / Kit 2 / Vite 8);
  this version keeps adapter config in `vite.config.ts`, so there is no
  `svelte.config.js`.

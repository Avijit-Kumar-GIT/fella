# Changelog

All notable changes to Fella are recorded here. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- **Renamed Woody → Fella** throughout (app name, `dev.fella.app` identifier,
  `fella.db` / `fella.md`, `FELLA_*` env vars, repos). A first-launch migration
  carries an existing `dev.woody.app` data dir over, so keys and saved
  conversations are kept.

## [0.1.0] first public build

The initial release: a local-first desktop app that answers questions about a
folder of your own files with deterministic SQL / Python, and shows its working.

### Added

- **Ask questions in plain language** over a folder of CSV / TSV / JSON /
  NDJSON / Excel files (loaded as SQL tables) and PDF / text documents.
- **Deterministic answers.** Every figure comes from a tool result (`run_sql`,
  or `run_python` for stats SQL can't express), never from the model. A
  verification pass re-runs the cited queries and checks every number.
- **The working.** A fold under each answer shows the files read, the queries
  run, sample rows, timings, and the self-checks.
- **Documents** are read directly `grep_files` (regex search) and `read_file`
  (full text). No index, works on every model provider.
- **Models.** Local Ollama by default (nothing leaves the machine); hosted
  providers (Vercel AI Gateway, OpenAI, xAI, Ollama Cloud, OpenRouter, any
  OpenAI-compatible endpoint) via `/login` + a pasted key kept in a `0600` file.
- **Packs** opt-in extensions, none bundled: `theme` (colour schemes),
  `skill` (vocabulary/rules fed to the model), and `mcp` (connect a remote data
  source over the Model Context Protocol). Install by id with hash-checked
  downloads, or add one from a local folder (`/packs`); connect a source with
  `/connect`. A per-workspace `fella.md` adds context without a pack.
- **One-line installers** (`scripts/install.sh`, `scripts/install.ps1`) and
  per-OS builds on the releases page.
- Markdown-rendered answers; `Ctrl+K` command palette; mid-run stop;
  configurable agent step budget (`FELLA_MAX_STEPS`) and query timeout
  (`FELLA_QUERY_TIMEOUT_SECS`).

### Notes

- Builds are **unsigned** and there is no auto-updater yet re-run the install
  command or re-download to update.
- `.parquet` needs a DuckDB build (`--features duckdb`, not shipped).
- The hosted pack browser isn't live yet: `/packs add <path>` works offline, and
  `/packs install <id>` pulls from a small seed catalog.

[Unreleased]: https://github.com/Avijit-Kumar-GIT/fella/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/Avijit-Kumar-GIT/fella/releases/tag/v0.1.0

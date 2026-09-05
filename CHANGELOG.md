# Changelog

All notable changes to Fella are recorded here. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **`/update`** checks the latest GitHub release and, if it's newer,
  downloads + checksum-verifies the right installer for your OS and
  installs it (Fella closes; reopen it once the installer finishes). Manual
  only there's still no automatic or background check. See `SECURITY.md`
  and `docs/SECURITY-REVIEW-v0.1.md` for the egress entry this adds.

### Fixed

- **`/reindex` broke every already-loaded table.** A workspace that opened
  fine reported the exact same, unchanged file as unreadable the moment
  `/reindex` ran in that session a SQLite `DROP VIEW IF EXISTS` on a name
  that only ever existed as a table errored instead of no-opping. Affected
  any tabular source (CSV, TSV, JSON, NDJSON, XLSX), not just one file type.
- **`run_sql` rejected the read-only `REPLACE()` string function**, blocking
  the standard way to strip currency formatting (commas, `$`) from a text
  column before summing it a blanket ban on the word "replace" caught the
  harmless function along with the mutating `REPLACE INTO` statement it was
  meant to stop (which was already blocked another way).
- **`scripts/install.ps1`'s checksum check always failed** on Windows
  PowerShell 5.1: `SHA256SUMS` is served as `application/octet-stream`, and
  `Invoke-WebRequest`'s `.Content` for that content-type is a raw byte
  array, not text, so the checksum could never be found regardless of what
  the file actually contained.
- **`run_python` could never find Python on Windows.** The interpreter
  search hardcoded the POSIX `PATH`-list separator `:`, which breaks on
  Windows both because the real separator is `;` and because Windows paths
  themselves contain `:` (drive letters).

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

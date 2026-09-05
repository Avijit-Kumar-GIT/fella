# Changelog

All notable changes to Fella are recorded here. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.3]

### Fixed

- **Windows: `/update` closed the app but never installed anything.** The
  detached hand-off that runs the downloaded installer was built as a
  `cmd /C` string with several quoted paths in it; Rust's argument quoting
  and `cmd`'s own parsing disagree on embedded quotes, so the installer and
  relaunch commands both arrived mangled and neither ran. It also used
  `timeout` for its grace delay (which aborts immediately with no console)
  and chained the relaunch with `&` (which fires it *before* the install
  finishes). The hand-off is now a `powershell -File` script that sleeps,
  runs the installer to completion, then relaunches and it breaks away from
  any job object so exiting Fella can't take it down. The download and
  checksum half were always fine; a failed install still can't produce a
  broken one. If it still doesn't take, the already-verified installer is
  left at `%TEMP%\fella-update\` to run by hand.

## [0.1.2]

### Added

- **`/history <n>` reopens a past conversation.** `/history` now lists your
  saved conversations a preview of the first question, the message count, the
  date, and which folder it was about instead of only a count and a file
  path. `/history <n>` loads that conversation back into a new tab. If the
  reopened conversation was about a different folder than the one currently
  open, a note explains that a new question will answer from the current
  folder.
- **`parse_num()` in `/sql` and model queries.** A general helper that reads a
  number a person wrote tolerating a currency sign, thousands separators, a
  trailing `%`, and accounting-style negatives `(1,234)` and returns nothing
  for anything it can't parse, so `SUM`/`AVG` over a partly-messy text column
  skip the unreadable rows instead of a bare `CAST` silently returning a wrong
  total. When a column is imported as text because it's only mostly numeric,
  the schema note now points at `parse_num` and a way to count what didn't
  parse.

### Changed

- **Custom app icon.** The default Tauri template icon is replaced with a real
  Fella mark a soft dome character in the brand teal across every platform's
  icon set.
- **Default window is 760×720** (was 1080×760), sized to the reading column so
  the welcome screen and conversations don't sit in a narrow strip with wide
  empty margins on first launch. Still resizable, and the OS remembers a size
  you set.
- **README** brought back in line with the shipped app: five commands that
  weren't listed (`/tab`, `/focus`, `/history`, `/retry`, `/help`), the tab
  keyboard shortcuts, the per-tab model, the full set of verification checks,
  all three pack kinds, and the follow-up/session memory are now documented,
  and the app icon is shown.

### Fixed

- **Windows: the mouse cursor went invisible while navigating the `/open`
  folder picker** when `/open` was run by typing it and pressing Enter.
  Windows hides the pointer while you type and normally restores it on the
  next mouse move, but the modal picker takes the message loop before that
  happens. Fella now restores the pointer before opening the picker, matching
  what the welcome-screen "Choose a folder" button already did.

### Removed

- The unused `anyhow` dependency declaration. Also an internal de-duplication
  of the `FELLA_*` environment-knob parsing and the removal of a dead
  scaffold file no behaviour change.

## [0.1.1]

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
- **`scripts/install.ps1`'s checksum check always failed** on Windows
  PowerShell 5.1: `SHA256SUMS` is served as `application/octet-stream`, and
  `Invoke-WebRequest`'s `.Content` for that content-type is a raw byte
  array, not text, so the checksum could never be found regardless of what
  the file actually contained. (Already live before this release the
  install scripts aren't versioned artifacts.)

## [0.1.0] first public build

The initial release: a local-first desktop app that answers questions about a
folder of your own files with deterministic SQL / Python, and shows its working.

### Changed

- **Renamed Woody → Fella** throughout (app name, `dev.fella.app` identifier,
  `fella.db` / `fella.md`, `FELLA_*` env vars, repos). A first-launch migration
  carries an existing `dev.woody.app` data dir over, so keys and saved
  conversations are kept.

### Fixed

- **`run_sql` rejected the read-only `REPLACE()` string function**, blocking
  the standard way to strip currency formatting (commas, `$`) from a text
  column before summing it a blanket ban on the word "replace" caught the
  harmless function along with the mutating `REPLACE INTO` statement it was
  meant to stop (which was already blocked another way).
- **`run_python` could never find Python on Windows.** The interpreter
  search hardcoded the POSIX `PATH`-list separator `:`, which breaks on
  Windows both because the real separator is `;` and because Windows paths
  themselves contain `:` (drive letters).

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

[Unreleased]: https://github.com/Avijit-Kumar-GIT/fella/compare/v0.1.3...HEAD
[0.1.3]: https://github.com/Avijit-Kumar-GIT/fella/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/Avijit-Kumar-GIT/fella/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/Avijit-Kumar-GIT/fella/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/Avijit-Kumar-GIT/fella/releases/tag/v0.1.0

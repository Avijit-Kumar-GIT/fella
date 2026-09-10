# Changelog

All notable changes to Fella are recorded here. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **One-click return to your last folder.** The welcome screen now shows a
  "Reopen ‹folder›" button (Enter is a shortcut for it) instead of Fella
  silently opening the previous folder for you — so you can switch folders,
  read the onboarding, or start fresh without fighting past an auto-load. The
  button is absent if that folder has moved or you've never opened one.
- **Per-folder memory (experimental).** Fella now keeps a small plain-text
  `memory.md` for each folder — the *learned* companion to `fella.md`. It fills
  itself in from what already happens: a query that passed the self-check
  becomes a reusable "recipe", and a correction you give ("no, gym is under
  health") becomes a note about your words. Nothing is guessed and no extra
  model call is made. The file lives in Fella's data dir, is yours to read and
  edit, and a fresh folder with nothing learned adds nothing to the prompt.
  `FELLA_MEMORY=0` turns it off. Early: it doesn't yet show a measurable gain
  on clean test folders — the payoff is meant for messy real ones and for
  carrying your corrections across sessions.

### Changed

- **Multi-file questions get combined, not answered from one file.** A new rule
  tells the model that when a question spans two tables (or a table and a
  document) it should JOIN them in a single query, or read the document and
  reconcile its figure with the query. The schema digest now also lists any
  column name shared by two or more tables — `orders."customer_id" ↔
  customers."customer_id"` — so the model can see the join key even when the
  case differs across files.
- **One `/inspect_table` tool instead of two.** The model used to have separate
  "show me the columns" and "show me some rows" tools; they were nearly the same
  thing (the first already included a few rows) and a weaker model sometimes
  called both. They're now a single `inspect_table` — column stats plus the
  first few rows, with a `rows` count if you want more. Fewer tools, fewer
  redundant calls; measured against the eval suite to confirm answers are
  unchanged.
- **A column whose values differ only in capitalisation is now flagged.** If a
  category-style column holds `Rent`, `rent`, and `RENT` as if they were
  different, Fella notes it on the column and, if a query filters that column
  by exact case, says so — so a `Rent` row and a `rent` row don't silently fall
  on opposite sides of a filter. It doesn't rewrite your query; it points out
  where folding case (`lower(col)`, `COLLATE NOCASE`) would matter.
- **Fella won't forecast.** The "if the files can't answer, say so" rule now
  spells out that a question about the future ("next month", "will I", "how
  much will") has no answer in past records the model says so instead of
  computing an average and presenting it as a projection. (Found by the new
  agent-eval harness: one model was doing exactly that.)
- **A stale figure now gets one shot at a fix.** When the deterministic
  verification pass re-runs a query behind the answer and gets a *different
  result*, Fella spends one tool-free turn asking the model to restate its
  answer to match the re-run, then re-checks. Still no number comes from the
  model itself; the pass is still deterministic. (It does *not* re-ask over the
  fuzzier "this number isn't in a result" check that one stays a warning in
  the fold, because a tool-free reconcile there tends to mangle a correct
  answer.) Set `FELLA_VERIFY_REASK=0` to turn it off.
- **An empty total reads as "0", not a failed query.** A `SUM`/`AVG`/`MIN`/`MAX`
  over rows that matched nothing used to come back as a blank cell; some models
  read that as "the query broke" and ran two or three more to double-check the
  category exists. The result now says plainly that nothing matched and an
  empty sum or count is 0. On one measured model a "how much did I spend on X"
  where the answer is zero went from four queries to one.
- **Fewer false alarms in the self-check.** The verification fold no longer
  flags: a correct "0" answer backed by an empty aggregate; a floating-point
  total that re-serialises with a last-digit difference when re-run; a number
  that's actually part of a filename (`txns_00`). These were always cosmetic,
  but with the new corrective re-ask a false alarm now costs a model call, so
  they're fixed.

## [0.1.4]

### Added

- **Modern default model per provider.** `/login` now lands on a cheap,
  current model instead of an empty pick or a dated one: OpenAI and the
  gateways default to `gpt-5.6-luna`, xAI to `grok-4.3`, Ollama Cloud to
  `gemma4:31b`. `/model` still switches to anything the provider lists; a
  default that later 404s is fixed the same way.

### Changed

- **`/login <provider>` reuses a saved key.** If you've signed in to that
  provider before, `/login <provider>` now just switches to it the key is
  already in `auth.json`. It only asks for a key on the first sign-in, or
  when you explicitly type `/login <provider> key` to replace one. `/login`
  with no argument still lists every provider and marks which are connected.
- **`/logout <provider>` keeps the key.** It now just stops using the
  service (and drops back to local Ollama if that was the active one); the
  key stays in `auth.json` so `/login <provider>` reconnects with no
  re-paste. `/logout <provider> forget` is the new way to actually delete a
  saved key.
- **`/model` lists only text-generation models.** A provider's `/models`
  response also carries embeddings, image, audio/TTS, moderation and legacy
  base-completion ids none of which work as the answering model. Those are
  filtered from the list and its autocomplete; you can still select one by
  typing its exact id.
- **A `403` from a provider no longer reads as "bad API key".** It's almost
  always an account, plan or credit limit (Vercel AI Gateway restricts free
  credits, an OpenAI org isn't verified, a region is blocked) re-pasting the
  key won't help. The message now says so and passes through what the
  provider itself said. `401` still points you to `/login`.
- **A genuinely wrong key is now recognised whatever status code it hides
  behind.** xAI answers a bad key with `400 "Incorrect API key provided"`,
  not `401`, so `/login` used to say "couldn't reach xAI just now; it should
  work once it's reachable" the opposite of the truth. Fella now reads the
  body, so that key is reported as rejected on the spot.

### Fixed

- **OpenAI reasoning models (`o1`/`o3`/`o4`, `gpt-5` incl. `gpt-5.6`) failed
  with a 400.** They reject `max_tokens` (they want `max_completion_tokens`)
  and any `temperature` but the default; the `gpt-5` family additionally
  rejects its own default `reasoning_effort` when function tools are in play
  on `/chat/completions`. Fella now sends `max_completion_tokens`, drops
  `temperature`, and asks `gpt-5*` for `reasoning_effort: "none"` (it never
  shows a reasoning trace anyway). Every other model and provider (`gpt-4o`,
  Grok, OpenRouter, custom) is unchanged, and the fix also applies to
  gateway-namespaced ids like `openai/gpt-5.6-luna`.

## [0.1.3]

### Fixed

- **Windows: `/update` closed the app but never installed anything.** The
  detached hand-off that runs the downloaded installer was built as a
  `cmd /C` string with several quoted paths in it; Rust's argument quoting
  and `cmd`'s own parsing disagree on embedded quotes, so the installer and
  relaunch commands both arrived mangled and neither ran. It also used
  `timeout` for its grace delay (which aborts immediately with no console)
  and chained the relaunch with `&` (which fires it *before* the install
  finishes). The hand-off is now a `powershell -File` script that **waits
  for Fella to release the lock on its own `.exe`** (a silent installer
  can't overwrite a running binary and, with no window to show an error,
  just aborts — a fixed short delay wasn't enough), runs the installer via
  its process handle, then relaunches; it breaks away from any job object so
  exiting Fella can't take it down. Every step is logged to
  `%TEMP%\fella-update\update.log`. The download and checksum half were
  always fine; a failed install still can't produce a broken one, and the
  verified installer is left at `%TEMP%\fella-update\` to run by hand if the
  automatic step doesn't take.

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

[Unreleased]: https://github.com/Avijit-Kumar-GIT/fella/compare/v0.1.4...HEAD
[0.1.4]: https://github.com/Avijit-Kumar-GIT/fella/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/Avijit-Kumar-GIT/fella/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/Avijit-Kumar-GIT/fella/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/Avijit-Kumar-GIT/fella/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/Avijit-Kumar-GIT/fella/releases/tag/v0.1.0

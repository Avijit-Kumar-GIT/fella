# Fella

**Ask questions about your own files.**

Fella is a small local-first desktop app for personal analytics. Point it at a folder
of your own stuff bank statements, health exports, workout logs, notes, receipts
and ask questions in plain language:

- *"How did my spending change this year?"*
- *"What patterns are in my workouts?"*
- *"Which factors affect my coffee brewing results?"*
- *"Summarize the trends in these documents."*

Every answer is produced by **deterministic computation** SQL, or Python when SQL
can't express it never by the model guessing. And every answer shows its
**working**: open the fold under any reply for the exact files, queries and rows.

**Read-only.** Fella reads your folder; it never writes, moves or deletes anything.
Nothing leaves your computer except the request to the model you choose (a local one
by default).

## Philosophy

Small, fast, and resistant to feature bloat *in the base version*, which ships as one
binary with nothing bundled. Local-first. Minimal dependencies. It's for a regular
person doing personal analytics not analysts, not developers so it's plain-language
throughout and copes with a messy real-world folder. It is deliberately *not* a general
task agent: no file-management, no chores, and the base has a fixed, small tool set.
Customisation is opt-in and stays out of the base: vetted themes, skills, and
MCP connectors a user can install themselves (see
[`docs/EXTENSIBILITY.md`](docs/EXTENSIBILITY.md)). The whole thing stays understandable
by one person. See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).

## Requirements

- **macOS** 10.15+, **Windows** 10+, or **Linux** with WebKitGTK 4.1
  (`libwebkit2gtk-4.1`, present on current GNOME/KDE desktops).
- **A model.** A local [Ollama](https://ollama.com) works out of the box and
  keeps everything on your machine; or bring an API key for a hosted provider
  (`/login`).
- No account, no sign-up. The app is a single small binary.

## Install

**macOS / Linux**

```sh
curl -fsSL https://lilfella.app/install.sh | sh
```

**Windows** (PowerShell)

```powershell
irm https://lilfella.app/install.ps1 | iex
```

Prefer to do it by hand? Grab the build for your OS from the
[latest release](https://github.com/Avijit-Kumar-GIT/fella/releases/latest):
the **`.dmg`** (macOS drag Fella to Applications, then right-click it →
**Open** the first time), the **`-setup.exe`** (Windows *More info → Run
anyway* if SmartScreen warns), or the **`.AppImage`** / **`.deb`** (Linux). The
scripts above just do this for you.

Then give Fella a model:

- **Local, private (default):** install [Ollama](https://ollama.com) and
  `ollama pull llama3.1`. Fella uses it on `localhost:11434` automatically.
- **Hosted:** on first run type `/login`, pick a provider (Vercel AI Gateway,
  OpenAI, xAI, Ollama Cloud, OpenRouter, or any OpenAI-compatible endpoint), and
  paste an API key it's kept in a `0600` file, never the database or the
  browser. Then `/model` picks the model.

Everything including questions about your PDFs and notes works on every
provider (Fella reads documents directly, no embedding step). Builds are
**unsigned** and there is no auto-updater yet re-run the install command (or
re-download) to update, and verify the download against the `SHA256SUMS` on the
release. Signing, notarisation, an updater and Homebrew/winget are planned.

## Build from source

For contributors, or to run an unreleased revision. Needs Rust 1.88+, Node 22+
with pnpm, and on Linux the GTK/WebKit libraries in
[`docs/DEV_SETUP.md`](docs/DEV_SETUP.md).

```sh
git clone https://github.com/Avijit-Kumar-GIT/fella
cd fella && pnpm install
pnpm tauri dev          # or:  pnpm tauri build   for local installers
```

## Supported files

| Kind | Formats | How it's used |
|------|---------|---------------|
| Tabular | `.csv` `.tsv` `.json` `.ndjson` | Column types are detected; loaded as a table and queried with SQL |
| Spreadsheets | `.xlsx` | Each sheet becomes a table |
| Documents | `.pdf` `.txt` `.md` `.log` | Text extracted; the model searches it (`grep_files`) and reads whole files (`read_file`). No index, works on any provider |

`.parquet` needs the DuckDB build (`cargo build --features duckdb`); the default build
is SQLite-only to stay small.

## Using it

One window: choose a folder, then type questions. Calm and compact a dim header, the
conversation, an input box, a quiet status line with the fast, no-chrome feel of
[fx](https://fx.sh) but plain-language and non-technical, not a terminal. Monospace
shows up only where data lines up (tables, queries). Slash commands below are a
power-user shortcut; you never need them.

| Command | What it does |
|---------|--------------|
| `/open` | Choose a folder (or use the button / drag one in). `/open <path>` skips the picker |
| `/files` | List detected files and tables |
| `/schema <name>` | Show a table's columns, types and null rates |
| `/sql <query>` | Run SQL directly, bypassing the model (still recorded as evidence) |
| `/login` `/logout` `/auth` | Sign in to a hosted provider (Vercel AI Gateway, OpenAI, xAI, Ollama Cloud, OpenRouter, or a custom OpenAI-compatible endpoint); list what's signed in |
| `/model` | Show or change the LLM provider, base URL and model (for a custom endpoint) |
| `/reindex` | Check the folder again for new or changed files |
| `/packs` | Themes and skills you've added. `/packs add <path>` for a local one, `/packs install <id>` from the seed catalog ([`docs/EXTENSIBILITY.md`](docs/EXTENSIBILITY.md)) |
| `/connect` | Connect a data source you installed as an `mcp` pack (paste its token) |
| `/clear` | Start a new conversation |

**Keys:** `Enter` send · `Shift+Enter` newline · `↑` recall last input ·
`Ctrl+L` clear screen · `Ctrl+K` command palette · `Esc` stop a running
answer, otherwise collapse all evidence.

You can also click the pulsing dot next to the composer to stop a run. A stopped
run keeps whatever evidence it had gathered and answers `Stopped.`

### The working

Under each answer is a fold-away line like `▸ working · 3 steps · 412 rows · 0.7s`.
Open it to see every tool the model called, the SQL or Python it ran, a sample of the
rows that came back, timings, and the self-checks Fella ran afterwards it re-executes
the queries the answer cites and confirms every figure appears in a real result.

### Personalizing

Fella works with nothing set up. If you want more: drop a `fella.md` in your folder to
tell it how your files are organised and what your terms mean, or add a theme or skill
pack from a local folder with `/packs add <path>`. A small seed catalog is installable
by id (`/packs install <id>`); a browsable gallery of packs comes later. All optional.
See [`docs/EXTENSIBILITY.md`](docs/EXTENSIBILITY.md).

## Contributing

Fella is open source and takes contributions two ways: to the **app** (features,
fixes, new file formats, engine or UI work) here, and to **packs** (themes,
skills, MCP connectors) in the `fella-extensions` repo. Start with
[`CONTRIBUTING.md`](CONTRIBUTING.md); the pack model is in
[`docs/EXTENSIBILITY.md`](docs/EXTENSIBILITY.md).

## Status

Early, pre-1.0 tagged builds land on the
[releases page](https://github.com/Avijit-Kumar-GIT/fella/releases). See
[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for the design,
[`docs/ROADMAP.md`](docs/ROADMAP.md) for the wish-list of small, in-scope
improvements, and [`docs/DECISIONS.md`](docs/DECISIONS.md) for why things are the
way they are.

## License

[MIT](LICENSE).

<div align="center">
  <img src="logo.svg" width="88" alt="Fella logo">
  <h1>Fella</h1>
  <p><strong>Your life is already in files. Fella makes it queryable.</strong></p>
  <p>Turn a folder of statements, exports, notes, and logs into answers you can inspect all the way back to the source.</p>

  <p>
    <a href="https://lilfella.app">Website</a> ·
    <a href="https://docs.lilfella.app">Documentation</a> ·
    <a href="https://github.com/Avijit-Kumar-GIT/fella/releases/latest">Download</a>
  </p>

  <p>
    <a href="https://github.com/Avijit-Kumar-GIT/fella/actions/workflows/ci.yml">
      <img src="https://github.com/Avijit-Kumar-GIT/fella/actions/workflows/ci.yml/badge.svg" alt="CI status">
    </a>
    <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-0f766e" alt="MIT license"></a>
  </p>
</div>

> *The more an AI can do for you, the more it can do to you.*

Most of the data that matters to you is already somewhere on your computer:
bank exports, health records, receipts, spreadsheets, notes, and the half-clean
folder you never got around to organizing. Fella gives that pile a boundary and
a way to speak.

Point it at a folder, ask a question in plain language, and get a computed
answer with the path back to the evidence. Fella turns the question into
read-only analysis, produces a chart when one makes the pattern clearer, and
keeps the underlying queries, rows, and checks available for inspection.

It is the kind of queryable surface a company might build with a warehouse and
a data team, brought down to the files you already have. The model helps
translate the question; Fella's analytics engine does the work. That distinction
is the product.

It is deliberately not a general-purpose computer agent: Fella reads your data,
but it does not write, move, delete, send, or act on your behalf.

## A personal data engine with a conversational front door

Fella is not another dashboard that asks you to create a new data silo. It
starts with the folder you already own and makes it useful without asking you
to become a data analyst first.

| | |
| --- | --- |
| **Start with a folder** | Mount the files you already have and keep the workspace bounded to that source. |
| **Ask the question you actually have** | Explore spending, health, workouts, projects, documents, or any other personal dataset without learning SQL first. |
| **Make messy data useful** | Fella catalogs mixed files, infers tables, searches documents, and chooses the right read-only computation. |
| **See where the answer came from** | Answers retain the tools, queries, source rows, timings, provenance, charts, and verification checks behind them. |
| **Keep the power on your side** | The agent can analyze the workspace, but it cannot write, move, delete, send, or act. |
| **Bring your own model** | Connect OpenAI, Vercel AI Gateway, xAI, Ollama Cloud, OpenRouter, or a custom OpenAI-compatible endpoint. |

## How an answer is produced

```text
your folder, as it is
        ↓
bounded workspace + typed local tables
        ↓
question → read-only SQL · document search · bounded Python · charts
        ↓
verification + provenance checks
        ↓
an answer with a path back to the source
```

The model is the conversational front door, not the source of truth for the
numbers. The Rust analytics engine performs the computation, and a separate
verification pass re-runs cited queries and checks the answer against real
results. If the folder cannot support the claim, Fella should say so.

That makes Fella useful for questions such as:

- “Where did my spending change over the last six months?”
- “How does my sleep relate to my workout days?”
- “Which projects have the most unresolved work?”
- “Show me the trend, and let me see the records behind it.”

## Supported files

| Type | Formats | Behavior |
| --- | --- | --- |
| Tabular data | `.csv`, `.tsv`, `.json`, `.ndjson` | Detects column types and loads a queryable local table. |
| Spreadsheets | `.xlsx` | Loads each worksheet as a table. |
| Documents | `.pdf`, `.txt`, `.md`, `.log` | Searches extracted text directly; no embedding index is required. |
| Parquet | `.parquet` | Available in the optional DuckDB build; not included in the default SQLite build. |

## Install

Download the latest installer from the [releases page](https://github.com/Avijit-Kumar-GIT/fella/releases/latest),
or use the platform installer:

**macOS / Linux**

```sh
curl -fsSL https://lilfella.app/install.sh | sh
```

**Windows PowerShell**

```powershell
irm https://lilfella.app/install.ps1 | iex
```

On first launch:

1. Mount a folder.
2. Type `/login` and connect a model provider with your own API key.
3. Choose a model with `/model`.
4. Ask a question.

Fella does not require a Fella account. Provider credentials are stored locally
in `auth.json`, separate from settings and conversation history.

## Using the app

The main surfaces are intentionally small:

- **Ask** — start a conversation or open another tab.
- **Search** — search across conversations, files, repositories, and app resources.
- **Repositories** — folder mounts with their conversations and workspace view.
- **Projects** — optional, user-created knowledge pages associated with a repository.
- **Workspace** — inspect sources or edit the user-authored `fella.md` context file.
- **Settings** — provider, model, appearance, and analysis preferences.

Useful commands:

| Command | Purpose |
| --- | --- |
| `/open` | Mount or switch to a folder. |
| `/files` | List the files and tables Fella found. |
| `/schema <name>` | Inspect a table's columns, types, and null rates. |
| `/sql <query>` | Run a read-only SQL query directly. |
| `/context` | Open the workspace's `fella.md` guide. |
| `/memory` | View or clear learned notes for the current folder. |
| `/history` | Browse and reopen saved conversations. |
| `/update` | Explicitly check for and install a verified update. |

`Ctrl` becomes `Cmd` on macOS. The command palette is available with `Ctrl/Cmd+K`.

## Privacy and safety

- Fella reads the mounted workspace but never modifies its files.
- The base release has a fixed, small tool set with no write, shell, browser,
  or connector runtime.
- Provider API keys stay in local `auth.json` with restrictive permissions where
  supported; they are not stored in the settings database or browser storage.
- With a hosted provider, the question and the relevant tool results are sent
  to the provider you selected. With a local provider, they can remain on the
  machine.
- Updates happen only when explicitly requested with `/update`.

Read the full [security review](SECURITY.md) and the project's [principles](docs/PRINCIPLES.md).

## Build from source

The current release uses Tauri 2, SvelteKit, and Rust. See
[`docs/DEV_SETUP.md`](docs/DEV_SETUP.md) for platform dependencies and provider setup.

```sh
git clone https://github.com/Avijit-Kumar-GIT/fella.git
cd fella
pnpm install
pnpm tauri dev
```

Useful verification commands:

```sh
pnpm check
pnpm build
cargo test --manifest-path src-tauri/Cargo.toml --locked
```

The Electron shell comparison lives on the `electron-migration` branch and is
documented separately in [`docs/ELECTRON.md`](docs/ELECTRON.md).

## Documentation

- [Architecture](docs/ARCHITECTURE.md) — engine, harness, data flow, and boundaries
- [Why Fella](docs/WHY.md) — the product thesis
- [Principles](docs/PRINCIPLES.md) — the commitments behind the design
- [Non-goals](docs/NON-GOALS.md) — what Fella deliberately does not become
- [Developer setup](docs/DEV_SETUP.md) — dependencies, providers, tests, and evaluation
- [Roadmap](docs/ROADMAP.md) — planned in-scope work
- [Decisions](docs/DECISIONS.md) — the engineering decision log
- [Contributing](CONTRIBUTING.md) — contribution guidelines

## Status

Fella is an early, pre-1.0 release. The core personal analytics workflow is
implemented; interfaces and supported providers will continue to evolve.

## License

[MIT](LICENSE)

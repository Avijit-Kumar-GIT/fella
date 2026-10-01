<div align="center">
  <img src="docs/site/logo.svg" width="88" alt="Fella logo">
  <h1>Fella</h1>
  <p><strong>An opinionated analytics engine and harness for personal data.</strong></p>
  <p>Turn messy files into consistent, correctness-first analysis&mdash;with no tool sprawl and no write access.</p>

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

General-purpose agents are built to do more. Fella is built to get one class of
work right: analytics over the data you already have.

Your computer is full of data that never became insight: a year of expenses, a
workout log, project exports, PDFs, notes, and files with names you no longer
remember. Fella gives those fragments a bounded workspace you can actually
explore. Ask in plain language, compare periods, follow a thread, and find the
pattern hiding in the mess.

Fella combines a Rust analytics engine with an opinionated AI harness. The
model drives interpretation, decomposition, tool choice, and explanation inside
a deliberately small read-only boundary. The engine catalogs files, queries
local data, searches documents, makes charts, compiles supported analytical
plans, and checks the result before it reaches you.

For analytics, a plausible answer is not good enough. The goal is to reach the
right answer the first time, with as little unnecessary reasoning between the
question and result as possible. That is why Fella does not ship as a general-
purpose agent with a large tool belt. Analytics rarely needs to write files,
run shell commands, browse the web, or act on your behalf.

The result is an opinionated system: small enough for the model to reason about,
powerful enough to cover the major families of personal analysis, and bounded
enough to trust with the files you already have.

## Three things Fella optimizes for

Fella does one useful thing deeply: it makes the personal data already on your
computer understandable without making the reasoning needlessly complicated.

| | |
| --- | --- |
| **Consistency** | A small fixed tool set and structured decomposition keep the route from question to analysis predictable across questions. |
| **Correctness** | The engine computes the numbers, the harness checks the work, and the result should be right the first time whenever the data can support it. |
| **Efficiency** | Deliberate context, bounded steps, duplicate-call avoidance, and only the tools analytics needs keep wasted reasoning and tokens down. |
| **Read-only by design** | Fella can analyze the workspace, but it cannot write, move, delete, send, or act. The boundary is part of the architecture, not a setting. |
| **Useful on real files** | Mount the folder you already have, ask in plain language, and get a concise analysis or chart without building a warehouse or learning SQL first. |
| **Bring your own model** | Connect OpenAI, Vercel AI Gateway, xAI, Ollama Cloud, OpenRouter, or a custom OpenAI-compatible endpoint. |

## From a question to an answer

```text
your folder, as it is
        ↓
bounded workspace + typed local tables
        ↓
model → observations · SQL · document search · bounded Python · charts
        ↓
patterns · comparisons · charts · caveats
        ↓
a clear answer you can use
```

The model is the analytical driver, not the source of truth for the numbers. It
can choose and revise a route through the data, but it reaches the workspace
only through bounded read-only tools. Fella's Rust analytics engine performs
the computation, optionally compiles a semantic plan, and a separate
verification pass checks the result against real data. If the folder cannot
support the claim, Fella should say so.

When you want to go deeper, the result opens up: source files, calculations,
and verification are there to inspect. Evidence supports the analysis without
getting in the way of using it.

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
- [Analytical computer roadmap](docs/ANALYTICAL-COMPUTER-ROADMAP.md) — runtime design, implementation status, and quality gates
- [Decisions](docs/DECISIONS.md) — the engineering decision log
- [Contributing](CONTRIBUTING.md) — contribution guidelines

## Status

Fella is an early, pre-1.0 release. The core personal analytics workflow is
implemented; interfaces and supported providers will continue to evolve.

## License

[MIT](LICENSE)

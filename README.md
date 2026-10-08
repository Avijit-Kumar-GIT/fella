<div align="center">
  <img src="docs/site/logo.svg" width="88" alt="Fella logo">
  <h1>Fella</h1>
  <p><strong>An opinionated analytics harness for the questions behind your data.</strong></p>
  <p>Make sense of messy files with a model-driven loop and a read-only local engine.</p>

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

General-purpose agents are built to take actions. Fella is built to help you
understand: ask a question, investigate the relevant context, and get an answer
whose basis you can inspect.

Your computer is full of data that never became insight: a year of expenses, a
workout log, project exports, PDFs, notes, and files with names you no longer
remember. Fella lets you ask about those files without first building a data
warehouse or learning SQL. A workspace organizes local evidence; it is context
for the conversation, not the product itself.

Fella combines a Rust analytics engine with an opinionated AI harness. The
model drives interpretation, decomposition, tool choice, and explanation inside
a reviewed, read-only boundary. The local engine catalogs files, queries data,
searches documents, makes charts, compiles supported analytical plans, and
checks computational claims. General questions can be asked without mounting
a folder. Visible web research when external sources matter remains future
work; the current release does not send questions or local data to a web-search
service.

For analytics, a plausible answer is not good enough. The goal is to reach the
right answer the first time, with as little unnecessary reasoning between the
question and result as possible. Fella does not need file writes, shell
commands, desktop control, or the ability to act on your behalf. When a
question benefits from outside information, the intended route is bounded,
read-only, and visible—not an unrestricted browser agent.

The result is an opinionated system: a model-led analysis loop with reviewed,
read-only tools, deterministic execution where exact computation matters, and
clear limits when the available evidence cannot support a claim.

## Three things Fella optimizes for

Fella does one useful thing deeply: it makes the personal data already on your
computer understandable without making the reasoning needlessly complicated.

| | |
| --- | --- |
| **Consistency** | A coherent model-led loop and reviewed tools keep the route from question to answer understandable. |
| **Correctness** | The engine computes the numbers, the harness checks the work, and the result should be right the first time whenever the data can support it. |
| **Efficiency** | Deliberate context and proportionate tool use keep wasted reasoning down without an arbitrary tool-count limit. |
| **Read-only by design** | Fella can analyze the workspace, but it cannot write, move, delete, send, or act. The boundary is part of the architecture, not a setting. |
| **Useful on real files** | Mount the folder when a question depends on it; ask in plain language and get an analysis or chart without building a warehouse or learning SQL first. |
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

**Upgrading from the published 0.2.0 Tauri app:** install the Electron build
manually from the releases page once; `/update` in the old shell cannot convert
the app. The new shell reuses the existing `dev.fella.app` data directory, so
settings, provider credentials, and conversation history remain available.
After that migration, `/update` checks and applies Electron releases.

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
- The current release has no write, shell, desktop-control, or web-research
  tools. The roadmap adds visible web research as a bounded read-only route;
  it must not send mounted-file contents, rows, snippets, workspace memory, or
  credentials to a search service.
- Provider API keys stay in local `auth.json` with restrictive permissions where
  supported; they are not stored in the settings database or browser storage.
- With a hosted provider, the question and the relevant tool results are sent
  to the provider you selected. With a local provider, they can remain on the
  machine.
- Updates happen only when explicitly requested with `/update`.

Read the [security policy](SECURITY.md) and [product commitments](docs/PRODUCT.md).

## Build from source

The desktop shell is Electron, with the SvelteKit UI and Rust analytics engine
running as a local sidecar. See [`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md) for
toolchain requirements and provider setup.

```sh
git clone https://github.com/Avijit-Kumar-GIT/fella.git
cd fella
pnpm install
pnpm electron:dev
```

Useful verification commands:

```sh
pnpm check
pnpm build
cargo test --manifest-path src-tauri/Cargo.toml --locked
```

The Electron shell and its line-delimited JSON bridge are described in
[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).

## Documentation

- [Product](docs/PRODUCT.md) — Fella's purpose, analyst loop, and boundaries
- [Architecture](docs/ARCHITECTURE.md) — the Electron shell and Rust analytics runtime
- [Roadmap](docs/ROADMAP.md) — current quality priorities and deferred work
- [Development](docs/DEVELOPMENT.md) — setup and contributor conventions
- [Testing](docs/TESTING.md) — regression, end-to-end, and model evaluation
- [Performance](docs/PERFORMANCE.md) — ingestion, build, and memory measurement
- [Release](docs/RELEASE.md) — packaging and release procedure
- [Design](docs/DESIGN.md) — UI tokens and interaction guidance
- [Mintlify deployment](docs/DOCS-DEPLOYMENT.md) — public docs hosting
- [Contributing](CONTRIBUTING.md) — contribution guidelines

## Status

Fella is an early, pre-1.0 release. The core personal analytics workflow is
implemented; interfaces and supported providers will continue to evolve.

## License

[MIT](LICENSE)

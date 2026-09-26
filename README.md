<p align="center"><img src="src-tauri/icons/icon.png" width="96" alt="Fella"></p>

# Fella

[![CI](https://github.com/Avijit-Kumar-GIT/fella/actions/workflows/ci.yml/badge.svg)](https://github.com/Avijit-Kumar-GIT/fella/actions/workflows/ci.yml)

[Documentation](https://docs.lilfella.app) · [Releases](https://github.com/Avijit-Kumar-GIT/fella/releases)

*The more an AI can do for you, the more it can do to you.*

**An opinionated analytics engine and harness for personal data.**

Turn messy files into consistent, correctness-first analysis—with no tool
sprawl and no write access.

<!-- TODO(demo): a ~20s GIF here, before anything else: open a folder of real-looking
     files -> ask a question -> a useful answer and chart appear. This is the
     single highest-leverage thing missing from this README. Record with the
     actual app; no product work needed. -->

General-purpose agents are built to do more. Fella is built to get one class of
work right: analytics over the data you already have.

Your computer is full of data that never became insight: a year of expenses, a
workout log, project exports, PDFs, notes, and files with names you no longer
remember. Fella gives those fragments a bounded workspace you can actually
explore. Ask in plain language, compare periods, follow a thread, and find the
pattern hiding in the mess.

Fella combines a Rust analytics engine with an opinionated AI harness. The
harness breaks broad questions into smaller analytical steps, keeps context
deliberate, and uses the fewest tools needed. The engine catalogs files, queries
local data, searches documents, makes charts, and checks the result before it
reaches you.

For analytics, a plausible answer is not good enough. The goal is to reach the
right answer the first time, with as little unnecessary reasoning between the
question and result as possible. Analytics rarely needs to write files, run
shell commands, browse the web, or act on your behalf, so Fella does not ship
with those capabilities.

The result is an opinionated system: small enough for the model to reason about,
powerful enough to cover the major families of personal analysis, and bounded
enough to trust with the files you already have.

Ask questions such as:

- *"How did my spending change this year?"*
- *"What patterns are in my workouts?"*
- *"Which factors affect my coffee brewing results?"*
- *"Summarize the trends in these documents."*

The model helps translate the question; the engine does the computation. The
primary result stays concise—a clear answer, a useful chart, and relevant
caveats. When you want to go deeper, the underlying queries, source rows, and
verification checks are available to inspect.

**Read-only.** Fella reads your folder; it never writes, moves or deletes anything.
Nothing leaves your computer except the request to the model provider you choose.
Fella is BYOK-only: you connect a provider with your own API key.

## Three things Fella optimizes for

| | |
| --- | --- |
| **Consistency** | A small fixed tool set and structured decomposition keep the route from question to analysis predictable across questions. |
| **Correctness** | The engine computes the numbers, the harness checks the work, and the result should be right the first time whenever the data can support it. |
| **Efficiency** | Deliberate context, bounded steps, duplicate-call avoidance, and only the tools analytics needs keep wasted reasoning and tokens down. |
| **Read-only by design** | The agent can analyze the workspace, but it cannot write, move, delete, send, or act. The boundary is part of the architecture, not a setting. |

## Philosophy

"Personal computer" used to mean the machine was actually yours. It kept
your secrets, and no institution stood between you and it. AI has spent its
whole existence undoing that word. Fella is a bet that the read-only,
local-first version isn't the compromise. It's the one that gets to keep
"personal" and actually mean it.

**Reads your files, computes real answers, never writes anything back.** Small,
fast, and resistant to feature bloat *in the base version*, which ships as one
binary with nothing bundled. Local-first. Minimal dependencies. It is for a
regular person doing enterprise-grade personal analytics—not analysts or
developers—so it stays plain-language throughout and copes with a messy
real-world folder. It is deliberately *not* a general task agent: no
file-management, no chores, and the base has a fixed, small tool set.
The shipped release keeps customization deliberately small: the user-authored
`fella.md` context file, provider/model settings, and appearance are the active
extension points. MCP is documented as an inert experimental command; packs and
augments are archived design work rather than runtime features. The whole thing
stays understandable by one person. See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for how it's built,
[`docs/CAPABILITY-POLICY.md`](docs/CAPABILITY-POLICY.md) for the experimental
analysis switches and longer-term enterprise reference,
[`docs/WHY.md`](docs/WHY.md) for the reasoning, [`docs/PRINCIPLES.md`](docs/PRINCIPLES.md)
for the commitments, and [`docs/NON-GOALS.md`](docs/NON-GOALS.md) for what it deliberately
doesn't do.

`docs/` has more than those four start with them. The rest (`DECISIONS.md`,
`AUDIT.md`, `HARNESS.md`, `QUESTIONS.md`, `PERFORMANCE.md`, ...) is the engineering
log we hold the project's own claims accountable against useful if you're
contributing or curious how a specific decision got made, not required reading.

## Requirements

- **macOS** 10.15+, **Windows** 10+, or **Linux** with WebKitGTK 4.1
  (`libwebkit2gtk-4.1`, present on current GNOME/KDE desktops).
- **A model provider.** Fella is BYOK-only. Connect [Ollama Cloud](https://ollama.com/settings/keys),
  OpenAI, Vercel AI Gateway, xAI, OpenRouter, or a custom OpenAI-compatible
  endpoint with `/login` and your own API key.
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

Then give Fella a model provider:

- **Ollama Cloud:** on first run type `/login`, pick **Ollama Cloud**, and paste an API key from
  [ollama.com/settings/keys](https://ollama.com/settings/keys) its free tier
  covers a set of starter models at no cost (1 request at a time; more models
  and concurrency need paid credits). Then `/model` picks the model.
- **Other providers:** Vercel AI Gateway, OpenAI, xAI, OpenRouter, or
  any OpenAI-compatible endpoint the same way (`/login`), at your discretion
  these are typically paid per token. An API key is kept in a `0600` file,
  never the database or the browser, regardless of provider.

Everything including questions about your PDFs and notes works on every
provider (Fella reads documents directly, no embedding step). Builds are
**unsigned** type `/update` any time to check for and install a newer
release yourself (it verifies the download against `SHA256SUMS` first,
same as the install scripts); there's no automatic background check.
Signing, notarisation, and Homebrew/winget are planned.

## Build from source

For contributors, or to run an unreleased revision. Needs Rust 1.93+, Node 22+
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
shows up only where data lines up (tables, queries). Open more than one thing at
once with tabs (`Ctrl+T`), each keeps its own conversation, and can even run its
own model. Slash commands below are a power-user shortcut; you never need them.

| Command | What it does |
|---------|--------------|
| `/open` | Choose a folder (or use the button / drag one in). `/open <path>` skips the picker |
| `/files` | List detected files and tables |
| `/schema <name>` | Show a table's columns, types and null rates |
| `/sql <query>` | Run SQL directly, bypassing the model (plain result; no evidence fold or post-answer verification) |
| `/login` `/logout` `/auth` | Sign in to a hosted provider (Vercel AI Gateway, OpenAI, xAI, Ollama Cloud, OpenRouter, or a custom OpenAI-compatible endpoint); list what's signed in |
| `/model` | Show or change the LLM provider, base URL and model. Per-tab: each tab can run a different model, but all tabs share one login |
| `/reindex` | Check the folder again for new or changed files |
| `/memory` | See what Fella has learned about this folder on its own (`/memory forget` clears it) |
| `/context` | Open `fella.md` in the Workspace editor: tell Fella how your files are organised and what your terms mean, in your own words |
| `/update` | Check for a newer release and install it (checksum-verified, same as the install scripts); Fella closes and you reopen it once the installer finishes |
| `/mcp` | Experimental and inert. No official connectors are enabled; custom implementations require a fork or experimental build |
| `/tab` | Open another conversation in a new tab |
| `/focus` | Hide the tabs and header for a plain view (again to undo) |
| `/clear` | Start a new conversation (the old one is saved) |
| `/history` | List your saved conversations; `/history <n>` reopens one in a new tab |
| `/retry` | Ask your last question again |
| `/help` | Show all commands |

**Keys:** `Enter` send · `Shift+Enter` newline · `↑` recall last input ·
`Ctrl+K` / `Ctrl+Shift+P` command palette · `Ctrl+N` new conversation ·
`Ctrl+T` new tab · `Ctrl+W` close tab · `Ctrl+[` / `Ctrl+]` previous or next tab ·
`Ctrl+1`…`9` switch tab · `Ctrl+Shift+A/S/C` Ask, Sources, Context ·
`Ctrl+,` settings · `Ctrl+O` open folder · `Ctrl+B` sidebar · `Ctrl+L` clear screen ·
`Ctrl+Shift+F` focus mode · `Esc` stop a running answer, otherwise collapse all evidence.
On macOS, use `Cmd` in place of `Ctrl`.

You can also click the pulsing dot next to the composer to stop a run. A stopped
run keeps whatever evidence it had gathered and answers `Stopped.`

### Going deeper

The result stays clean by default. Open the fold-away line like
`▸ working · 3 steps · 412 rows · 0.7s` when you want to understand how it was
worked out: the tools called, SQL or Python used, source rows, timings, and the
self-checks Fella ran afterwards. Follow-up questions in the same conversation
reuse what earlier turns already established—the schema, the useful context, and
the queries that worked—instead of starting over each time.

### Personalizing

Fella works with nothing set up. If you want to add local guidance, `/context`
opens `fella.md` in the Workspace editor so you can describe how your files are
organised and what your terms mean. The file can also be created by hand. `/mcp`
is present as an inert experimental signpost; it does not connect to anything in
the personal release.

## Contributing

Fella is open source and takes contributions to the **app**: features, fixes,
new file formats, engine, UI, sandbox, and verification work. Extension and MCP
designs remain archived for future forks. Start with
[`CONTRIBUTING.md`](CONTRIBUTING.md); the current release boundary is in
[`docs/LEAN-PERSONAL-RELEASE.md`](docs/LEAN-PERSONAL-RELEASE.md).

## Status

Early, pre-1.0 tagged builds land on the
[releases page](https://github.com/Avijit-Kumar-GIT/fella/releases). See
[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for the design,
[`docs/ROADMAP.md`](docs/ROADMAP.md) for the wish-list of small, in-scope
improvements, and [`docs/DECISIONS.md`](docs/DECISIONS.md) for why things are the
way they are.

## License

[MIT](LICENSE).

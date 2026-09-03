# Fella Extensibility

How Fella stays minimal at install while letting people who want more add it
themselves. Read `docs/DECISIONS.md` (2026-08-29 entries) for the decisions
that set this; this file is the maintained reference.

## The principle

**"No bloat" is about the base version.** Fella ships as one binary with
nothing bundled: no packs, no connectors, no extra dependencies. A
non-technical person installs it, points it at a folder, and gets good answers
without ever knowing packs exist. Default answer quality never depends on a
pack.

**Beyond the base, it's the user's call.** Someone who wants Fella to speak
their domain's vocabulary, to look different, or to read a source outside the
local folder can install a pack for it. Fella does not gatekeep *whether* you
extend. What the core team does is **vet** every pack it lists in the
marketplace: read the source, confirm it does what it says.

**Trust comes from two places:** the base is small and auditable, and
marketplace entries are reviewed and content-hashed. Fella guarantees you *can*
install a pack, not that every pack is safe. Anything you install from outside
the marketplace is marked unverified.

## What a pack is

A pack is **exactly one of three kinds**. Nothing else is a pack.

| Kind | Payload | What it does | Cost when active |
|------|---------|--------------|------------------|
| **`theme`** | `theme.json` a map of the app's `:root` CSS tokens (`src/app.css`), optionally an `appearance` hint | changes colours and spacing; one theme active at a time | none; inert data |
| **`skill`** | `skill.md` Markdown, size-capped | text injected into the system prompt as a "Your context" section: vocabulary, file conventions, caveats the model should always apply. Several can be enabled at once. It can only shape how Fella words and interprets an answer there is no tool for it to call. | none; inert text |
| **`mcp`** | `connector.json` (`{ "transport": "http", "url", "auth", "setup" }`) | lets Fella read a source outside the local folder (Notion, a notes repo, a wiki) via a **remote** [Model Context Protocol](https://modelcontextprotocol.io) server. The server's tools appear in the agent loop and evidence panel like the built-ins, namespaced `<id>__<tool>`. The token goes in `auth.json` (`Secrets`); you paste it with `/connect <id>`. | a live connection only while an enabled `mcp` pack is used |

A pack is a directory with a `fella-pack.json` manifest plus its one payload
file. No app code. No archive format, no package manager.

### `mcp` connectors in detail

- **Streamable HTTP only.** A connector is a URL plus a pasted token; there is
  no local process. `connector.json`:
  ```json
  {
    "transport": "http",
    "url": "https://mcp.example.com/mcp",
    "auth": { "type": "bearer", "secret": "EXAMPLE_TOKEN" },
    "setup": "Where to get the token and what access it grants."
  }
  ```
  `auth` is `{"type":"none"}`, `{"type":"bearer","secret":"<VAR>"}`, or
  `{"type":"header","header":"X-Api-Key","secret":"<VAR>"}`. stdio (local
  subprocess) servers are not supported yet.
- **Read-only by default.** A tool the server marks `readOnlyHint: true` is
  offered normally. One with no annotation is offered but its evidence row is
  marked "effects not declared". One marked `readOnlyHint: false` or
  `destructiveHint: true` is withheld, and Fella says so.
- **The MCP client is the `mcp` build feature** (in the shipped app;
  `--no-default-features` drops it). It uses the official `rmcp` SDK over
  Fella's existing HTTP client.
- **`/connect`** manages connectors: `/connect` lists them and their status,
  `/connect <id>` pastes the token and turns it on, `/connect <id> off` /
  `forget` turn it off / clear the token.

Precedent for inert, user-editable config: `.fellaignore` in a workspace root
(`src-tauri/src/engine/catalog.rs`). A per-workspace `fella.md` is the same
idea for `skill`-style context, kept local and unmanaged.

## Two ways to contribute

Contributing to Fella is not limited to packs. See `CONTRIBUTING.md`.

1. **The app** (`fella` repo). Features, fixes, **new file-format support**,
   **new built-in tools**, engine or UI work anything that needs app code.
   Normal open-source flow: issue, discussion, PR, review, ships in the next
   release binary. A new file format or a new tool is this lane, never a pack.
2. **Packs** (`fella-extensions` repo). Themes, skills, and MCP connectors, via
   that repo's `CONTRIBUTING.md` and its per-kind rules. No app code. Lighter
   review bar.

The split is about where a change lives and how it is reviewed, not about who
may contribute.

## The marketplace — three repos

**Rollout status (2026-09-02):** the hosted side is paused until there's demand
for customisation — the browse site isn't deployed, `fella-extensions` is still
private, and the install-counter proxy is written but not running. What works
today: local `/packs add <path>` (offline) and by-id `/packs install <id>`
against the `fella-extensions` catalog. The design below is unchanged; only the
rollout is held. See [`DECISIONS.md`](DECISIONS.md), 2026-09-02.

| Repo | Visibility | Holds |
|------|-----------|-------|
| `fella` (this repo) | public | the app. Reads `catalog.json` from a stable URL; installs a pack by id. |
| `fella-extensions` | **public** | `packs/<id>/` (manifest + `README.md` + `LICENSE` + payload), the `fella-pack.json` schema, the pack rules, and a **generated** `catalog.json` (`scripts/build-catalog.mjs`; CI fails if it's stale). Pack PRs land here; the app fetches the raw `catalog.json`. |
| `fella-web` | private | the front-end sites `marketing/` (the landing page + docs) and `marketplace/` (a gallery that renders `catalog.json`). Presentation only; the app never depends on it. |

`fella-extensions` is public for two reasons: community packs arrive as PRs, and
the app fetches `catalog.json` over an unauthenticated URL. Presentation has
neither constraint, so `fella-web` is private.

The app reads only **`id`** and **`files[].{path,url,sha256}`** from each entry;
install fetches each file, verifies its hash, and writes it to
`<app-data>/extensions/<id>/`. Everything else in an entry is for the browse
site: `kind`, `name`, `version`, `description`, `author`, `license`, `homepage`,
`published` / `updated` (from git history), `size_bytes`, `installs` (nullable —
see below), and `readme_html` (the pack's `README.md`, rendered to sanitised
HTML at build time). Unknown fields are ignored, so the catalog can grow without
an app change.

- **First-party packs** live in `fella-extensions/packs/<id>/`, so their
  `files[].url` tracks `main` — content is still SHA-256-locked and a maintainer
  owns `main`. A pack hosted in an outside repo would pin its URLs to a **commit
  SHA** (that repo can force-push); not wired up yet.
- **Submitting a pack** = a PR to `fella-extensions` adding `packs/<id>/` and the
  regenerated `catalog.json`. The core team reviews the content; the hashes lock
  it.
- `installs` is `null` until pack downloads route through a Fella-controlled
  endpoint that counts them (so the app never has to phone home — see
  `docs/DECISIONS.md`, 2026-09-02). Until then the site shows a "New" badge.
- Nothing auto-updates. Nothing nags on launch.

## In the app

One command, `/packs`, following the `completionsFor()` +
`COMMAND_DESCRIPTIONS` discoverability pattern (`src/lib/commands.ts`):

- `/packs` what's installed.
- `/packs browse` opens the packs repo (the browse website isn't live yet — see
  the rollout note above); the marketplace website is where you'll find a pack's
  id once it ships.
- `/packs install <id>` installs by id from the catalog (each file
  hash-checked). The app never renders the catalog itself.
- `/packs add <path>` a local pack directory; marked **unverified**, with a
  line on what it can do or reach.
- `/packs remove <id>`, `/packs enable|disable <id>`.

Installs land in `<app-data>/extensions/`, a sibling of `auth.json` and
`conversations/`.

## Where skills and packs live

Two scopes, and deliberately **not** the `.agents/skills/` model that Pi and fx
use (a shared cross-tool directory, an ancestor-directory walk to the git root,
project-trust prompts) that is coding-agent machinery for git workflows Fella's
audience does not have, and a Fella pack (written against Fella's fixed,
read-only tools) is not portable to those tools anyway.

- **Global**: `<app-data>/extensions/<id>/` installed packs. This is Fella's
  equivalent of `~/.claude/skills/`, under the `dev.fella.app` identifier.
- **Workspace**: `fella.md` at the folder root. Inert text; no trust prompt
  because Fella is read-only.

## What is not a pack

- **A new file format, or a new agent tool.** That is app code. Open an issue
  or PR on `fella` (lane 1).
- **Anything the base needs for a good first run.** If a non-technical user
  would want it out of the box, it belongs in the base.
- **A "full feature" delivered as an add-on.** There is no plugin runtime and
  no package-manager ecosystem; the three kinds above are the whole surface.

## How this squares with the non-negotiables

- **Read-only workspace** is unchanged and unconditional. Fella's own tools
  never write, move, or delete anything in your folder. An `mcp` connector is a
  separate data source, surfaced as evidence, not a way to write the folder.
- **Nothing leaves the machine** describes the base. An enabled `mcp` connector
  talks to its own service because you set it up to.
- **Credentials** for a connector use the same `auth.json` store and rules.
- **Deterministic, shows its working** is unchanged: connector tool calls
  appear in the evidence panel like the built-ins.

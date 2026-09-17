# Security

## Reporting a vulnerability

Please report security issues **privately**, not as a public issue:

- GitHub → the repo's **Security** tab → **Report a vulnerability** (private
  advisory), or
- email the maintainer (see the profile on
  <https://github.com/Avijit-Kumar-GIT>).

Include what you did, what you observed, and the impact. We'll acknowledge
within a few days and keep you posted on a fix.

If the issue looks like it could be exploited, describe the class of problem
rather than including a working exploit.

## What Fella guarantees (the base app)

- **Read-only agent.** Fella's agent reads the folder you point it at; it never
  writes, moves, or deletes anything there, and the model never emits a file.
  There is no write tool to disable none was built. (An opt-in `augment`
  pack adds a tab where *you* save a note or table you typed into one named
  file; see below.)
- **BYOK model access.** The only model network call the base app makes on its
  own is the request to the provider you chose with your own API key.
  `/packs install` and `/update` reach GitHub, but only when you type one of
  those commands never automatically, never on startup.
- **Credentials** (API keys, and tokens for `mcp` connector packs) live in
  `auth.json` (mode `0600`) in the OS app-data directory never in the
  settings database, `localStorage`, or the transcript, and never echoed.
- **Deterministic answers.** Figures in an answer come from a tool result (SQL
  or Python), checked by a verification pass never from the model directly.

## Extensions change this, by your choice

Installing a pack is opt-in:

- A **theme** or **skill** pack is inert data (CSS tokens / Markdown) it
  cannot execute code or make network calls.
- An **`mcp` connector** pack connects to a remote MCP server you configure.
  That server runs elsewhere with your credentials and may reach the network
  installing one is your informed decision.
- An **`augment`** pack turns on a built-in capability (a notes tab, a small
  table) and binds it to a slash command. It ships no code. The tab saves a
  file **into your open folder when you type in it** the first time a pack
  can make Fella write there. Only your keystroke writes, only the one file the
  manifest names, and the agent's tools are unchanged (still no write tool).

Fella vouches only for the code review of packs listed in the vetted catalog;
anything you side-load is marked **unverified**.

`run_python` executes code the model writes inside the checked-in
`wasm32-unknown-unknown` RustPython guest through Wasmi. The guest receives no
WASI imports and has no filesystem, network, environment, or subprocess access;
its only host capabilities are captured output, OS entropy for interpreter hash
maps, and a bounded read-only `sql()` bridge. The host applies fuel, stack,
memory, source, output, SQL-row, and SQL-response limits, while the data engine
keeps its normal query timeout.

This is a stronger capability boundary for generated analytics code, but still
defense in depth rather than a promise that a vulnerability in Wasmi,
RustPython, or the signed guest artifact is impossible. The guest has a small
Python core and the built-in analytics helpers; it does not provide pandas,
NumPy, SciPy, arbitrary packages, or a package installer.

## Build integrity

Release builds are produced by GitHub Actions from a tagged commit. Until code
signing / notarisation is in place they are **unsigned** verify a download's
checksum against the release notes, or build from source.

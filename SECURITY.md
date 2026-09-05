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

- **Read-only.** Fella reads the folder you point it at; it never writes,
  moves, or deletes anything there.
- **Local-first.** The only network call the base app makes on its own is the
  request to the model provider you chose (a local Ollama by default).
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
  installing one is your informed decision. Fella vouches only for the code
  review of packs listed in the vetted catalog; anything you side-load is
  marked **unverified**.

`run_python` executes code the model writes, in a restricted subprocess
(`python3 -I`, a fresh temp working directory, a stripped environment, `RLIMIT_*`
for CPU/memory/file size, a wall-clock timeout). This is best-effort isolation for
analysing your own data, not a hostile-code sandbox: it does **not** confine
filesystem reads to the workspace or block network access. Treat a snippet as
code you chose to run on your own machine.

## Build integrity

Release builds are produced by GitHub Actions from a tagged commit. Until code
signing / notarisation is in place they are **unsigned** verify a download's
checksum against the release notes, or build from source.

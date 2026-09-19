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
  The user may explicitly edit the root `fella.md` context file from the
  Workspace surface; that editor is separate from the agent and is the only
  workspace write in the personal release.
- **BYOK model access.** The only model network call the base app makes on its
  own is the request to the provider you chose with your own API key.
  `/update` reaches GitHub only when you type the command, never automatically
  and never on startup. `/mcp` is an inert experimental command.
- **Credentials** (provider API keys) live in `auth.json` (mode `0600`) in the
  OS app-data directory, never in the settings database, `localStorage`, or the
  transcript, and are never echoed.
- **Deterministic answers.** Figures in an answer come from a tool result (SQL
  or Python), checked by a verification pass never from the model directly.

## Experimental extension boundary

The personal release has no pack manager, augment runtime, or MCP connector.
`/mcp` is retained as an inert signpost and creates no connector or network
activity. The root `fella.md` file is the supported user-authored context
surface. Extension designs are archived for future custom forks; see
[`docs/EXTENSIBILITY.md`](docs/EXTENSIBILITY.md).

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

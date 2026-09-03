# Contributing to Fella

Fella is a small, local-first desktop app for personal analytics by
non-developers. Before proposing a change, read `README.md`,
`docs/ARCHITECTURE.md`, and `docs/EXTENSIBILITY.md` so a change fits the
project's shape. `docs/DECISIONS.md` is the log of why things are the way they
are.

## Two ways to contribute

**1. The app (this repo).** Features, bug fixes, new file-format support, new
built-in tools, engine or UI work. This is the normal path for anything that
needs app code. Flow: open an issue, discuss the approach, send a PR, it ships
in the next release.

**2. Packs (the `fella-extensions` repo).** A pack is one of exactly three
things: a **theme**, a **skill** (instructions/vocabulary fed to the model), or
an **mcp** connector. Packs carry no app code. They are submitted, reviewed, and
listed in the marketplace repo, not here. See its `CONTRIBUTING.md` for the
rules and the per-kind bar.

If you are unsure which lane a change belongs in: anything that changes what the
compiled binary can do is lane 1; anything that is only data the app reads
(colours, prompt text, a connector config) is lane 2.

## What the app will and won't take

Fella's positioning is locked (`docs/EXTENSIBILITY.md`, `docs/DECISIONS.md`):
personal analytics for non-developers, read-only, local-first, anti-bloat in the
base. Changes are reviewed against these non-negotiables:

- **Read-only.** Nothing writes, moves, or deletes anything in the user's folder.
- **Local-first.** The base makes one network call, to the model the user chose.
- **Credentials** live in `auth.json` (mode `0600`), never the settings DB,
  `localStorage`, or the transcript.
- **Anti-bloat in the base.** A new dependency needs a real justification. No
  settings modal. The codebase stays understandable by one person.
- Not a general task agent: no file management, no chores.

Good contributions: a new file format behind the existing ingest path; a
narrowly useful built-in tool; performance work; clearer plain-language copy; a
bug fix with a test. Out of scope: turning Fella into a coding agent, write
tools, a plugin runtime, anything that assumes the user is a developer.

## Development

Full environment notes are in `docs/DEV_SETUP.md`. In short:

```sh
pnpm install
pnpm tauri dev
```

Before you open a PR, all four must pass (CI runs the same on Linux):

- `cargo test --locked` from `src-tauri/` (**SQLite default features only**
  never `--features duckdb` it is CI-only and OOMs most machines)
- `cargo clippy --all-targets --locked -- -D warnings` from `src-tauri/`
- `pnpm run check` 0 errors, 0 warnings
- `pnpm run build`

rustfmt is not enforced match the style of the code around your change:
comment density, naming, and idiom. For UI work, `docs/DESIGN.md` has the
visual and interaction rules.

## Pull requests

- Branch off `main`. One logical change per PR.
- Commit messages follow Conventional Commits, matching the existing log:
  `feat(agent): ...`, `fix(connect): ...`, `docs: ...`, `chore: ...`.
- If your change alters a design decision, add a dated entry to
  `docs/DECISIONS.md` in the same PR. If it changes something described in
  `docs/ARCHITECTURE.md`, update that too.
- Fill in the PR template: what changed, which lane, how you verified it.
- New behaviour needs a test. The engine is testable without the UI see the
  existing `src-tauri/tests/`.

## Reporting bugs and proposing features

Use the issue templates. A good bug report includes what you did, what you
expected, what happened, and your OS. A feature proposal should say who it helps
(the audience is non-developers) and why it belongs in the base rather than a
pack.

## Conduct

Be decent, and assume good faith. Harassment or personal attacks get you removed
from the project.

# Principles

*The positive commitments behind Fella — what it always does, not what it
refuses to. Referenced from [`WHY.md`](WHY.md); the refusals are in
[`NON-GOALS.md`](NON-GOALS.md); the dated history of how each got decided is
[`DECISIONS.md`](DECISIONS.md).*

- **Enterprise-grade personal analytics, for non-developers.** A regular person points Fella at
  their own folder of files (statements, health exports, notes, logs) and asks
  questions about their own life in plain language. Not a tool for analysts —
  the audience doesn't write SQL or Python, so the app doesn't ask them to.
- **Read-only.** Fella reads the folder; it never writes, moves, or deletes
  anything in it, and it produces answers, not files. This is the single
  safety guarantee and it's structural: there is no write tool to disable.
  The user may explicitly edit the root `fella.md` context file; the model
  cannot write it.
- **Deterministic, auditable answers.** Every number comes from a real
  computation SQL, or Python when SQL can't express it never from the model
  guessing. Every answer carries the exact steps, queries, and rows behind it,
  open for inspection.
- **Local-first.** The base makes one model network call to the provider the
  user chose. Nothing else leaves the machine during ordinary analysis; `/mcp`
  is inert and `/update` runs only when explicitly invoked.
- **Credentials stay local and scoped.** An API key lives in `auth.json`
  (mode `0600`), never the settings database, `localStorage`, or the
  transcript.
- **Anti-bloat in the base.** A new dependency needs a real justification.
  No settings modal. Minimal dependencies, small binary, fast startup. The
  codebase stays understandable by one person. "Lightweight" here is
  actually four separable things (binary/dependency weight, runtime
  performance, codebase simplicity, feature scope) — see
  [`LIGHTWEIGHT.md`](LIGHTWEIGHT.md) for which parts of the engine are
  allowed to spend weight on which axis, and which must not.
- **A fixed, small tool set.** The personal release keeps customization to
  provider/model settings, appearance, and the user-authored `fella.md` file.
  Pack, augment, and connector designs remain archived rather than becoming
  default runtime surfaces.

See also [`ARCHITECTURE.md`](ARCHITECTURE.md#what-fella-is) for how these
translate into the actual build, and [`NON-GOALS.md`](NON-GOALS.md) for the
refusals these same commitments imply.

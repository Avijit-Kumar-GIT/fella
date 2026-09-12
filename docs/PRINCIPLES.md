# Principles

*The positive commitments behind Fella — what it always does, not what it
refuses to. Referenced from [`WHY.md`](WHY.md); the refusals are in
[`NON-GOALS.md`](NON-GOALS.md); the dated history of how each got decided is
[`DECISIONS.md`](DECISIONS.md).*

- **Personal analytics, for non-developers.** A regular person points Fella at
  their own folder of files (statements, health exports, notes, logs) and asks
  questions about their own life in plain language. Not a tool for analysts —
  the audience doesn't write SQL or Python, so the app doesn't ask them to.
- **Read-only.** Fella reads the folder; it never writes, moves, or deletes
  anything in it, and it produces answers, not files. This is the single
  safety guarantee and it's structural, not policy there is no write tool to
  disable. (An opt-in `augment` pack adds a tab where *you* save a note or
  table you typed that's a user keystroke writing one named file, never the
  model. See [`EXTENSIBILITY.md`](EXTENSIBILITY.md).)
- **Deterministic, auditable answers.** Every number comes from a real
  computation SQL, or Python when SQL can't express it never from the model
  guessing. Every answer carries the exact steps, queries, and rows behind it,
  open for inspection.
- **Local-first.** The base makes one network call: to the model the user
  chose (a local one by default). Nothing else leaves the machine.
- **Credentials stay local and scoped.** An API key lives in `auth.json`
  (mode `0600`), never the settings database, `localStorage`, or the
  transcript.
- **Anti-bloat in the base.** A new dependency needs a real justification.
  No settings modal. Minimal dependencies, small binary, fast startup. The
  codebase stays understandable by one person.
- **A fixed, small tool set in the base**, customization opt-in and pushed to
  the edges (themes, skills, MCP connectors, augments a user installs
  themselves see [`EXTENSIBILITY.md`](EXTENSIBILITY.md)) rather than grown
  into the base every install ships regardless of who uses it.

See also [`ARCHITECTURE.md`](ARCHITECTURE.md#what-fella-is) for how these
translate into the actual build, and [`NON-GOALS.md`](NON-GOALS.md) for the
refusals these same commitments imply.

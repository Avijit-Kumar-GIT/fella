# Non-goals

*What Fella deliberately does not do, and why. Referenced from
[`WHY.md`](WHY.md); the positive commitments are in
[`PRINCIPLES.md`](PRINCIPLES.md); the dated history of how each of these got
decided is [`DECISIONS.md`](DECISIONS.md).*

- **Not a task agent.** The agent has no write/move/delete tools and emits no
  artifacts; no permission dialogs see [`AUDIT.md`](AUDIT.md). Fella answers
  questions; it doesn't do chores. (An opt-in `augment` pack adds a note/table
  tab the *user* saves, see [`EXTENSIBILITY.md`](EXTENSIBILITY.md); the
  agent's own tools are unchanged.)
- **Not horizontal.** The zero-config base build stays small in feature count
  and spends its weight on intelligence and quality-of-life better answers,
  fewer interactions, faster and lighter not more commands. A capability
  earns its place by making the existing job better or shorter, not by adding
  a parallel thing to do; breadth lives in extensions. (`DECISIONS.md`
  2026-09-08.)
- **A fixed, small tool set in the base.** Adding a built-in tool or a
  file-format parser is a code change, not a plugin. Beyond the base, users
  can install vetted themes, skills, MCP connectors, and augments themselves
  see [`EXTENSIBILITY.md`](EXTENSIBILITY.md).
- **MCP is opt-in, not bundled.** Fella ships an MCP client so a user can
  connect an external source (Notion, a notes repo); no connector ships by
  default and none is a core dependency. (Reversed 2026-08-29; was "No MCP".)
- **No server / cloud / Docker / Redis.** One desktop process. SQLite for app
  state, in-process DuckDB (opt-in) for analysis.
- **No generic agent framework.** One reasoning loop, purpose-built, ~one
  file.
- **No terminal roleplay.** It's a REPL, but sans-serif and plain-language;
  monospace only where data lines up.
- **The model never touches data directly.** It can only call deterministic
  tools; all numbers in an answer must come from a tool result.
- **SQL first.** Python is the escape hatch, not the default.

See also [`ARCHITECTURE.md`](ARCHITECTURE.md#what-fella-is) for what Fella
*is*, and the "Would need a positioning decision" section of
[`ROADMAP.md`](ROADMAP.md) for ideas that touch one of these and haven't been
decided either way yet.

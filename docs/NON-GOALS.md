# Non-goals

*What Fella deliberately does not do, and why. Referenced from
[`WHY.md`](WHY.md); the positive commitments are in
[`PRINCIPLES.md`](PRINCIPLES.md); the dated history of how each of these got
decided is [`DECISIONS.md`](DECISIONS.md).*

- **Not a task agent.** The agent has no write/move/delete tools and emits no
  artifacts; no permission dialogs. Fella answers
  questions; it doesn't do chores. The user may explicitly edit `fella.md`,
  which is context for the agent rather than an agent-generated artifact.
- **Not horizontal.** The zero-config base build stays small in feature count
  and spends its weight on intelligence and quality-of-life better answers,
  fewer interactions, faster and lighter not more commands. A capability
  earns its place by making the existing job better or shorter, not by adding
  a parallel thing to do; future breadth belongs in reviewed custom forks.
  (`DECISIONS.md`
  2026-09-08.)
- **A fixed, small tool set.** Adding a built-in tool or a file-format parser
  is a code change. The personal release has no plugin, pack, or connector
  runtime; the former extension design is archived for future forks.
- **MCP is experimental and inert.** `/mcp` remains documented as a future
  boundary, but the default build has no MCP client, connector credentials,
  dynamic tools, or outbound connector path.
- **No server / cloud / Docker / Redis.** One desktop process. SQLite for app
  state, in-process DuckDB (opt-in) for analysis.
- **No generic agent framework.** One reasoning loop, purpose-built, ~one
  file.
- **No terminal roleplay.** It's a REPL, but sans-serif and plain-language;
  monospace only where data lines up.
- **The model never touches the workspace directly.** It drives the fixed
  read-only tools, while all numbers in an answer must come from a bounded tool
  result or a computation replayable by the engine.
- **SQL first.** Python is the escape hatch, not the default.

See also [`ARCHITECTURE.md`](ARCHITECTURE.md#what-fella-is) for what Fella
*is*. Reopening a non-goal requires a concrete use case and an explicit entry
in [`DECISIONS.md`](DECISIONS.md); the current runtime direction is in
[`ANALYTICAL-COMPUTER-ROADMAP.md`](ANALYTICAL-COMPUTER-ROADMAP.md).

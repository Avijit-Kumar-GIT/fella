# Non-goals

*What Fella deliberately does not do, and why. Referenced from
[`WHY.md`](WHY.md); the positive commitments are in
[`PRINCIPLES.md`](PRINCIPLES.md); the dated history of how each of these got
decided is [`DECISIONS.md`](DECISIONS.md).*

- **Not a computer-use or task-execution agent.** Fella has no tools to write,
  move, or delete files, operate desktop apps, or complete external chores.
  It may inspect, research, analyze, estimate, and explain; it does not act on
  the user's behalf. The user may explicitly edit `fella.md`, which is context
  for the agent rather than an agent-generated artifact.
- **Not a horizontal task agent.** Ask may answer general informational
  questions and use visible web research, but Fella does not become an agent
  that writes code, operates desktop apps, runs shell commands, sends messages,
  or completes chores. Breadth is about answering questions well, not gaining
  arbitrary powers.
- **No arbitrary extension runtime.** Built-in, audited capabilities may grow
  when they improve Ask. Fella does not load user-supplied code, arbitrary
  plugins, or a dynamic connector marketplace in the personal release.
- **MCP is experimental and inert.** `/mcp` remains documented as a future
  boundary; this product direction does not authorize arbitrary connectors or
  dynamic tools. A reviewed web-research route is a separate, bounded
  capability.
- **No server / cloud / Docker / Redis.** One desktop process. SQLite for app
  state, in-process DuckDB (opt-in) for analysis.
- **No arbitrary action runtime.** The model-driven analysis loop may inspect,
  decompose, use multiple tools, and revise its work as needed. Fella does not
  grow into a shell/browser/desktop-control harness or expose arbitrary code
  execution outside its bounded, read-only analysis capabilities.
- **No terminal roleplay.** It's a REPL, but sans-serif and plain-language;
  monospace only where data lines up.
- **The model never touches the workspace directly.** It uses bounded,
  read-only workspace tools. Claims about the user's data must be grounded in
  relevant file evidence or computation; general explanations and researched
  claims use their own appropriate basis.
- **No mandated analysis language or fixed tool route.** Let the model choose
  among suitable read-only tools, SQL, and sandboxed Python. The runtime
  validates scope and execution; benchmarks judge the result and its support,
  not whether a particular internal route was selected.
- **Forecasts are not categorically out of scope.** A forecast or scenario may
  be answered from available evidence with its method, assumptions, and
  uncertainty made clear. If relevant history or time grain is absent, explain
  that specific limitation and ask for the missing information or provide a
  useful partial result; do not treat “future-facing” alone as a reason to
  refuse.

See also [`ARCHITECTURE.md`](ARCHITECTURE.md#what-fella-is) for what Fella
*is*. Reopening a non-goal requires a concrete use case and an explicit entry
in [`DECISIONS.md`](DECISIONS.md); the current runtime direction is in
[`ANALYTICAL-COMPUTER-ROADMAP.md`](ANALYTICAL-COMPUTER-ROADMAP.md).

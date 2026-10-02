# Principles

*The positive commitments behind Fella — what it always does, not what it
refuses to. Referenced from [`WHY.md`](WHY.md); the refusals are in
[`NON-GOALS.md`](NON-GOALS.md); the dated history of how each got decided is
[`DECISIONS.md`](DECISIONS.md).*

- **Enterprise-grade personal analytics, for non-developers.** A regular person points Fella at
  their own folder of files (statements, health exports, notes, logs) and asks
  questions about their own life in plain language. Not a tool for analysts —
  the audience doesn't write SQL or Python, so the app doesn't ask them to.
- **Ask is broader than the workspace.** A folder organizes and grounds
  questions about local data; it is not required for general questions. Use
  model knowledge for stable explanations and visible, read-only web research
  when current information or sources materially help. These routes support
  the analytics product; they do not turn Fella into a general computer-use or
  task-execution agent.
- **Read-only.** Fella reads the folder; it never writes, moves, or deletes
  anything in it, and it produces answers, not files. This is the single
  safety guarantee and it's structural: there is no write tool to disable.
  The user may explicitly edit the root `fella.md` context file; the model
  cannot write it.
- **Model-driven analysis, deterministic execution.** The model inspects,
  interprets, decomposes, and may generate SQL or Python as in a coding
  harness. Fella's read-only analytics engine executes that work and returns
  evidence the model can use to revise or explain its answer. Analytical
  figures come from computation or source evidence, not unsupported guessing.
  The execution trace remains available for inspection.
- **Useful answer coverage, without bluffing.** Reliability is not achieved by
  refusing whenever meaning is uncertain. Hard limits protect access and
  execution; semantic uncertainty should prompt further inspection, a
  disclosed assumption, a partial or comparative answer, or a focused
  clarification—not an automatic block. Forecasts and scenarios are allowed;
  distinguish the estimate from observed facts and communicate the material
  method, assumptions, and uncertainty. Say the data cannot answer only when
  no usable evidence path remains, and then explain the actual missing evidence.
- **Local-first, transparent egress.** Workspace discovery, ingestion, and
  computation stay local. The user-selected model provider receives the
  question and any context/tool results needed to answer it. When web research
  is used, make that route visible and send only a minimal, generalized search
  query—never mounted-file contents, rows, snippets, workspace memory, or
  credentials. Pages are untrusted source material, not instructions. `/mcp`
  remains inert and `/update` runs only when explicitly invoked.
- **Credentials stay local and scoped.** An API key lives in `auth.json`
  (mode `0600`), never the settings database, `localStorage`, or the
  transcript.
- **Efficient, maintainable software.** Keep startup, memory, build size, and
  code complexity visible through measurement. A new dependency needs a real
  justification, but no arbitrary size or tool-count cap should prevent a
  meaningful improvement to analytical quality. Current measurements and
  profiling steps are in [`PERFORMANCE.md`](PERFORMANCE.md) and
  [`PERFORMANCE-LOG.md`](PERFORMANCE-LOG.md).
- **A small, reviewed tool set—not a tool-count ceiling.** Every capability
  must improve the Ask experience and remain bounded, auditable, and
  proportionate. Fella has no arbitrary-code plugin or general-purpose action
  runtime. Provider/model settings, appearance, and user-authored `fella.md`
  remain the supported customization points.

The no-folder general-answer route and web-research tool are product direction
and are not yet shipped by the current runtime. Their implementation order,
privacy contract, and acceptance checks are in
[`PRODUCT-ROADMAP.md`](PRODUCT-ROADMAP.md).

See also [`ARCHITECTURE.md`](ARCHITECTURE.md#what-fella-is) for how these
translate into the actual build, and [`NON-GOALS.md`](NON-GOALS.md) for the
refusals these same commitments imply.

# Goals

The philosophies, the v1 scope line, and the order, in one place, written
short on purpose. If a claim here needs a paragraph to defend, it belongs
in `DECISIONS.md` instead, not here.

## Philosophies

- Depth, not reach.
- Lightweight is four axes, not one word (`LIGHTWEIGHT.md`).
- Every number has a receipt.
- Plain math stated clearly beats sophisticated math stated confusingly.
- If you can't explain it in four sentences, it isn't finished, it's just built.

## The four-sentence test

The actual analytics engine, in four sentences, the bar every subsystem
gets held to before it counts as documented:

> Point fella at a folder. It turns your files into a small local
> database. When you ask a question, the model writes SQL against it and
> fella re-runs that exact query itself before you see the answer, if the
> number doesn't match, it says so instead of guessing. If the question is
> a comparison, it can draw a plain chart instead of a wall of numbers.

## Analytics engine, v1 scope

**In:**
- SQL execution over ingested files (SQLite default, DuckDB opt-in)
- Deterministic verification (re-run cited queries, flag untraced figures)
- Basic stats only: median, stdev, correlation, linear regression — stdlib, zero new deps
- Charts: bar/line, colored, validated against flat/degenerate data
- `depth_rule`: decompose a change/trend/correlation question before answering
- One brief, notable aside on a plain lookup, when the data actually
  supports it — ships only behind an eval case that proves it helps, not
  on faith
- An eval tier that actually tests decomposition/correlation questions —
  doesn't exist yet; the current biggest hole

**Out. Not later, out:**
- Hypothesis testing, significance, p-values
- Non-linear correlation, robust regression
- Any math upgrade that needs a new dependency

**Adjacent, not part of v1:** MCP server mode (the same six tools, reachable
by an outside agent, not new capability, a new door). Its own initiative
once v1 is actually done, not a v1 blocker.

## Release goals

1. Every major subsystem passes the four-sentence test — analytics,
   verification, memory, charts.
2. Binary stays at the SQLite-default footprint; DuckDB stays opt-in, last resort.
3. Tool count stays at 7. A new one needs an issue + a `DECISIONS.md` entry, same bar as always.
4. Every new prompt rule ships with an eval case, not just a description.
5. MCP-server mode is named and scoped, not built, not blocking this release.

## Tech stack, and why it isn't changing

Rust + Tauri + SQLite fits a single-user, local-first, offline desktop
app: one native binary, no server process, no runtime dependency.
Supermemory's TS + EffectTS + Hono + Postgres + Drizzle fits their actual
problem, a multi-tenant cloud service at real scale, that's a correct
stack for that shape of problem, not a better one in general. Swapping
fella to it would trade the native-binary, no-server-process property
(the whole basis of axis 1, `LIGHTWEIGHT.md`) for ecosystem convenience
fella doesn't need. The one place a different stack already applies
correctly: the frontend is Svelte/TS, because a webview UI is a genuinely
different problem than the local engine. That split stays.

## Order

1. Cut the hypothesis-testing wishlist items — done, this pass.
2. Write the eval tier for decomposition/correlation questions, before the aside rule, not after.
3. Build the aside rule against that eval tier.
4. Ship v1.
5. Scope MCP-server mode as its own initiative.

## See also

- `docs/LIGHTWEIGHT.md` — the four axes in full, with the real numbers behind them.
- `docs/DECISIONS.md`, 2026-09-13 — "depth, not reach," the identity this doc assumes.
- `docs/ROADMAP.md` — the fuller wish-list; this doc is what actually ships first.

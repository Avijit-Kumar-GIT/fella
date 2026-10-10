# Fella product

Fella is an opinionated analytics harness built around conversation. A person
can ask a general question without mounting a folder, then use the same
conversation to inspect and analyze local data when a question depends on it.
The workspace organizes sources and context; it is not the product's purpose.

## The analyst's loop

Fella should help a model work the way an analyst approaches unfamiliar data:

1. Inspect the available sources, structure, labels, quality, and relevant
   context.
2. Interpret and, where useful, decompose the question into smaller questions.
3. Investigate the data, refine the interpretation, and repeat inspection as
   needed.
4. Ask the user when an unresolved choice would materially change the result.
5. Compute with an appropriate read-only method, check the result, and explain
   the evidence, assumptions, and limits.

This is an adaptive, model-directed loop—not a required sequence of tool calls.
The model interprets human language and chooses how to investigate; Fella's
runtime provides the observations, execution tools, continuation, and scoped
checks that make that work useful and inspectable.

## Product commitments

- **Consistency:** preserve definitions, source scope, and relevant context
  across turns without treating old prose as new evidence.
- **Correctness:** make a useful, well-supported answer as often as possible.
  A valid query or a successful replay is evidence about execution, not proof
  that the interpretation was right.
- **Efficiency:** reduce irrelevant context and repeated work while spending
  enough reasoning and tool calls to answer well. Few calls are not a goal by
  themselves.
- **Read-only analysis:** the agent can inspect mounted data and calculate
  from it, but cannot edit, move, or delete workspace files. The user may
  explicitly edit the root `fella.md` context file in the app.
- **Local control:** files and workspace state stay on the user's computer.
  When a user asks a connected model a question, that provider receives the
  prompt and any context or tool results used to answer it.

## Current scope

The maintained desktop app is Electron with a Svelte UI and a local Rust
analytics sidecar. Ask supports model-knowledge answers without a workspace and
model-directed analysis of supported local files. SQL and sandboxed Python are
both first-class computation routes; forecasts and scenarios are allowed when
the available data and stated assumptions support them. Charts, document
search, clarification, conversation continuity, and inspectable analysis
details are part of the same conversation flow.

Web research is **not shipped** and remains on the backburner. Fella does not
silently send workspace contents to web search. The desktop app is not a
general-purpose file manager, shell automation tool, notebook, or multi-agent
orchestrator. A read-oriented analyst CLI, with FellaDB as its proposed local
filesystem search capability, is an exploratory direction and is not shipped.
See [Architecture](ARCHITECTURE.md) for the current implementation,
[Roadmap](ROADMAP.md) for open work, and [Stack vision](STACK-VISION.md) for
the proposed path from files to reusable project knowledge.

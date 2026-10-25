# Product vision

Fella is an AI-native workspace for analytical work with local information.
It should make asking a question feel simple while keeping the work behind a
consequential answer inspectable and reusable.

## Product idea

**The conversation is the interface, the agent performs the work, and the
analysis is the artifact.**

Fella helps people move from unfamiliar files to a useful answer: discover
relevant sources, interpret the question, compute with an appropriate method,
check the result, communicate uncertainty, and preserve the work. Small asks
should stay lightweight. A longer investigation should accumulate useful
context, Questions, evidence, results, and a report without forcing the user
through a rigid wizard.

The model decides what to investigate and how to present the supported result.
The Rust runtime owns data access, execution, verification, and persistence.
The user can steer meaning that would materially change the result.

## Product surfaces

- **Chat** is the default place to ask questions and continue a conversation.
- **Notebook** is an optional arrangement of Chat beside the active
  Investigation or another workspace tool in the same window.
- **Files** shows what the mounted folder contains and what Fella can analyze.
- **Reports** preserves selected analytical work in a reviewable, portable
  document.

A mounted folder defines data scope. The window's Chat/Notebook arrangement is
only presentation. These concepts must remain distinct.

## What a good experience feels like

A person can open a folder they do not understand, ask a question in ordinary
language, and receive an answer shaped for the work: concise explanation,
numbers, table, chart, or a useful combination. The content fits the window,
uses consistent components, and does not require a second click or reload to
become usable.

The person can inspect where a result came from, understand what a check did
and did not establish, follow up in context, and save useful results without
recomputing them. If a rich presentation cannot be produced, the supported
answer and evidence still remain available.

The experience should feel fluid, calm, and direct while preserving correctness,
transparency, and user control. It is not a chatbot with a chart template, a
traditional dashboard builder, or an unrestricted agent framework.

## Product principles

1. **Use the right source and method.** The model adapts its approach to the
   question and available information.
2. **Keep computation deterministic and bounded.** Models reason; the Rust
   engine mediates source access, executes analytical tools, and persists the
   trace.
3. **Bind claims to evidence.** Preserve source identity, revision, calculation,
   and checks so users can inspect the basis of an answer.
4. **Say what remains uncertain.** A completed tool call or passing check is
   evidence only for what it tested.
5. **Keep the interface simple by default.** Reveal method and controls when
   useful; do not make ordinary questions feel like project management.
6. **Preserve useful work.** Saved results and reports should be reusable and
   portable without unnecessary recomputation.
7. **Respect local control.** Users choose their model provider and folder
   scope. Workspace data is not silently sent to web search.

For actual shipped capabilities and limitations, see [Product](./PRODUCT.md).
For the object and retention rules, see [Product model](./PRODUCT-MODEL.md).

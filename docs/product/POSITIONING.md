# Fella positioning

## Category

Fella is an AI analytics workspace for people working with local files. It
helps users explore unfamiliar information, answer analytical questions, and
preserve the results with their source and evidence context.

The product is built around a model-directed analytics harness. The model
chooses what to inspect and how to explain results; the local Rust runtime
controls source access, executes bounded analysis, checks outputs, and retains
the analytical record.

## Audience

Fella serves analysts and other people who need to answer questions from
spreadsheets, exports, documents, and other files without first building a
warehouse or dashboard. The interface should be approachable to a nontechnical
person while keeping methods and evidence available for professional review.

## Product promise

A user can open a folder, ask a question in ordinary language, and get a
contextual answer with the appropriate mix of explanation, values, tables, and
visualizations. They can inspect the analysis, continue in a Conversation,
organize longer work as an Investigation, and save results in a report.

The product model and its boundaries are described in the
[product contract](./PRODUCT.md) and [architecture](../architecture/ARCHITECTURE.md).

## What makes the product distinct

- **Explicit local scope:** the user chooses the mounted folder that can be
  analyzed; Fella does not silently search the rest of the computer.
- **Flexible analysis:** the model adapts its approach to the question instead
  of forcing every answer into one chart or workflow template.
- **A controlled runtime:** read-only SQL, sandboxed Python, source revisions,
  evidence, and bounded tool results make analytical work inspectable.
- **Persistent work:** conversations, Investigations, saved results, and
  Reports connect follow-up work to its sources and provenance.
- **Useful by default:** Chat is lightweight, while Notebook and Investigation
  details are available when a person needs a more structured view.

## Claim boundaries

- Do not claim Fella is the first or only AI product for file analysis.
- Do not describe Fella as fully local inference. A local model endpoint is an
  option, but hosted providers receive prompts and relevant context or tool
  results selected for analysis.
- Do not imply that General chat searches the whole computer or the web.
- Do not claim an answer is correct solely because a query executed or a
  deterministic check passed. Explain what evidence and checks establish.
- Do not describe Fella as a dashboard builder, general-purpose terminal, or
  agent with unrestricted computer access.

# Fella positioning

This brief captures Fella's product story for investor, partner, and
public-facing conversations. It is a narrative guide, not a market-size claim or a
substitute for the implementation boundaries in the product and architecture
docs.

## Core position

Fella is building an AI analyst for the data that never makes it into the data
warehouse: exports, spreadsheets, logs, and documents people still need to
understand.

The product category is **file-native analytics**. “Analytics harness” is
useful technical shorthand for the runtime underneath; the user-facing story
is an analyst that can investigate ordinary files and show its work.

## Investor narrative

> Fella turns a folder of everyday files into an analytical workspace. Ask a
> question in plain language, and a model directs the investigation while
> Fella's local analytics engine inspects sources, runs queries or bounded
> calculations, builds charts, and records the evidence behind its answer.
>
> The point is not to give an AI unrestricted control of a computer. Fella is
> built for analytical work: file access is scoped to a workspace, computation
> happens through constrained tools, and the answer can be traced back to its
> sources and calculations. It makes ad hoc data analysis conversational
> without making it a black box.
>
> The larger bet is that a growing share of useful analysis will begin outside
> polished dashboards and governed warehouses. Fella is building the
> analytical layer for that messier reality.

## Why this matters

Important questions often begin with data that has not been modeled for a BI
system: a one-off export, a set of spreadsheets, or a report beside the source
tables. The analyst must first discover what is present, understand its shape,
resolve ambiguous labels, choose a sound calculation, and explain the result.
That work is more than asking a model to summarize a file.

Fella treats that investigation as the product. The model interprets the
question and directs the work; Fella's runtime owns file scope, bounded
computation, evidence capture, and targeted verification. The differentiating
thesis is not access to a model, but an analytics-specific system for turning
messy files into inspectable answers.

## Product truth

Fella is an open-source desktop application with a Svelte interface and a
local Rust analytics engine. It can catalog selected folders, load supported
tables into a local query engine, search and read supported text documents,
run read-only SQL and bounded Python, create structured charts, and retain
analysis evidence and verification findings across a conversation.

“Local” describes the file and analytics runtime, not necessarily model
inference. When a hosted provider is selected, the question and relevant
context or file-derived tool results used to answer can be sent to that
provider. Fella does not upload the folder as a bulk file; provider handling is
governed by the provider the user chooses. See the
[privacy guide](site/developer-platform/using-fella/privacy.mdx).

The product is early. The current quality benchmark is still being curated,
and its coverage is not broad enough to support a claim of general analytical
leadership. See the [FolderQA benchmark](../bench/fqa-bench/README.md).

## The longer-term possibility

The current product is the analyst. Two separate directions could extend what
users can do with it:

- **Projects** are currently local, user-authored notes associated with a
  repository. The direction is to make them concise, evidence-backed briefs
  that curate useful findings and visualizations and link back to their source
  conversations.
- **FellaDB** is a proposed optional companion for broader personal-file
  search. It is not required by Fella and is not part of the shipped product.

Together, these could help users find relevant files and preserve useful
analysis without turning Fella into a dashboard builder or a general-purpose
computer-control agent. The
[roadmap](ROADMAP.md) and [FellaDB vision](FELLADB.md) describe the current
boundaries.

## Positioning boundaries

- Do not claim Fella is the first or only AI product for file analysis. The
  category has adjacent products and broader agent frameworks; Fella's claim is
  its specific local-file analytics focus and purpose-built analytical
  runtime.
- Do not describe Fella as fully local inference. A local model endpoint is an
  option, but hosted providers receive the model requests and selected context
  needed for analysis.
- Do not imply General searches the whole computer. Broad personal-file search
  belongs to the proposed FellaDB direction, with explicit user-selected scope.
- Do not present Projects as an automatically generated wiki today. They are
  currently user-authored local notes.
- Do not imply enterprise deployment, governance, or broad correctness has
  been established. The maintained product is an early desktop release.

## Adjacent market context

This is a positioning reference, not an exhaustive competitive analysis. As of
October 2026, adjacent examples include [Julius's file-analysis
workflow](https://www.juliuscontent.com/docs/get-started/quickstart), [Hex's
Notebook agent](https://learn.hex.tech/docs/explore-data/notebook-view/notebook-agent),
and [Microsoft Agent Framework's
harness](https://learn.microsoft.com/en-us/agent-framework/concepts/harness).
They support a broader landscape of AI-assisted analysis and agent tooling;
they do not establish that Fella is unique. Lead with the wedge, not an
absence-of-competition claim.

## The bet

AI can make analytics easier to ask for, but trust comes from the system that
scopes, computes, and shows its work. Fella is building that system for the
files people already have. The next proof point is demonstrating reliable
analysis across a broader, independently evaluated range of real-world files.

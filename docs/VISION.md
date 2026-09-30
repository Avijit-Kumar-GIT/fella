# Fella: The Analytical Computer

> The grand vision for Fella: a personal and enterprise-capable analytical
> runtime that makes messy data understandable, computable, and trustworthy.

This document is the north star, not a claim about the current release. The
current implementation should remain small and testable, but we have AI to
help us build toward the best solution rather than limiting the destination to
the first version we can ship.

## The vision in one sentence

Fella becomes a **personal analytical computer**: point it at a folder and it
builds a living, inspectable understanding of the data, accepts questions in
ordinary language, performs bounded analysis, explains uncertainty, verifies
the result, and becomes more consistent with that workspace over time.

The folder is the boundary. The analytical runtime is the product.

## What Fella should feel like

The ideal Fella experience is not opening a blank chatbot and hoping the model
understands a spreadsheet. It is closer to opening a capable analytical
instrument that already knows how to work with the data in front of it.

After mounting a folder, Fella should be able to say, in effect:

- these are the sources I found;
- these are the kinds of records and measures they appear to contain;
- these are the caveats that could change an answer;
- these are the definitions I know or still need you to confirm;
- ask me a question and I will show you what I used and how certain I am.

The user should be able to ask a simple question and receive a fast answer,
ask an ambiguous question and receive a useful clarification, or ask a complex
question and watch Fella decompose it into an understandable analytical path.

The product should feel opinionated because it has a clear view of what a good
analytical answer is: relevant, correctly scoped, computed from the data,
honest about uncertainty, and efficient to produce.

## The best possible Fella system

The grand system is one loop with several cooperating planes:

~~~text
                         Fella Runtime
                               |
      +------------------------+------------------------+
      |                        |                        |
 Workspace Plane         Semantic Plane           Analysis Plane
 files, sources,         meaning, definitions,     contracts, plans,
 profiles, revisions     entities, memory         probes, execution
      |                        |                        |
      +------------------------+------------------------+
                               |
                      Verification Plane
                 invariants, provenance, review
                               |
      +------------------------+------------------------+
      |                        |                        |
  Result Plane            Learning Plane          Experience Plane
 answers, charts,         corrections, evals,     Ask, history,
 evidence, status         verified traces         search, summaries
~~~

These planes are not separate products. They are different responsibilities of
one revision-aware runtime.

## 1. A universal workspace understanding layer

Fella should work on data that has not been cleaned, modeled, or documented by
a data team. Mounting a folder should trigger a progressive understanding
pipeline:

- discover and classify sources;
- infer types, dates, units, and likely entities;
- profile values, nulls, outliers, and near-duplicate labels;
- identify candidate measures, dimensions, and relationships;
- detect ingestion caveats and suspicious data quality;
- maintain a stable revision for every workspace state.

The result is not merely a schema dump. It is a `WorkspaceModel` that describes
what the data appears to mean and how certain each fact is.

## 2. A semantic layer without requiring data modeling first

Users should not need to author a dbt project before asking a question. Fella
should infer a useful semantic model automatically, then let the user refine
it when necessary.

The semantic layer should understand:

- entities such as transactions, workouts, accounts, or subscriptions;
- measures such as amount, duration, distance, or count;
- dimensions such as category, merchant, region, or status;
- time conventions, units, and sign conventions;
- aliases such as `rent` meaning several stored category values;
- preferred sources and exclusions;
- confirmed definitions and unresolved ambiguity.

Advanced users and organizations should be able to import or author stronger
definitions, but manual modeling should improve Fella rather than be required
to make it useful.

## 3. An adaptive reasoning and planning system

The model should not be forced through one rigid chain of thought. Fella should
choose the cheapest reliable strategy for the question:

~~~text
low risk       -> direct interpretation and execution
moderate risk  -> inspect likely fields, then execute with stated assumptions
high risk      -> compare candidates, probe, execute, and strengthen checks
material choice -> clarify when investigation cannot resolve a user preference
no evidence path -> explain what the data cannot establish
~~~

AI should be used broadly where it provides leverage:

- interpreting natural language;
- proposing semantic candidates;
- identifying likely ambiguity;
- selecting useful probes;
- choosing between SQL, Python, document analysis, and charts;
- explaining results in plain language;
- suggesting definitions for user confirmation.

The runtime should remain authoritative over scope, permissions, execution,
verification, and durable memory. The best solution is not one giant
autonomous model. It is a system that gives models freedom to reason while
constraining the consequences of being wrong.

## 4. Correctness as a first-class product capability

Fella should distinguish three different claims:

1. the query ran;
2. the query produced a reproducible result;
3. the result answers what the user meant.

The third is the defining challenge. Fella should verify analytical invariants
such as:

- grouped totals reconcile with the overall total;
- filters resolve to values that exist;
- date ranges are represented in the source;
- ratios use compatible populations;
- averages remain within plausible bounds;
- requested comparisons are all present;
- charts match their underlying results;
- the workspace did not change during the run.

Correctness must not become a reason to suppress useful work. A harness can
lose trust by refusing an answer the model could have supported with another
inspection or a reasonable, disclosed assumption. Keep hard stops for genuine
execution and access boundaries; semantic uncertainty should usually lead to
more investigation, a qualified or partial answer, or a comparison of plausible
interpretations. Ask for clarification when a material user-owned choice
remains, and say the data cannot answer only when no usable evidence path
remains. Verification should guide revision and communicate support, not act as
a blanket answer gate.

Measure answer coverage and unnecessary abstention alongside correctness,
calibration, clarification burden, and cost. A shadow evaluation can relax
semantic gates for blocked cases while preserving the same read-only sandbox,
revealing when harness policy—not missing evidence—prevented a correct answer.

## 5. Intelligence that compounds without becoming surveillance

Fella should get better at a workspace without turning it into a hosted user
profile or an opaque vector database.

It should remember:

- confirmed user definitions;
- recurring vocabulary and aliases;
- source caveats;
- trusted relationships;
- corrections and their scope;
- verified analytical patterns;
- unresolved questions worth revisiting.

Every memory item should carry its authority, evidence, workspace revision,
and supersession state. A model guess should not silently become a permanent
rule. The improvement loop should be inspectable, editable, and local.

The larger learning system should also use verified traces offline: benchmark
failures, successful corrections, wasted tool calls, and semantic mistakes can
improve prompts, routing, models, and planner policies without sending private
workspace data anywhere.

## 6. Longitudinal analysis, not just one-off answers

Once the runtime has stable workspace revisions and semantic contracts, Fella
can support analyses that remain useful over time:

- rerun a saved question against new data;
- explain what changed between two workspace revisions;
- track recurring metrics without building a dashboard first;
- surface unusual changes when the user asks for them;
- maintain a workspace brief of important findings and definitions;
- compare personal or departmental data across consistent scopes;
- revisit past answers and show exactly which assumptions changed.

These are not separate dashboard features. They are natural consequences of
having revision-bound analytical turns and verified results.

## 7. The developer and enterprise foundation

The runtime should expose a stable, typed boundary that works from the desktop
UI, a CLI, tests, and future integrations:

~~~text
open workspace
inspect workspace model
ask question
clarify interpretation
run or rerun analysis
inspect trace and verification
edit semantic definition
export a result or runtime record
~~~

Rust remains the authority for the data plane, safe execution, persistence,
and verification. The UI remains a projection of runtime state. Model
providers remain interchangeable inference adapters.

For enterprise use, organizations can provide stronger semantic definitions,
policies, and approved models without changing the core product. The same
runtime can serve a person analyzing a folder of bank statements and a team
analyzing a controlled repository of operational data.

## What Fella will deliberately not become

The grand vision is ambitious in analytical depth, not in general autonomy.
Fella should not become:

- a coding agent;
- a general-purpose computer-use agent;
- a dashboard platform that requires modeling before usefulness;
- a write-capable automation system;
- a hosted behavioral profile of the user;
- a plugin marketplace whose breadth weakens the trust boundary;
- a chat surface where fluent language substitutes for computation.

The strongest version of Fella is powerful because it is unusually deep at one
job, not because it tries to perform every job.

## The moat

The hard-to-copy advantage is the closed loop between semantic interpretation,
workspace grounding, deterministic computation, verification, and scoped
memory.

Any individual part can be copied:

- a SQL agent;
- a schema profiler;
- a memory file;
- a chart renderer;
- a natural-language interface.

The compounding advantage comes from how they work together. Every ambiguous
question, correction, verification failure, and successful analysis can improve
future interpretation while remaining tied to the actual data and its revision
history.

That is the grand Fella harness:

> **A private analytical computer that learns what your data means, computes
> what you ask, and shows what its answer rests on.**

## Success criteria

The vision is working when:

- a new folder becomes useful without manual data modeling;
- simple questions are fast;
- difficult questions become understandable rather than merely verbose;
- ambiguity is surfaced before it creates a materially wrong answer;
- correctness improves across repeated questions in the same workspace;
- every result can be tied to a workspace revision and execution trace;
- users can inspect and correct what Fella believes;
- the system remains lightweight, local-first, read-only, and model-agnostic.

## Related documents

- [`SEMANTIC-RUNTIME.md`](SEMANTIC-RUNTIME.md) — concrete runtime and harness architecture
- [`PRINCIPLES.md`](PRINCIPLES.md) — product commitments
- [`WHY.md`](WHY.md) — the case for structural restraint and personal software
- [`GOALS.md`](GOALS.md) — current release scope
- [`ROADMAP.md`](ROADMAP.md) — candidate work beyond the current release

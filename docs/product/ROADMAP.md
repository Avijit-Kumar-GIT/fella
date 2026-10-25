# Roadmap

This is a concise map of open product and runtime questions, not a release
schedule or a list of completed work. The issue tracker and versioned evaluation
suites carry slice-level acceptance criteria.

## Current focus

1. **Measure analytical quality broadly.** Extend evaluation across unfamiliar
   folders, domains, messy data, follow-up questions, charts, documents, and
   uncertainty. Report correctness, coverage, visualization suitability, false
   deferrals, and efficiency as separate outcomes. See
   [FQA-Bench](../../bench/fqa-bench/README.md) and
   [product evaluation](../../bench/product-eval/README.md).
2. **Improve generated answers.** Make the json-render presentation feel
   question-specific and consistently composed. It may use narrative, metrics,
   tables, charts, and controls when they help. Keep the verified answer and
   evidence available if layout generation or rendering fails.
3. **Make analytical changes reviewable.** Compare changes in question,
   definitions, source scope, method, values, findings, and dependent reports.
   Preserve prior revisions and make affected outputs visible without
   recomputing unrelated work.
4. **Keep the desktop easy to use at every size.** Continue improving the Chat,
   Notebook, Investigation, Files, and Reports hierarchy, shared components,
   keyboard access, and behavior in compact windows.
5. **Strengthen source freshness and large-workspace performance.** Make it
   easier to understand when saved analysis refers to an older catalog revision;
   measure refresh, memory, and responsiveness with representative multi-file
   workspaces and packaged builds.

## Deferred

Web research, unrestricted file-writing agents, and enterprise collaboration
are not part of the current product commitment. Revisit them only with a clear
user need, explicit boundaries, and an evaluation plan.

## How work enters the roadmap

Prefer general capabilities that help multiple kinds of analysis. Define
expected behavior and grading criteria before evaluating a change. Preserve
failures and report coverage limits. A passing unit test alone does not establish
that model-driven behavior works in the real application.

# Roadmap

This is a short map of open product and runtime questions, not a release
schedule or a list of completed backlog items. The repository's versioned
benchmarks and issues carry the detailed task contracts. Do not turn an
individual failed example into a special-case production rule.

## Current priorities

1. **Measure broad analytical quality.** Grow FQA-Bench beyond its initial
   draft suites with independent workspaces, whole-folder discovery, varied
   domains and messy formats, reviewed answer contracts, and a blind holdout.
   Report correctness, answer coverage, false deferrals, chart semantics,
   clarification quality, and efficiency separately. See
   [`bench/fqa-bench/methodology.md`](../bench/fqa-bench/methodology.md) and
   [`bench/product-eval/`](../bench/product-eval/).
2. **Improve general interpretation and normalization.** Let the model use
   inspected source profiles, observed values, prior user definitions, and
   focused clarification to resolve meaning. Preserve raw values and lineage;
   uncertain mappings remain explicit rather than being silently rewritten.
3. **Strengthen analysis and visualization across families.** Evaluate
   SQL/Python choice, joins, time series, forecasts, scenarios, and chart data
   and presentation as independent quality dimensions. Keep verifier findings
   scoped to the claim, evidence, or chart that they actually affect.
4. **Keep large-workspace work evidence-led.** The current ingestion path has
   been exercised on 5,000 small sources and a single 10-GiB CSV on Linux, but
   those are isolated diagnostics, not platform-wide guarantees. Incremental
   refresh, progressively queryable sources, multi-large-file coverage, and
   packaged Windows measurements remain open engineering questions.
5. **Keep the runtime understandable.** Improve the analysis-turn trace and
   replay experience only where evaluation or user workflows show a real gap;
   preserve one model-directed loop and a small, fixed, read-only tool surface.
6. **Make the Ask work area composable without making the app a canvas.** Keep
   navigation and the composer anchored; let users open a chart or source
   preview beside the active conversation. Expand to additional pane types or
   resizing only when the conversation remains primary and provenance stays
   clear.

## Deferred

- **Web research:** intentionally deferred. It needs an explicit privacy and
  network policy, source provenance, hostile-page handling, and its own
  evaluation route before it is enabled.
- **Multi-agent orchestration, file-writing tools, and enterprise governance:**
  not planned for the personal release without measured user need and a clear
  security and product design.

## How work enters the roadmap

Prioritize general capabilities that improve more than one task family. Define
expected behavior and grading criteria before running a candidate; preserve
failures for review. Change a benchmark contract only when its intended
behavior is independently adjudicated, version the change, and report old and
new results separately. A capability is not considered complete merely
because a unit test passes: verify it in the real harness when the claim is
about model-driven behavior.

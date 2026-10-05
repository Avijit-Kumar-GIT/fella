# Visualization benchmark

This battery exercises the personal analytics visualization path end to end:

1. the model chooses `run_sql`, `run_python`, or `forecast_analysis` and `make_chart`;
2. the engine derives chart values from the selected typed result instead of trusting model-supplied marks;
3. chart data is validated for readable size, finite numbers, and meaningful variation;
4. the answer keeps a short prose takeaway beside the typed visual evidence.

The suite covers category breakdowns, time trends, negative values, two-series
comparisons, CSV/TSV/JSON inputs, currency-formatted text, joins, and cases where a
chart should be skipped or rejected. The 2026-10-04 expansion adds model tasks for
donut, scatter, histogram, box plot, area, stacked area, heatmap with a missing cell,
precomputed forecast bands, the forecast-tool-to-chart path, and a Python-generated
scenario chart. Specialized chart golds compare typed points, quartiles, matrix cells,
and forecast arrays, not just the existence of a visualization. Source-sensitive cases
also assert that the chart reused the Python or forecast result requested by the task.
Expected outcomes are fixture-derived and fixed before model evaluation.
The expanded 25-case task file is frozen at SHA-256
`cf92d192a17f9901ad5ac1205413a2b89014a9733a9f641dd8cb79dae2dd56b8`.

Category charts cap at 12 labels for readability; time-series line charts can carry
up to 1,000 points, with longer ranges expected to use a coarser time period or a
narrower date range.

The hosted cases describe user-visible outcomes and do not assert a particular internal
row-cap implementation. Capacity boundaries are covered separately by deterministic
tests in `src-tauri/tests/chart_tool.rs`.

The hosted run is opt in and BYOK only. It defaults to the hosted Ollama provider
and Gemma 4 31B; it never starts or calls a model server on the machine.

From the repository root, copy a Fella data directory containing `fella.db` and
the provider credential to a scratch location, then run:

```bash
AGENT_EVAL_DATA_DIR=/path/to/copied-data-dir \
  ./scripts/test-visualizations.sh
```

The model and iteration count are configurable. A focused/full model run can be
started with:

```bash
FELLA_EVAL_MODEL=openai/gpt-5.6-luna FELLA_EVAL_ITERS=1 \
  AGENT_EVAL_DATA_DIR=/path/to/copied-data-dir \
  ./scripts/test-visualizations.sh --json /tmp/fella-visualization.json
```

Useful options are passed through to `agent_eval`:

```bash
FELLA_EVAL_ITERS=3 EVAL_SHOW_ANSWERS=1 \
  AGENT_EVAL_DATA_DIR=/path/to/copied-data-dir \
  ./scripts/test-visualizations.sh --only monthly --json /tmp/visualization.json
```

The deterministic checks do not need a provider. They live in `chart.rs`,
`tests/chart_tool.rs`, and the agent-loop integration tests. `pnpm test:chart-renderer`
server-renders every supported chart kind and checks rendered marks, accessible names,
exact-value tables, theme-token contrast, and responsive overflow rules. This is a
structural renderer check, not a substitute for human review in the desktop webview.
The benchmark cases require a real BYOK model because they measure
question-to-answer behavior. Record model/provider, iteration count, task-set SHA-256,
answer/chart gold, runtime status, failed tool calls, and token/time cost in every live
report. Do not edit tasks or gold after observing output; preserve failures and review
any proposed task-set revision separately.

The latest recorded OpenAI direct BYOK run and post-run diagnostics are in
[`backlog-5.md`](../product-eval/reports/backlog-5.md). The untouched 25-case run scored
20/25; later runs were focused diagnostics, not a combined post-fix score. Three
benchmark outcomes remain flagged for review, and no gold was changed.

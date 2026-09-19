# Visualization benchmark

This battery exercises the personal analytics visualization path end to end:

1. the model chooses `run_sql` and/or `make_chart`;
2. the engine derives chart values from SQL instead of trusting model-supplied values;
3. chart data is validated for readable size, finite numbers, and meaningful variation;
4. the answer keeps a short prose takeaway beside the typed visual evidence.

It covers category breakdowns, time trends, negative values, two-series comparisons,
CSV/TSV/JSON inputs, currency-formatted text, joins, and cases where a chart should
be skipped or rejected. Category charts cap at 12 labels for readability; time-series
line charts can carry up to 1,000 points, with longer ranges expected to use a coarser
time period or a narrower date range.

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

Useful options are passed through to `agent_eval`:

```bash
FELLA_EVAL_ITERS=3 EVAL_SHOW_ANSWERS=1 \
  AGENT_EVAL_DATA_DIR=/path/to/copied-data-dir \
  ./scripts/test-visualizations.sh --only monthly --json /tmp/visualization.json
```

The deterministic checks do not need a provider. They live in `chart.rs`,
`tests/chart_tool.rs`, and the agent-loop integration tests. The benchmark cases
require a real BYOK model because they measure question-to-answer behavior.

# Fella FolderQA Benchmark (FQA-Bench)

FQA-Bench is the versioned benchmark for Fella's filesystem analytics harness. Its target is the complete product described by the product roadmap, not only the features currently shipped. It asks whether the model, tools, ingestion, conversation state, and presentation work together to inspect a folder, understand an analytical request, resolve uncertainty, compute and verify results, and communicate useful evidence.

**Version:** FQA-Bench v0.1 (first unreleased target-state specification). The active task contract assumes the roadmap is complete. A task can therefore exercise clarification/resume, whole-folder discovery, forecasting, scenarios, richer charts, or persistent workspace context even while its implementation remains in progress. A current-build run must be labeled as a capability diagnostic, not treated as the benchmark definition.

FQA-Bench covers questions whose answer depends on a mounted local workspace. General knowledge and web research are separate product routes; hybrid tasks that join local evidence with research are reported separately and must never send local file contents to the web. See the product evaluation map.

## Benchmark model

The unit is an **episode**, not a single prompt: a frozen workspace plus one or more user and assistant turns. Episodes include direct analysis, broad inspection, multi-source reasoning, clarification followed by a resolved answer, follow-ups, forecasts/scenarios, chart requests, and evidence-limited questions. Each episode has one primary capability and may carry multiple analytical-family, topic, format, quality, and interaction tags.

Golds describe intended user-visible outcomes: values and dimensions, acceptable methods/ranges, needed assumptions, useful clarification, source limits, provenance, and chart semantics. They do not prescribe SQL, Python, tool order, internal plans, or a particular wording. An efficient alternative method is correct when it answers the same question from the declared evidence.

Forecasts and scenarios are valid analysis tasks, not an automatic refusal category. Unsupported evidence is also not a cold-refusal category: grade whether Fella identifies the actual missing evidence, gives any useful supported partial result, and offers a concrete way forward. Clarification is a successful intermediate action when the ambiguity is material; the episode must then test whether Fella uses the user's answer.

## Files and ownership

    bench/fqa-bench/
      README.md
      methodology.md
      taxonomy.yml
      coverage-v0.1.yml             # target capability/family coverage and gaps
      task.schema.json
      answer-key.schema.json
      audit-v0.1.md                 # first-version scope, coverage, and limitations
      suites/<suite-id>/
        README.md                   # source, license, transformations, limits
        tasks.jsonl                 # prompts and labels; safe to expose to the model
        answer-keys.jsonl           # evaluator-only; never mounted
        cases.jsonl                 # generated adapter for the current runner
        workspaces/<workspace-id>/  # exact files visible to the model
      runs/                         # run metadata and outputs; never fixture data

The answer key and source archives stay outside every mounted workspace. Public development tasks may be visible, but a public task set is not a blind evaluation set. Holdouts must be drawn from different workspace/source/generator families and stored outside routine development access.

Current runner compatibility:

    python3 bench/fqa-bench/build_runner_cases.py \
      --suite-dir bench/fqa-bench/suites/uci-bike-sharing --check
    python3 bench/fqa-bench/build_runner_cases.py \
      --suite-dir bench/fqa-bench/suites/clarification-housing --check
    python3 bench/fqa-bench/suites/clarification-housing/validate_suite.py
    python3 bench/fqa-bench/test_report_slices.py

To run a suite against Fella, first make an isolated copy of the app data
directory (including `fella.db` and `auth.json`) and point the evaluator at
that copy. Set the provider/model explicitly; each suite is a separate run:

    AGENT_EVAL_DATA_DIR=/path/to/isolated-fella-data \
    cargo run --release --manifest-path src-tauri/Cargo.toml \
      --features eval --example agent_eval -- bench \
      --dir bench/fqa-bench/suites/uci-bike-sharing \
      --models provider/model --iters 3 --json /tmp/fqa-uci.json

    AGENT_EVAL_DATA_DIR=/path/to/isolated-fella-data \
    cargo run --release --manifest-path src-tauri/Cargo.toml \
      --features eval --example agent_eval -- bench \
      --dir bench/fqa-bench/suites/clarification-housing \
      --models provider/model --iters 3 --json /tmp/fqa-housing.json

The clarification episode is Fella-only because baseline adapters do not
replay and grade the intermediate clarification turn. Preserve all task
failures in the report. Never edit a gold after seeing candidate output; a
corrected task needs an approved rationale, a new benchmark version, and a
comparable rerun.

The UCI Bike Sharing suite is an independently sourced, clean public-data anchor with 13 episodes, including descriptive comparisons, a chart, a held-out mean-baseline forecast, a what-if scenario, follow-up context, and specific evidence-limit cases. It is one small domain anchor, not a representative v0.1 benchmark by itself. The generated personal-data batteries and older component suites remain useful development diagnostics; they do not become authoritative simply by being numerous. Their earlier run artifacts are preserved as historical records and are not silently rescored under v0.1.

## Evaluation and reporting

Follow the methodology and target coverage matrix in coverage-v0.1.yml. Report correctness, clarification quality, false deferral, unsupported-claim rate, evidence/provenance, chart correctness, forecast error/calibration, conversation consistency, operational validity, and efficiency separately. Show counts and denominators for every slice. Do not collapse a serious weakness into a composite score.

For completed agent_eval JSON runs, join results to task tags:

    python3 bench/fqa-bench/report_slices.py \
      --tasks bench/fqa-bench/suites/uci-bike-sharing/tasks.jsonl \
      --results /path/to/fella-run.json

The slice report supports topic, product capability, analysis family,
interaction, answerability, workspace scope, file format, and data condition.
It reports task correctness and efficiency; qualitative dimensions such as
clarification necessity, evidence quality, unsupported claims, and chart
semantics still require the separate rubric/review process described in the
methodology. For the separate top-level product-family view and paired
baseline/candidate protocol, use
[`bench/product-eval/scorecard.py`](../product-eval/scorecard.py) with this
suite's `tasks.jsonl` and the saved `agent_eval` result JSON.

Invalid setup/provider/evaluator runs are reported separately, never counted as model failures or quietly excluded. A task or grading change requires user-visible rationale, a new benchmark version, and same-version comparisons; candidate output is never a reason to revise a gold.

## What v0.1 can and cannot claim

The v0.1 specification is designed around the complete product backlog, but the corpus is still being curated. The current independently sourced anchor is narrow, and the legacy synthetic profile is not independent real-world evidence. coverage-v0.1.yml distinguishes target coverage from fixtures that exist today. Until the required topic/workspace breadth, episode grading, independent review, and blind holdout exist, publish task-level results and gaps—not a claim that FQA-Bench is representative or that Fella is best-in-class.

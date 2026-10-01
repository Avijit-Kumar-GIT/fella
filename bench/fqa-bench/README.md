# Fella FolderQA Benchmark (FQA-Bench)

FQA-Bench is Fella's internal, versioned evaluation standard for analytics over mounted folders. It measures whether the **product harness** helps a model inspect files, interpret a request, analyze data, and communicate a grounded result—not whether it can match a particular sentence or reproduce one SQL plan.

**Status: v0.1 curation in progress.** The existing `folder-qa` battery remains a useful development baseline. A first independent public-data workspace is now curated, but the benchmark is not yet representative or release-grade. See [the data audit](audit-v0.1.md) before interpreting scores.

## Where benchmark assets live

```text
bench/fqa-bench/
  README.md
  methodology.md
  taxonomy.yml
  task.schema.json
  answer-key.schema.json
  audit-v0.1.md
  suites/<suite-id>/
    README.md                       # provenance, coverage, limits
    tasks.jsonl                     # canonical prompts and tags
    answer-keys.jsonl               # evaluator-only; never mounted
    cases.jsonl                     # generated compatibility adapter
    workspaces/<workspace-id>/...   # the only files exposed to Fella
  runs/                             # local/CI outputs, not fixture data
```

The existing runner consumes `<suite>/cases.jsonl`; the canonical task and evaluator answer key are separate, then adapted by [`build_runner_cases.py`](build_runner_cases.py). Answer keys and source archives stay outside every mounted workspace. Blind holdouts must live outside this public repository (for example, in a private repository or controlled CI storage).

The current legacy core is indexed at [`catalog/folder-qa.jsonl`](catalog/folder-qa.jsonl). Its broad capability labels are a **provisional crosswalk from legacy task tiers**, not independently reviewed annotations. Regenerate/check that sidecar with:

```bash
python3 bench/fqa-bench/catalog_folder_qa.py --write
python3 bench/fqa-bench/catalog_folder_qa.py --check
```

The first independently sourced suite, [`suites/uci-bike-sharing/`](suites/uci-bike-sharing/), provides 11 draft episodes over a complete, compact workspace. Rebuild/check its pinned public-data fixture and runner adapter with:

```bash
python3 bench/fqa-bench/suites/uci-bike-sharing/prepare_workspace.py --check
python3 bench/fqa-bench/build_runner_cases.py --suite-dir bench/fqa-bench/suites/uci-bike-sharing --check
```

All 11 episodes expose every workspace file. The suite fits the current runner's 100 KB text-comparison limit. It covers clean public mobility data only; it does not close the personal-data messiness or clarification gaps.

## Evaluation axes

Each task gets one primary analyst capability and may have several supporting capabilities. Domain, format, messiness, interaction shape, and answerability are cross-cutting tags—not substitutes for the capability being measured. See [taxonomy.yml](taxonomy.yml) and [methodology.md](methodology.md).

The intended scorecard keeps answer correctness, appropriate clarification, unsupported confident claims, unnecessary deferrals, chart correctness, and operating cost separate. The task metadata supports capability/domain slices; the current runner does not yet produce the complete tagged scorecard automatically. Do not collapse these into a single headline that can hide a serious weakness.

For completed `agent_eval --json` runs, join results to task tags and print capability/domain/interaction/format slices with:

```bash
python3 bench/fqa-bench/report_slices.py \
  --tasks bench/fqa-bench/suites/uci-bike-sharing/tasks.jsonl \
  --results /path/to/fella-run.json
```

Use `--tasks bench/fqa-bench/catalog/folder-qa.jsonl` for the provisional legacy crosswalk. Invalid runs are counted separately and excluded from correctness rates; missing task IDs remain visible as a coverage gap.

## Existing batteries and their role

| Existing path | Role in the eventual benchmark | Limitation |
| --- | --- | --- |
| `bench/folder-qa/` | Broad personal-data development baseline | One generated synthetic profile; per-case file staging; no clarification or multi-turn tasks |
| `bench/folder-qa-hard/` | Harder calculation/combination diagnostic | Reuses the same underlying generated profile; not an independent domain sample |
| `bench/fqa-bench/suites/uci-bike-sharing/` | First independent public-data anchor; inspection, aggregation, follow-up, reconciliation, chart, and unsupported-measure/detail tasks | One clean urban-mobility source; 11 draft episodes; no messy-data or clarification coverage |
| `bench/messiness/` | Data-quality diagnostic | Nine cases, overwhelmingly spending data; not a broad messy-data sample |
| `bench/step-judgment/`, `bench/policy-pressure/` | Follow-up and pressure diagnostics | Separate small batteries; not yet a consistent task schema |
| `bench/visualization/` | Chart behavior diagnostic | Separate battery; current cases lack standard domain/format tags |
| `bench/tool-selection/`, `bench/self-verification/`, `bench/injection/`, `bench/scale/`, memory suites | Component/robustness diagnostics | Keep separate from end-to-end FQA task accuracy unless their protocol and graders are normalized |

This inventory is intentionally conservative: a test suite does not become representative merely because it has many cases. FQA-Bench v0.1 needs additional independently designed workspaces, full-folder discovery tasks, and explicit clarification episodes before it can support broad capability claims.

## Adding a task

1. Write the user goal and task episode before consulting the implementation.
2. Build or select the workspace fixture independently of the harness code.
3. Have a second reviewer solve it from the visible files and verify the analytical contract.
4. Tag its capability, topic, formats, data conditions, interaction type, and expected behavior using the controlled vocabulary.
5. Put the answer contract outside the mounted workspace and validate the fixture, paths, and grader.
6. Keep development and blind evaluation splits grouped by workspace/source, not by individual question.

Do not add a task solely to encode a discovered code path. A failure may become a development regression case, but it should enter the held-out benchmark only if it represents a reusable analyst capability and passes independent task review.

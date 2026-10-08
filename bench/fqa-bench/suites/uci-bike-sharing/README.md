# Public-data anchor: UCI Bike Sharing

This is the first independently sourced FQA-Bench workspace. It is a **clean public-data anchor**, not a proxy for messy personal folders and not evidence that Fella is ready for general use. The tasks are authored against a frozen workspace and are grouped by analyst capability in `tasks.jsonl`; `answer-keys.jsonl` is evaluator-only. `cases.jsonl` is a generated adapter for the current `agent_eval bench --dir` runner.

## Provenance and license

- Dataset: [Bike Sharing](https://archive.ics.uci.edu/dataset/275/bike+sharing+dataset), UCI Machine Learning Repository, DOI [10.24432/C5W894](https://doi.org/10.24432/C5W894).
- Snapshot retrieved: 2026-10-01.
- Original dataset authors: Hadi Fanaee-T and João Gama; data covers Capital Bikeshare in Washington, D.C., 2011–2012.
- License: Creative Commons Attribution 4.0 International (CC BY 4.0), as stated by [UCI](https://archive.ics.uci.edu/dataset/275/bike+sharing+dataset).
- The exact source archive retrieved for this snapshot is preserved at `source/uci-bike-sharing.zip`; SHA-256: `b70182d0d0508e9abbb79306ce5c0cec34869000f8220175ac83d11dbe845401`.
- Dataset citation: Fanaee-T, H. (2013). *Bike Sharing* [Dataset]. UCI Machine Learning Repository. https://doi.org/10.24432/C5W894.
- Paper citation: Fanaee-T, H. and Gama, J. (2013). “Event labeling combining ensemble detectors and background knowledge.” *Progress in Artificial Intelligence*. https://doi.org/10.1007/s13748-013-0040-3.

The source archive is included under CC BY 4.0 with attribution. Fella's benchmark prompts, scripts, schemas, and task annotations remain separately governed by the repository license.

## Workspace construction

`prepare_workspace.py` verifies the archive hash, copies the complete UCI daily table, and derives a compact monthly rollup from the complete hourly table. The rollup groups all hourly records by the original `yr` and `mnth` fields and sums total, casual, and registered rentals; it also records the number of hourly rows in each group. No task-specific row selection or random sampling is used.

The mounted workspace consists of exactly three files:

- `daily.csv`: all 731 original daily records.
- `monthly-usage-from-hourly.csv`: all 24 year-month groups derived from the original hourly file.
- `field-guide.md`: grain, field meanings, code mappings, and the derivation boundary.

The full raw hourly table is deliberately not mounted: it is larger than the current comparison runner's 100 KB text-embedding limit. The compact rollup preserves a genuine second grain/source for reconciliation while allowing the same workspace to be run through Fella, a bare-model baseline, and the current OpenAI code-interpreter comparison. It does **not** support questions about individual hours.

Every task exposes all three workspace files. That makes source discovery part of the episode instead of handing the runner a per-question file subset. The answer keys and source archive are outside the mounted workspace.

## Rebuild and validate

```bash
python3 bench/fqa-bench/suites/uci-bike-sharing/prepare_workspace.py --check
python3 bench/fqa-bench/build_runner_cases.py --suite-dir bench/fqa-bench/suites/uci-bike-sharing --check
python3 bench/fqa-bench/build_runner_cases.py --suite-dir bench/fqa-bench/suites/clarification-housing --check
python3 bench/fqa-bench/suites/uci-bike-sharing/validate_suite.py
python3 bench/fqa-bench/suites/clarification-housing/validate_suite.py
```

To regenerate a case file after intentionally editing task or answer-key metadata, replace `--check` with `--write`. `--check` must pass before a benchmark run is valid.

`validate_suite.py` independently recomputes the answerable task contracts from the mounted data and checks that the legacy runner adapter exposes the entire inventory. These are benchmark oracles only; they are not imported by the Fella engine.

## Coverage and limits

This suite currently exercises file inspection, year-code interpretation, filtering and aggregation, a follow-up, cross-grain reconciliation, an exact monthly chart, two evidence-limit requests (missing price and missing hour-level detail), a held-out mean-baseline forecast, and an explicit what-if scenario. Evidence-limit answers should name the missing information and a useful next step, not stop at a cold refusal. The forecast prompt fixes the data cutoff and baseline method; the scenario specifies its assumption. These examples test basic outcome handling, not forecast calibration or general predictive quality. The suite is deliberately not a “messy data” test: the source has no missing values and the curated workspace is small. FQA-Bench still needs independently designed personal-data workspaces with naturalistic messiness and additional unrelated public-data domains before any broad claim is justified. A separate housing fixture exercises clarification and resolution.

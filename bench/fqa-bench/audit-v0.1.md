# FQA-Bench v0.1 scope and curation audit

**Status:** target-state specification; corpus construction and interaction grading are incomplete. This is the first unreleased FQA-Bench version. It assumes the complete product roadmap is implemented; it does not assert that today's application already supports every measured capability.

## Decision and intended claim

FQA-Bench v0.1 is a multi-topical benchmark for model-driven analysis of local filesystems. Its product-level question is whether Fella helps a capable model perform analyst work well—not whether a hand-authored rule or one internal execution route wins.

The target episode follows a real analytical workflow:

1. Inspect the mounted workspace and identify relevant sources, fields, grain, quality issues, and limitations.
2. Interpret and, where useful, decompose the request against those sources.
3. Ask a concise clarification when different reasonable interpretations materially change the answer; otherwise state a reasonable assumption and proceed.
4. Analyze with an appropriate read-only method, verify calculations and source alignment, and continue from new evidence or user input.
5. Present the answer with usable context, provenance, uncertainty, and a visualization when it helps.

The benchmark includes descriptive and diagnostic analytics, comparisons, joins, statistical analysis, time-series forecasts, scenarios, anomaly/data-quality work, document synthesis, and visual analytics. Forecasts are not presumed invalid. Missing evidence should result in a specific explanation, any useful supported partial answer, and a next step—not a blanket refusal.

## Current assets and their proper use

- The 13-episode UCI Bike Sharing fixture is an independent, clean, public-data anchor spanning inspection, aggregation, comparison, reconciliation, a chart, follow-up, evidence limits, a held-out baseline forecast, and an explicit scenario. It covers one small domain and is not independently reviewed by multiple annotators yet.
- The generated folder-qa profile has broad everyday topics, multiple formats, and common analytical shapes, but all tasks share one synthetic persona and generally stage case-selected files. It remains a visible development diagnostic, not a representative whole-folder benchmark or the v0.1 score by itself.
- FolderQA-hard, messiness, visualization, tool-selection, self-verification, injection, scale, memory, policy-pressure, and step-judgment are component/regression batteries with varying graders and protocols. Keep their results separate until each is reviewed and adapted to the v0.1 contract.
- Prior run outputs are historical. Their task IDs, outputs, and scores must not be rewritten to resemble results on v0.1. Runs against retired forecast-refusal golds remain labeled as policy/task mismatch, not as v0.1 forecast failures or successes.

## Required corpus before a representative v0.1 claim

The coverage matrix in coverage-v0.1.yml is the target, not a claim that every cell already has a reviewed fixture. Before claiming representative coverage, FQA-Bench needs:

1. Multiple independent workspaces per major topic, with different naming conventions, folder structures, formats, data grains, and combinations of clean and messy inputs.
2. Entire-workspace discovery episodes with nested folders and relevant distractors, not only a preselected list of files.
3. Data normalization cases for aliases, dates, units, missing and malformed values, duplicate/summary rows, conflicting versions, and cross-file keys. Each gold must specify the evidence-based interpretation and acceptable ambiguity handling.
4. Analytic-family coverage from direct lookups through group comparisons, joins, robust statistics, time-series, forecasts, scenarios, and anomaly/quality questions.
5. Chart episodes whose contracts independently score source rows, transformations, marks/type, encodings, labels, scale, ordering, and uncertainty where relevant. A chart is not correct merely because its prose is plausible.
6. Multi-turn clarification/resume and follow-up episodes that score each consequential assistant turn and whether the final result honors the clarification.
7. Evidence/provenance and read-only/security cases that test helpfulness as well as boundaries. A safety pass cannot conceal an avoidable refusal.
8. Independent task review, generator/source-family-separated splits, and a blind holdout unavailable during ordinary development.

The Fella runner adapter grades an explicitly declared clarification turn and
the resolved continuation in the same conversation. A protocol fixture now
exercises that path, but one draft example is not enough to establish broad
clarification quality. Third-party and bare-model runners do not yet replay
this interaction contract, so those comparisons must be marked unsupported
rather than scored as equivalent episodes.

## Benchmark integrity

- Define intent, scope, ambiguity, answer contract, and grader before candidate runs.
- Derive numeric or chart oracles from the fixture using an independently reviewable method. Record units, filters, denominator, grain, tolerance, forecast method/horizon, and allowed alternatives.
- Do not require a particular SQL/Python plan, prompt phrase, tool ordering, or case-specific production behavior.
- Preserve failed/ambiguous grades and candidate outputs. A suspected grader problem is a separate adjudication item; gold changes require approval, a recorded rationale, a new version, and comparable reruns.
- Report task counts, workspace/source counts, validity, and denominators alongside results. Do not generalize from a tiny slice or publish a single score that hides false deferrals, unsupported claims, poor charts, or unstable answers.

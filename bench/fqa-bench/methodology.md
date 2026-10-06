# FQA-Bench methodology

**Version:** FQA-Bench v0.1, first unreleased target-state methodology

**Purpose:** evaluate the quality of Fella as a model-driven filesystem analytics harness.

## 1. Claim and scope

FQA-Bench asks whether Fella enables a model to carry out useful analysis over a user's mounted folder. The target workflow is iterative:

1. Inspect the available files and learn what they contain.
2. Interpret and decompose the user's question against that evidence.
3. Analyze the data, verify the result, and communicate it clearly.
4. Ask a focused clarification when a material ambiguity prevents a defensible answer, then continue with the user's response.

The primary subject is the complete model-plus-harness system. A bare-model condition is a useful paired ablation, but it is not the product benchmark by itself. UI rendering, installer behavior, and operating-system compatibility have separate test layers and must not be mixed into the analytics score.

FQA-Bench is specifically about questions whose answer depends on a mounted
filesystem workspace. It does not score general-knowledge questions that need
no files or web-research quality. A forecast or chart over mounted data may be
an FQA episode, but forecast quality and chart correctness should also be
reported as their own capability slices. The cross-route product evaluation
map—including separate general-knowledge, web-research, and hybrid families—is
in [`../product-eval/README.md`](../product-eval/README.md). A `general` domain
tag in the FQA taxonomy means the topic of a workspace, not a model-only
general-knowledge route.

FQA-Bench v0.1 is the first unreleased target-state benchmark for the intended
complete Fella product. It assumes the product roadmap's local inspection,
clarification/resume, analysis, forecasting, visualization, provenance, and
context capabilities are implemented; it is not a list of features the
current build already passes. General knowledge, web research, and hybrid
local-plus-web quality remain separately scored in the product evaluation
map, with local data kept out of web requests.

The earlier pre-release task draft included blanket forecast-refusal golds
and implementation-route assertions. Those expectations are retired from the
active v0.1 task set. Their run artifacts remain historical; they are not
silently rescored or compared as though the task set were unchanged.

## 2. Benchmark unit and corpus

The primary unit is an **analysis episode**: a frozen workspace snapshot plus one or more natural-language turns. Episodes may be single-turn questions, follow-ups, clarification-and-resolution sequences, or multi-part analyses. An episode may exercise several capabilities, but has one primary capability for diagnostic reporting.

Build the corpus from explicitly labeled sources:

- **Synthetic controlled workspaces:** appropriate for personal data that cannot ethically or practically be sourced from strangers, and for exact ground truth. Use multiple independently designed profiles and generators; do not treat one seeded persona as a population.
- **Public datasets:** use only when source, license, snapshot date, and transformations are recorded. Public data adds real-world schema/content variation, but public benchmark prompts may be known to models.
- **User-contributed examples:** only with explicit permission, data minimization, and a privacy-safe transformed fixture. This is a future source, not a prerequisite.

For each source, record provenance, license/permission, generation or transformation recipe, workspace hash, and which facts are synthetic. Do not place real credentials, private personal files, or unreviewed user data in the repository.

The corpus should cross capability families with topical domains, file formats, and data conditions. Use a stratified design instead of making every possible combination. A task can be multi-label; report intersections only when their sample size supports a meaningful conclusion.

## 3. Task authoring and quality control

For every episode:

1. An author defines the user intent and realistic wording without reading Fella's implementation or failure-specific code.
2. The fixture is assembled from that intent. The user-facing prompt must not reveal the answer key or require knowledge absent from the mounted files.
3. An independent reviewer inspects the exact workspace, independently derives or verifies the expected result, checks ambiguity/answerability, and reviews the grading contract.
4. Disagreements are resolved before the task is admitted. Record alternate valid interpretations and acceptable clarifications rather than forcing a single phrasing.
5. Verify that the fixture loads, the model sees only its declared workspace files, and the grader accepts an independently produced correct answer while rejecting plausible wrong ones.

Machine-check numeric/table/chart values wherever possible. Use human review for semantic interpretation, evidence sufficiency, and response usefulness. If using a model grader for those judgments, calibrate it against human-labeled examples and retain the disagreements for review.

## 4. Splits and protection from overfitting

- **Development:** visible tasks, answer keys, and fixtures; used for diagnosis and iteration.
- **Blind evaluation:** tasks and answer keys not present in the public repository or routine development workspace. The runner may access them in a controlled evaluation environment.
- **Anchor set:** a small, versioned set retained across releases for longitudinal comparison; it is not a substitute for the blind set.

Split by workspace, source dataset, and generator family—not by question. Otherwise paraphrases over the same rows leak across splits. Keep all questions about a workspace in the same split. A failure-specific task may be added to development immediately; it enters blind evaluation only through independent review and a later benchmark version.

The initial v0.1 task contract replaces categorical forecast-refusal
expectations with estimate, scenario, clarification, useful-partial-answer,
and specific evidence-limit outcomes. It removes exact internal plan and
interpretation labels as answer golds: SQL versus Python or another approved
tool route is diagnostic telemetry, not product correctness. Freeze each task
set and grader before candidate runs. A later material task/rubric change
requires a new version and an unchanged anchor for longitudinal comparisons;
historical draft runs remain labeled with their exact old task-set hash.

## 5. Grading and scorecard

Grade semantic outcomes rather than exact prose or an implementation-specific query plan. For quantitative answers, the contract should include the intended measure, filters, time scope, grouping, units, denominator where relevant, and numerical tolerance. For charts, grade the underlying series, labels, units, time buckets, and aggregation separately from visual presentation.

Forecasts and scenarios are in-scope analyses, not an automatic refusal class. If the task specifies a baseline or method, grade the computed estimate against that contract and check that the answer labels it as an estimate rather than an observation or certainty. If the wording leaves materially different forecast methods or horizons open, record acceptable methods/ranges or an independent human rubric instead of forcing one hidden method. When a source lacks the time dimension or history needed for a forecast, grade whether the answer names the actual evidence gap, preserves any useful partial result, and offers a concrete next step. “Unsupported” does not mean “say no and stop.”

Report these measures independently:

- **Answer correctness:** correct result and scope among answerable episodes.
- **Clarification quality:** asks a necessary, targeted question when competing interpretations materially change the result; then uses the answer correctly.
- **Unsupported-claim rate:** confident claims not supported by the workspace. Keep this visible; do not offset it with easy correct cases.
- **False deferral rate:** unnecessary clarification or refusal on answerable episodes; report forecast estimates separately from factual answers.
- **Evidence and chart correctness:** support traces point to relevant sources; chart data and semantics match the expected result.
- **Operational validity:** fixture/setup/provider/evaluator errors, tool failures, and completion rate. Invalid evaluation runs are not model failures or successes.
- **Efficiency:** tokens, model/tool rounds, retries, and wall-clock latency per useful outcome; report quality-cost tradeoffs rather than token totals alone.

Show numerator, denominator, and task count for every slice. Repeat stochastic episodes under a fixed run protocol; report majority outcome and consistency, plus uncertainty for aggregate comparisons. Tiny topical slices are descriptive only. There is no single composite score in v0.1; if one is later introduced, publish its weighting and retain the full scorecard.

## 6. Run protocol and artifacts

For each run, record benchmark version and split, task/workspace hashes, code commit, exact model/provider/endpoint and settings, harness/tool configuration, iteration count, run validity, traces, outputs, tokens, latency, and grader version. Use the same model and budgets for paired `main` versus candidate runs. A model/harness ablation must change only the component being studied.

Keep raw run artifacts outside fixture directories. Redact secrets and avoid publishing traces containing private data. A failed provider call, missing trace, stale fixture, or grader crash must be surfaced as invalid/incomplete—not silently dropped from the denominator.

The Settings Run Log is a separate, local operational aid. It summarizes saved
turn metadata and coarse friction triggers; it does not grade correctness,
replace benchmark run artifacts, or establish a capability score. Its
content-minimized view must not expose prompts, answers, tool arguments or
results, source names, or workspace paths.

## 7. Known limitation at v0.1

The first active suites are still draft coverage, not a representative release
gate. UCI Bike Sharing is one clean, single-domain public workspace; the
housing clarification fixture is one controlled protocol example. The legacy
folder-qa battery uses one generated personal profile and mostly case-selected
files. Together they do not establish independent-topic breadth, robust
whole-folder discovery, naturalistic messiness, calibrated forecasts, or
general clarification quality. See [audit-v0.1.md](audit-v0.1.md) and
[coverage-v0.1.yml](coverage-v0.1.yml). Do not publish a broad capability claim
until target coverage, independent review, and a blind holdout are complete.

The current Fella runner can now grade explicitly declared intermediate
assistant turns and the final answer in the same conversation. The
clarification-housing episode exercises that contract. This is protocol
support, not proof of the quality of the current implementation. Third-party
or bare-model comparison runners do not yet replay such episodes; mark those
conditions unsupported rather than comparing a different interaction.

## References

- Cursor, [How we compare model quality in Cursor](https://prod.cursor.com/blog/cursorbench): describes deriving tasks from real agent work, measuring several quality/cost dimensions, and checking offline results against controlled online evaluations.
- OpenAI, [Evaluation best practices](https://developers.openai.com/api/docs/guides/evaluation-best-practices): recommends objective-specific datasets and metrics, continuous evaluation, and human review/calibration of graders.
- OpenAI, [Introducing SWE-bench Verified](https://openai.com/index/introducing-swe-bench-verified/): illustrates why task fairness, test validity, environment setup, and independent human review matter.

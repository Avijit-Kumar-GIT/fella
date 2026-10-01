# Existing benchmark-data audit (2026-10-01)

## Decision

Keep `bench/folder-qa/` as a valuable **development/regression battery**, but do not use it alone as FQA-Bench's authoritative evaluation set. It has broad topical labels and file extensions, yet its data is one controlled synthetic personal profile and its evaluation stages only the files named by each case. New, independently designed workspaces and task episodes are required.

## What is present

The checked-in `bench/folder-qa/cases.jsonl` currently contains **79 tasks**. Its metadata reports 12 domains, 8 extensions, 13 multi-file tasks, and 12 tasks with distractor files. The assets include CSV, TSV, JSON, JSONL, Markdown, text, XLSX, and PDF. The fixture generator is deterministic (`random.seed(7)`) and computes golds from its generated rows.

This is a useful smoke/regression foundation for common personal-data questions: lookup, filtering, aggregation, ranking, small joins, a few charts, and unsupported forecast requests. It is not a sample of independent real-world workspaces.

## Important limitations

- All cases come from a single generated personal profile and a small number of regular tables/documents. The data is mostly clean and stable; a few special cases deliberately exercise empty filters and a non-ISO date.
- `folder-qa-hard` reuses copies of the same profile data, so its 16 harder tasks add analytical shapes, not independent source diversity.
- The `agent_eval bench` runner stages a fresh workspace containing only each case's `files` list. Therefore the current core mostly tests analysis after relevant files have been selected. Distractor files appear only in selected cases; the default is not “inspect the entire mounted folder.”
- None of the 79 core tasks has `setup_turns`; the core is single-turn. There are three refusal cases and one no-tool case, but no explicit clarification-and-resolution episode.
- Most tasks have scalar/string-style golds. The two chart cases do not establish broad chart semantics or presentation quality.
- `bench/messiness/` has nine cases, concentrated in one spending CSV. It does not establish messy-data robustness across health, reading, travel, or mixed structured/unstructured sources.
- Existing step-judgment and policy-pressure suites contain some follow-up tasks, and visualization/memory suites test other components, but they are separate batteries with inconsistent tags and protocols. They should be catalogued as diagnostics, not silently pooled into one accuracy number.
- The legacy case records have no primary-capability tags. `catalog/folder-qa.jsonl` now supplies a provisional broad-family crosswalk, but those labels are inferred from existing tier labels and still need human review before use as authoritative slices. Several legacy tiers are too fine-grained to be stable top-level reporting slices. At least one messiness case about weight is labeled `spending`, demonstrating why topical labels need review.
- The new task manifests are tagged. `report_slices.py` joins result JSON back to those tags and reports domain, capability, interaction, answerability, format, and data-condition slices. The evaluator itself still only filters runs by its legacy `tier` field, and the old 79-case catalog labels remain provisional.

## Stale result artifacts

There are three different counts in the current checkout: **79** task records in `cases.jsonl`, **64** in the checked-in `out/taxonomy.md`, and **66** in the current website benchmark documentation. The 64-task result is historical and must not be presented as a result on the current 79-task file. Regenerate a dated run before publishing any current score; preserve the old result as a historical artifact.

## Data needed for FQA-Bench v0.1

The first public-data anchor is now present at `bench/fqa-bench/suites/uci-bike-sharing/`: its 11 draft tasks receive the whole three-file workspace, retain provenance and a pinned source archive, and exercise a second grain through a reproducible rollup. This closes the initial public-source and full-small-workspace fixture gap only for that one source family. The remaining work is:

1. Multiple independent, controlled-synthetic personal workspaces with different schemas, naming conventions, folder layouts, and co-occurring data-quality issues.
2. Broader whole-folder discovery across larger folders, nested directories, distractors, and unrelated file types. The current public suite has a complete but small three-file workspace.
3. Clarification episodes with a real material ambiguity, an acceptable targeted question, and a follow-up turn that resolves it; the current evaluator cannot yet score that interaction protocol end to end.
4. Messy conditions across more than spending: missing/invalid values, inconsistent dates and units, duplicate/summary rows, near-synonyms, conflicting sources, and structured data linked to notes/documents.
5. Additional independent public-data domains. Keep these distinct from personal-workspace tasks; they test public data, not actual private-user behavior.

Retain the existing 79 tasks as a visible development set after tagging and review. The new UCI tasks are still marked draft/provisional pending independent human review. Build any blind split from different workspace/source families; do not randomly hide questions that share these same generated files.

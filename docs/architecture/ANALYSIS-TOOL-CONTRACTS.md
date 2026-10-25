# Analysis tool contracts

This document records the guarantees the desktop agent receives from its
workspace-data tools. It describes the shipped local harness, not a promise
that every result is complete or correct. A bounded preview is not a complete
dataset; a query or search error is not evidence that the requested fact is
absent.

## Shared execution and revision contract

- Each analytical turn records the mounted workspace path and catalog revision
  captured at its start. Every model-facing tool call checks that same path and
  revision both before execution and after it returns. If either changed, the
  result is discarded, recorded as `workspace_stale`, and not treated as
  evidence for the turn. Final verification also compares the turn snapshot
  with the currently mounted workspace.
- Investigation source selection is applied inside the read/query methods,
  not merely described in the prompt. `list_files`, `inspect_table`,
  `grep_files`, `read_file`, SQL, and Python data access respect the selected
  paths. A file outside scope produces an error instead of being read.
- Tool failures preserve their message and a stable `error_kind`. The model
  receives `ERROR (<kind>): <message>` so it can choose a relevant recovery;
  the evidence record keeps both fields for inspection. Engine categories
  include `bad_input`, `query`, `no_workspace`, `auth`, `payment`,
  `no_model`, `transient`, and `internal`; the harness also uses
  `workspace_stale` and `tool_not_found`. A category does not guarantee an
  automatic retry.
- Successful evidence records retain the tool arguments and returned result.
  Source references use workspace-relative paths. SQL lineage is derived from
  the query; Python retains its bounded SQL input trace; charts inherit the
  sources of the exact result they use; prior-analysis retrieval retains the
  original evidence sources. File-oriented operations record the exact
  inspected/read/listed source, the matching search passage and line, or the
  searched document scope when no passage matches. Inventory entries for
  skipped files retain the skip reason.

## Bounds and incomplete-result signals

| Tool | Bound | How an incomplete or shortened result is signaled |
| --- | --- | --- |
| `list_files` | 40 entries by default, maximum 50; each rendered page is capped at 12,000 characters. | The page reports shown/matching counts and tells the model to continue with the next offset when entries remain, including when the character cap ends a page early. |
| `inspect_table` | 0–50 sample rows (default 5); at most six frequent values per column, with each value shortened to 48 characters. | The sample is explicitly described as a sample; full row counts and column profiles remain separate. It is not advertised as a complete chart source. |
| `run_sql` | Read-only `SELECT`/`WITH`; 1,000 rows per query by default; 15-second query watchdog by default. Results up to 100 rows are rendered completely when the engine did not cap them; larger results render a 30-row preview. | Evidence keeps the returned row count and truncation flag; the model summary says “capped,” and the text preview reports the number of remaining rows. |
| `grep_files` | At most 50 ranked passages (default 15); two streamed search passes with a 64 MiB scan budget per pass; each long source line is excerpted to 420 characters. | The result says when scanning was incomplete because of the scan limit or an unreadable source. A no-match explicitly says lexical search does not prove absence. |
| `read_file` | 12,000 characters per document and 16,000 characters for a multi-document call. | The summary marks the returned text “truncated”; the retained conversation context has its own bound and says to reread when older document text is clipped or elided. |
| `run_python` | 64 KiB input, 60-second default execution timeout, bounded Wasm memory/fuel, 64 KiB each for captured stdout and stderr, and a 6,000-character model-facing output excerpt. `sql()` returns at most 10,000 rows per query and at most 1 MiB of serialized response; only eight queries are retained in the detailed trace. | Captured streams append an explicit truncation marker; the model-facing excerpt is ellipsized; query traces include row truncation and whether the trace is complete. Missing trace entries are not implied to be absent input. |
| `read_prior_analysis` | 24,000 characters across selected evidence, 50 rows per evidence result, and 4,000 characters per stored text output. | The packet includes `detail_truncated` when evidence was shortened and lists omitted evidence IDs. It returns evidence only for the same conversation, workspace revision, and Investigation source selection; stale retrieval returns no evidence. |
| `make_chart` | 10,000 raw source rows for grouped-series, histogram, and box-plot queries; otherwise the standard 1,000-row cap. | A capped raw chart query is rejected with a message to aggregate while preserving the requested period/grain. Charts from prior results require an exact complete result ID. |

These limits are implementation defaults. The SQL timeout and Python timeout
have documented environment overrides; any override changes the effective
operational limit and should be recorded in a benchmark or diagnostic.

## Verification points

Relevant automated coverage lives beside the contracts:

- `backend/src/engine/agent.rs`:
  `failed_tool_results_keep_a_typed_recovery_category` and
  `file_tools_preserve_exact_source_provenance_with_investigation_scope`.
- `backend/src/engine/catalog.rs`:
  `inventory_pages_obey_character_cap_and_continue_by_returned_count`.
- `backend/src/engine/state.rs`:
  `read_file_caps_document_text_and_reports_truncation` and
  `sql_row_cap_keeps_total_count_and_truncation_signal`.
- `backend/src/engine/analytics/pyexec.rs`:
  `captured_python_output_is_capped_and_marks_truncation`.
- `backend/src/engine/tools.rs`:
  `small_sql_results_are_rendered_completely_for_the_model`,
  `large_sql_results_keep_a_bounded_model_preview`, and
  `prior_analysis_is_conversation_scoped_and_revision_checked`.
- `backend/src/engine/analytics/verify.rs`:
  SQL/Python evidence replay checks and lineage validation.
- `tests/e2e/workspace.spec.mjs`:
  refresh-diff, freshness-failure, stale-result preservation, and
  source-scope UI journeys.

The audit checklist item [#286](../audits/FELLA-DESKTOP-AUDIT-2026-10-14.md)
tracks this contract work.

The evidence does not establish a broad analytical accuracy claim. It
establishes bounded interfaces, source/revision guardrails, and inspectable
provenance; analytical quality still requires the separate, frozen evaluation
work described in the audit.

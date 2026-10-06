# Verifier authority and evaluation

This document defines the expected verifier behavior for product-roadmap item
10. It is a policy and test specification, not a description inferred from a
particular model response.

## Authority policy

Verification findings have an explicit effect and target. A failed check does
not, by itself, reject an answer or invalidate every result used by it.

| Finding | Permitted effect | Must preserve |
| --- | --- | --- |
| Advisory or same-model disagreement | Explain the concern; no veto | Answer text and all evidence |
| Unsupported numeric or attribution claim | Request repair of that claim; if unresolved, retain the finding for inspection | Valid query, Python, table, and chart evidence |
| Query replay mismatch | Exclude only the result that failed replay and request a replacement | Other evidence and supported answer parts |
| Tool execution error | Keep the tool error on that call and let the model repair or work around it | Unrelated successful evidence |
| Query does not honor an explicit source scope or signed-value requirement | Exclude only the identified query result and request a corrected query | Unrelated results |
| Chart cannot be reconciled with its source result | Withhold only that chart and request a replacement when possible | Source result, prose, and other visualizations |
| Workspace revision changed during the answer | Block the stale answer | The trace and findings needed to explain the block |

The verifier does not accept a model's disagreement as proof of error. An
objective execution or artifact-integrity failure can withhold only the
affected output. A global block is reserved for a finding whose scope is the
entire answer, such as a workspace revision changing while it was being
computed. Existing read-only and safety boundaries are not weakened by this
policy.

The repair budget remains bounded. A repair must identify affected evidence
by ID; it must not clear the complete tool-call memo or mark an arbitrary
first result as superseded. Tool execution errors and verifier exclusions are
different states and must remain distinguishable in the trace.

## Evaluation criteria

Tests are written against the policy above and do not use model output as the
expected result. The following behavior is required:

1. A chart/source mismatch withholds the chart, does not produce a whole-answer
   hard failure, and leaves the source result eligible to support prose or a
   table.
2. An unsupported number requests a claim-scoped repair, does not hard-fail the
   turn, and does not exclude its supporting evidence.
3. A replay mismatch identifies the exact evidence item, makes that item
   ineligible for later checks, and does not invalidate sibling evidence.
4. A successful replay remains distinguishable from a failed replay and is
   required for the existing `Verified` status.
5. A same-model disagreement is informational and cannot cause an answer
   block or evidence exclusion.
6. A stale workspace revision is explicitly answer-scoped and remains a hard
   block.
7. The UI displays the target and effect only inside Analysis Details; a
   withheld chart is not rendered as if it passed, while unrelated answer
   content remains visible.

The controlled scripted run and its limitations are recorded in
[`VERIFIER-AUTHORITY-EVAL.md`](./VERIFIER-AUTHORITY-EVAL.md). For subsequent
model-backed or benchmark evaluations, report false-positive artifact/evidence
blocks, unsupported-claim acceptance, supported-answer coverage, retained
evidence, repair/tool-call count, and latency on an independently specified
task set. The scripted run does not claim general live-model improvement.

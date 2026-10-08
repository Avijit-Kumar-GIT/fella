# Blind qualitative review rubric v0.1

Use this rubric alongside the frozen task contract. It annotates the observed
answer and interaction; it does not revise the task, expected result, or
machine grade. Preserve disagreements and adjudicate them separately.

## Blinding and unit

- The review unit is one task × run condition × iteration, including the
  complete interaction needed to answer it (setup turns, clarification, and
  continuation where applicable).
- Randomize packet IDs and hide model, branch, and baseline/candidate labels
  until annotations are locked. Reviewers may see the task, authorized
  workspace snapshot, answer, and trace because those are required to judge
  evidence and method.
- Capture each reviewer independently using
  `reviewer-assessment.schema.json`; that raw record must not contain the
  task/model/run/condition mapping or expected-behavior gold. Run
  `review_agreement.py` on the blinded records before restoring that mapping.
  Preserve both reviewers' original rows and any later adjudication.
- Reviewers must not see the answer key before recording their independent
  judgment. They may inspect it afterward to resolve a disagreement, never to
  silently change the frozen gold.
- Store only the content-free fields in `review-annotation.schema.json` in
  committed aggregate artifacts. Keep answer/trace packets in the controlled
  local evaluation area and remove them according to the run's retention
  policy.
- After the reviewer labels are locked, the evaluation coordinator joins the
  task's frozen expected-behavior label and hidden run-condition mapping into
  the annotation. The reviewer packet must not expose either field.

## Labels

- `answer_behavior`: `answered` provides a substantive answer; `clarified`
  asks a targeted material question; `useful_limit` explains a real evidence
  limit and preserves supported partial value; `unnecessary_deferral` avoids
  an answer or a useful clarification despite available evidence; and
  `error_or_empty` means the interaction did not produce a usable response.
- `correctness`: judge the user-visible result against the frozen task
  contract, including scope, values, units, assumptions, and required
  qualifications. Use `partially_correct` for material omissions or mixed
  correct/incorrect results; do not award correctness for a refusal that
  merely avoids a wrong number.
- `evidence_support`: inspect claim-to-source support. A correct number with a
  mismatched or absent source is not fully supported. `not_applicable` is for
  requests that do not make evidence-dependent claims.
- `unsupported_claims`: count distinct factual or numeric claims with no
  support in the permitted evidence or model-knowledge scope. Do not count
  clearly labeled estimates as unsupported solely because they are uncertain.
- `clarification`: `necessary_and_used` requires a material ambiguity, a
  focused question that distinguishes the alternatives, and correct use of
  the reply. `necessary_but_not_asked` means the ambiguity materially changes
  the result. An unnecessary question imposes a turn without material benefit.
- `chart_correctness`: judge underlying values/aggregation, chart form,
  encodings, labels, ordering, units, missingness, and whether the chart helps
  answer the request. A correct prose answer does not rescue an incorrect
  chart.
- `forecast_quality`: check time ordering, held-out leakage, method fit,
  baseline/backtest when the task contract calls for it, and uncertainty
  communication. Grade the frozen method contract; do not introduce a new
  preferred forecast after seeing the result. For held-out point forecasts,
  record the prediction and observed value in the optional numeric fields so
  the scorecard derives absolute error. Record the predeclared baseline value
  and interval bounds when the task has them; interval coverage is reported
  only for episodes with an explicit interval and held-out observation.
- `failure_tags`: select every evidenced failure class. Keep `other` rare and
  explain it during adjudication; do not create production rules from one
  label.

Two reviewers independently annotate at least the blind holdout and a
predeclared sample of development cases. Report agreement and adjudication
rates by field; retain the original annotations. If reviewer agreement is
poor, revise the rubric only for a new evaluation version and re-annotate
without using candidate identity or score as a guide.

# Repository guidance for coding agents

## Evaluation and test integrity

- Define expected behavior and grading criteria before running the candidate
  implementation. Do not change, delete, weaken, or exclude a failing test
  because of the observed output.
- Report failing tests as failures. If a test appears ambiguous, incorrectly
  specified, or poorly graded, preserve its result and flag the concern
  separately for user review; do not silently convert it to a pass.
- Change a test expectation only when the user approves a correction grounded
  in the intended behavior—not to fit the implementation or a model response.
  Document the reason, version the benchmark/task set, and rerun comparisons
  under the same revised version.
- Keep benchmark criteria implementation-independent. A suspected grader
  false negative is an adjudication question, not permission to tune the gold
  answer after seeing candidate output.
- In reports, include failures, exclusions, grader limitations, and any
  post-hoc changes. Never present a post-hoc-adjusted score as an untouched
  baseline.

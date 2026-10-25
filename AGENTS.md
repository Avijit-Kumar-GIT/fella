# Repository guidance for coding agents

## Product UI design style

- Preserve Fella's established desktop visual language: Mac-like curvature and
  surface treatment, paired with Notion-like clarity and calm information
  hierarchy. Do not make configurable layouts look like a generic dashboard or
  a stock component-library demo.
- Use consistent, restrained corner radii and purposeful surface layering.
  Curvature should unify windows, panes, controls, and content—not turn every
  element into a separate rounded card.
- Favor readable typography, clear left-aligned hierarchy, useful whitespace,
  quiet chrome, and progressive disclosure. Keep the primary analytical work
  visually dominant and avoid dense, equal-weight columns that squeeze content.
- Composable panels and layout presets must use Fella's existing design tokens
  and component language. Prefer flexible, resizable panes and clear grouping
  over a fixed grid; customization must not introduce inconsistent spacing,
  radii, borders, or control treatments.
- Compact layouts may hide secondary detail, but must keep the active workspace
  and Investigation scope clear and must not obscure incomplete coverage or
  required review. Layout changes affect presentation only, never data access,
  analytical state, or provenance.

## Branch delivery

- When requested work is complete and proportionate validation has run, commit
  the task's changes and push the commit to the current branch by default.
- Do not switch branches, push to a different branch, or open a pull request
  unless the user asks. Respect an explicit request to keep changes local or
  not push.
- Stage only files belonging to the current task. Preserve unrelated or
  pre-existing work; if task changes cannot be safely isolated, ask before
  committing.
- Report the commit and push result, including the commit hash and branch.

## Evaluation and test integrity

- Define expected behavior and grading criteria before running the candidate
  implementation. Do not change, delete, weaken, or exclude a failing test
  because of the observed output.
- Keep test investigation bounded. Do not rerun the same failing case under the
  same setup and hypothesis more than twice. Every additional attempt must test
  a stated, materially different hypothesis or verify a relevant code change.
- After repeated failure, stop and inspect the available evidence once: the
  assertion, logs, app state, and relevant implementation. Distinguish a
  demonstrated product failure from a test synchronization, locator,
  environment, or specification problem. A DOM state or passing assertion is
  evidence only for what it directly establishes; do not infer more.
- If the intended behavior may be working but the test cannot establish it,
  preserve the failing result and report it as unresolved. Use a distinct,
  proportionate validation method only when it adds evidence; do not keep
  cycling the same test or alter its assertions merely to obtain a pass.
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

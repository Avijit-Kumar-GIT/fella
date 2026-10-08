# Contributing to Fella

Fella is a local-first analytics harness whose model inspects and analyzes
user-selected files through a bounded, read-only runtime. Before a substantial
change, read the [product contract](docs/PRODUCT.md),
[architecture](docs/ARCHITECTURE.md), and relevant
[testing guidance](docs/TESTING.md). For UI changes, also read
[Design](docs/DESIGN.md).

## Development

Use the Electron development workflow:

```sh
pnpm install
pnpm electron:dev
```

The Rust backend and sidecar live in `backend/`. Full setup instructions are in
[`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md). Run the relevant checks from
[`docs/TESTING.md`](docs/TESTING.md) before submitting a change.

## Change expectations

- Keep analytics read-only. The agent must not write, move, or delete source
  files; editing `fella.md` is a separate, explicit user action.
- Keep the model in charge of interpretation and tool choice. Runtime rules
  enforce general safety and execution boundaries; they should not encode one
  benchmark example's wording or file names.
- SQL and sandboxed Python are both first-class. Use the method that best fits
  the analysis rather than treating Python as a fallback.
- Update the architecture or product docs when a boundary or user-visible
  contract changes.
- Add regression coverage for deterministic defects and independent,
  predeclared evaluation criteria for model behavior.

## Test and benchmark integrity

Define expected behavior and grading criteria before running a candidate. Do
not change, delete, weaken, or exclude a failing test to fit observed output.
If a test seems ambiguous or incorrectly graded, retain the failure and flag
it for review. Correct an expectation only with approval and a rationale
grounded in intended behavior; version the task set and report results
separately. See [`docs/TESTING.md`](docs/TESTING.md).

## Pull requests

- Keep each PR focused on one logical change and describe the problem it
  addresses.
- Use a Conventional Commit title such as `feat(agent): ...`, `fix(ui): ...`,
  `test(analytics): ...`, or `docs: ...`.
- Include verification performed and disclose failures, exclusions, and any
  changed test contract.
- Justify new dependencies, tools, external services, and write capabilities
  against Fella's product boundary.

Report bugs with the steps, expected and actual behavior, and operating
system. Assume good faith and keep discussion respectful.

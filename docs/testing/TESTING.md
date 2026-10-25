# Testing and evaluation

Use the check that supports the claim being made. Deterministic tests cover
encoded behavior; desktop journeys cover real app flows; model evaluation
measures behavior on fixed tasks. None alone proves Fella is correct on
arbitrary user data.

Test code is grouped under [`tests/`](../../tests/). Keep each change's
expectations independent of its implementation and preserve failed results.

## Local checks

From the repository root:

```sh
pnpm check
pnpm build
cargo fmt --manifest-path backend/Cargo.toml --all -- --check
cargo clippy --manifest-path backend/Cargo.toml --all-targets --features eval --locked -- -D warnings
cargo test --manifest-path backend/Cargo.toml --locked
python3 -m unittest discover -s tests/unit/python
pnpm test:electron-bridge
pnpm test:electron-update
pnpm test:chart-renderer
pnpm test:release-artifacts
pnpm test:release-config
```

These cover frontend diagnostics/build, Rust integration and source-local unit
tests, benchmark reporting utilities, Electron bridge/updater behavior, chart
rendering, and release metadata. The optional `duckdb` feature is not part of
the normal release or CI gate.

## Desktop journeys

The Playwright configs and shared Electron harness live beside the suites in
[`tests/e2e/`](../../tests/e2e/). The tests use disposable app profiles. For
example:

```sh
pnpm electron:build
pnpm test:e2e:generative
pnpm test:e2e:desktop-ui
pnpm test:e2e:workspace
```

Credentialed journeys use a configured provider and incur provider usage:

```sh
pnpm test:e2e:release:live
pnpm test:e2e:charts:journey
pnpm test:e2e:analysis-families:real
```

The analysis-family journey runs against Luna and Gemma 4 31B. Configure the
provider in the app auth file; the Gemma route should use Ollama Cloud unless
the local Ollama server actually hosts that model. See
[`E2E-JOURNEYS.md`](./E2E-JOURNEYS.md) for prompts and manual visual checks.
Journey completion does not grade numerical correctness or chart suitability.

For a packaged application, pass the platform executable to
`pnpm test:electron:packaged`. This smoke check verifies startup and the Rust
sidecar connection.

## Analytics quality

FQA-Bench task contracts describe user intent and acceptable outcomes, not a
required SQL/Python plan, tool order, or phrase. Read its
[`methodology`](../../bench/fqa-bench/methodology.md),
[`audit`](../../bench/fqa-bench/audit-v0.1.md), and
[`run guide`](../../bench/fqa-bench/README.md) before interpreting results.

General knowledge, web research, forecasting, charts, clarification, and
hybrid workflows should be reported as separate families. Do not conflate an
answer score with runtime verification status or workflow completion.

Model-backed evaluation needs provider credentials and can incur usage costs.
For the visualization benchmark, set `AGENT_EVAL_DATA_DIR` to a scratch copy
of the app data and run:

```sh
pnpm test:visualizations
```

## Evaluation integrity

- Define expected behavior and grading criteria before running the candidate.
- Keep failed results visible. Treat suspected grader false negatives as
  adjudication questions, not permission to tune the implementation or gold.
- Change expectations only with owner approval grounded in intended behavior;
  record the reason, version the task set, and compare under the revised set.
- Keep tests independent of the implementation. Do not hard-code one fixture's
  vocabulary or filenames into production logic.
- Report failures, exclusions, invalid runs, grader limits, and post-hoc
  changes alongside every score.

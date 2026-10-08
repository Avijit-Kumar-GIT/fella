# Testing and evaluation

Use the test layer that supports the claim being made. Deterministic tests
establish encoded engine and UI behavior; an end-to-end journey establishes
that the app interaction works; model evaluation measures behavior on a fixed
task set. None alone proves that Fella is correct on arbitrary user data.

## Local checks

From the repository root:

```sh
pnpm check
pnpm build
pnpm test:electron-bridge
pnpm test:electron-update
pnpm test:release-artifacts
pnpm test:chart-renderer
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --all-targets --features eval --locked \
  --manifest-path src-tauri/Cargo.toml -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml
```

These cover frontend diagnostics/build, Electron bridge and updater behavior,
release artifacts, chart rendering, and the default SQLite Rust engine. The
optional `duckdb` feature is not part of the normal release/CI gate.

## End-to-end journeys

The live Electron suite exercises real application journeys, including
clarification/resume, chart rendering, cancellation, persistence, and compact
layout. It uses disposable app profiles. Real-model tests use the configured
provider credentials and incur provider usage:

```sh
pnpm test:e2e:release:live
pnpm test:e2e:charts:journey
pnpm test:e2e:analysis-families:real
```

The last command runs chart-family journeys against Luna and Gemma 4 31B. The
Gemma route must use the configured Ollama Cloud provider unless the local
Ollama server actually hosts that model. See
[`testing/E2E-JOURNEYS.md`](testing/E2E-JOURNEYS.md) for complete prompts and
visual checks. Journey pass/fail means workflow/rendering completed; it does
not grade numerical correctness or whether a chart was the best analytical
choice.

## Analytics quality

FQA-Bench is the versioned benchmark for filesystem analytics. Its task
contracts describe user intent and acceptable outcomes—not a required SQL or
Python plan, tool order, or phrase. The current v0.1 specification is an
unreleased target-state benchmark with draft coverage, not a representative
release-grade score. Read its
[`methodology`](../bench/fqa-bench/methodology.md),
[`audit`](../bench/fqa-bench/audit-v0.1.md), and
[`run guide`](../bench/fqa-bench/README.md) before interpreting results.

General knowledge, web research, forecasting, charts, clarification, and
hybrid workflows should be reported as separate families. The answer score
must not be conflated with runtime `Verified`/review status, and successful
workflow completion must not be reported as analytical correctness.

### Evaluation integrity

- Define the expected behavior, gold, and grading criteria before a candidate
  run.
- Keep failed results visible. A suspected grader false negative is an
  adjudication question, not permission to tune the implementation or gold.
- Change expectations only with owner approval grounded in intended behavior;
  record the rationale, version the task set, and report comparisons under
  the revised version.
- Keep tests independent of the implementation. Add a test for an observed
  defect, but do not hard-code the example's vocabulary or file names into
  production logic.
- Report failures, exclusions, invalid runs, grader limits, and post-hoc
  changes alongside every score.

## Latest recorded model journey

The paired real-model chart-family repeat on 2026-10-07 completed **6/6 Luna
turns and 5/6 Gemma turns**. Gemma's forecast-chart request failed after two
invalid chart-tool attempts. An earlier run scored 4/6 Luna and 6/6 Gemma. The
journey checked that charts rendered and the app did not stall; it did not
grade answer correctness or chart suitability. These small runs are diagnostic
observations, not a quality score or generalization claim. See
[`benchmarks`](../bench/fqa-bench/README.md) for the evaluation standard and
[`verifier authority run`](testing/VERIFIER-AUTHORITY-EVAL.md) for the dated
orchestration conformance results and its limitations.

The 2026-10-07 local release-gate run also passed the default Rust suite,
Clippy, frontend checks/build, Electron bridge/update/chart/artifact checks,
all eight Playwright release journeys, and packaged Windows/Linux startup
smokes. The Rust run reported 330 library tests and 23 agent-loop tests passed;
one agent-loop test and two explicit manual/performance probes were ignored.
No macOS GUI test was run. These results establish the checked workflows on
the tested systems, not numerical correctness for arbitrary user data.

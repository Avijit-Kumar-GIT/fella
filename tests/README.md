# Tests

Test code and test-only fixtures live here, grouped by the way they run.

| Directory | Purpose |
| --- | --- |
| [`e2e/`](./e2e/) | Playwright desktop and live-model journeys, their configs, and shared Electron helpers |
| [`rust/`](./rust/) | Rust integration tests and their fixtures; Cargo targets are registered in `backend/Cargo.toml` |
| [`unit/javascript/`](./unit/javascript/) | Deterministic JavaScript and renderer contract checks |
| [`unit/python/`](./unit/python/) | Tests for benchmark reporting utilities in `bench/` |
| [`evals/`](./evals/) | Provider-backed analysis runners and model-facing evaluation entry points |
| [`scripts/`](./scripts/) | Repository checks used by contributors and CI |

Rust unit tests that need private implementation details stay beside their
modules under `backend/src`. New standalone test code belongs in the matching
directory here. Keep benchmark task data and production evaluation utilities
under `bench/`.

## Common commands

From the repository root:

```sh
cargo test --manifest-path backend/Cargo.toml --locked
python3 -m unittest discover -s tests/unit/python
pnpm test:electron-bridge
pnpm test:electron-update
pnpm test:chart-renderer
pnpm test:json-render
pnpm test:studio-report
```

See [`docs/testing/TESTING.md`](../docs/testing/TESTING.md) for desktop and
provider-backed test commands, build prerequisites, and evaluation guidance.

The live Python-agent smoke runner is an optional model-backed example:

```sh
cd backend
FELLA_E2E_DATA_DIR=/path/to/copied-app-data \
  cargo run --release --example python_e2e
```

# Build and validation

The public repository keeps contributor checks and model evaluation guidance.
Standalone unit, integration, and Playwright test suites are not published.

## Contributor checks

From the repository root:

```sh
pnpm check
pnpm build
cargo check --locked --manifest-path backend/Cargo.toml
cargo fmt --manifest-path backend/Cargo.toml --all -- --check
cargo clippy --manifest-path backend/Cargo.toml --lib --locked -- -D warnings
```

CI runs frontend diagnostics, the production app build, Rust formatting, and
Clippy over the Rust library. Release automation builds installers for macOS,
Windows, and Linux. Building an installer does not establish that every
platform's interactive app flow works correctly.

The optional `duckdb` feature is not part of the standard build checks.

## Model evaluation

The benchmark harness and task sets under [`bench`](../../bench/README.md)
remain available for measuring model behavior. These evaluations are separate
from the contributor build checks, may use provider credentials and incur
usage charges, and do not establish correctness for arbitrary user data.

Keep task contracts independent of implementation details. Record the exact
model, provider, commit, and task-set version, and preserve failed outcomes in
the report.

# Development

This guide is for building and changing Fella. End users do not need Node,
pnpm, or Rust; packaged apps include the Electron runtime and Rust sidecar.

## Prerequisites

- Node.js 22 or newer and pnpm 11.
- Stable Rust 1.93 or newer, plus the platform's C/C++ build tools.
- On Windows, Visual Studio C++ Build Tools. On macOS, Xcode Command Line
  Tools. On Linux, Electron's Chromium libraries plus OpenSSL development
  headers and a C/C++ toolchain.
- A connected model provider is needed for live model journeys, not for the
  deterministic Rust and frontend tests.

## Run the app

From the repository root:

```sh
pnpm install
pnpm electron:dev
```

This builds the Svelte frontend and Rust sidecar, starts Vite, then launches
Electron. It is a single-terminal workflow on Windows, macOS, and Linux. For a
renderer-only development server, run `pnpm dev` and launch Electron in a
second terminal with `FELLA_ELECTRON_URL=http://127.0.0.1:1420` set.

The Rust backend can be checked or tested without opening the UI:

```sh
cargo test --locked --manifest-path backend/Cargo.toml
```

## Providers and local state

Connect a provider and choose a model in the app. Provider catalogs and model
IDs can change, so use the live list shown by the app rather than relying on a
model ID copied into this guide. Provider credentials are held in `auth.json`
under the Fella application-data directory, separate from settings and
conversation databases.

For isolated UI tests or benchmarks, set `FELLA_DATA_DIR` to a disposable
directory. Do not run two instances that write to the same data directory.
Copy credentials into an isolated test directory only when a real-provider
test requires them; never include them in test output or committed artifacts.

## Code and documentation conventions

- Keep the renderer a presentation layer. Electron's preload is an allowlisted
  bridge; the Rust runtime owns analytics, local persistence, and the
  canonical turn trace.
- Prefer shared UI tokens and primitives in `src/app.css`; check light and dark
  modes, keyboard focus, reduced motion, and accessible names when changing UI.
  See [Design](DESIGN.md).
- Update [Architecture](ARCHITECTURE.md) in the same change when a runtime
  boundary changes. Keep user-facing product scope in [Product](PRODUCT.md).
- Never change a test or benchmark expectation simply to match candidate
  output. Preserve the failure and flag a suspected specification or grader
  issue for review; see [Testing](TESTING.md).

## Useful links

- [Testing and evaluation](TESTING.md)
- [Performance measurements](PERFORMANCE.md)
- [Release runbook](RELEASE.md)
- [Mintlify deployment](DOCS-DEPLOYMENT.md)

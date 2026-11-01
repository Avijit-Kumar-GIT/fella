# Development

This guide is for building and changing Fella. End users do not need Node,
pnpm, or Rust; packaged apps include the Electron runtime and Rust sidecar.

## Prerequisites

- Node.js 22 or newer and pnpm 11.
- Stable Rust 1.93 or newer.
- The `wasm32-unknown-unknown` Rust target for the shared UI contract. The
  development/build/check hooks install it through rustup on first use;
  alternatively run `rustup target add wasm32-unknown-unknown` beforehand.
- Platform libraries required by Electron. Optional DuckDB builds may require
  additional native build tools. The standard app no longer depends on
  `node-pty` and does not require Visual Studio C++ Build Tools.
- A connected model provider is needed for live model journeys, not for
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

The Electron launcher starts Vite directly after that build, avoiding a second
contract-generation/build hook. It allows up to two minutes for cold dependency
optimization and reports progress every ten seconds while waiting. An early
Vite exit or a server belonging to another checkout still fails explicitly.

The Rust backend can be tested without opening the UI:

```sh
cargo test --locked --manifest-path backend/Cargo.toml
```

Presentation contract sources live in `crates/presentation-contract/`. Their
native build and browser WASM build run identical validation and compilation.
After changing catalog props, authoring, chart grammar, or acceptance code, run
`pnpm generate:presentation-catalog` and `pnpm build:presentation-contract`.
Normal `pnpm dev`, `pnpm build`, and `pnpm check` run these hooks automatically.
The catalog and compact engine contract are generated, tracked resources; the
platform-independent `.wasm` asset is generated locally and bundled by Vite.

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
- Prefer shared UI tokens and shadcn-svelte primitives in
  `src/lib/components/ui/`; check light/dark modes, keyboard focus, reduced
  motion, and accessible names when changing UI. See [Design](../design/DESIGN.md).
- Update [Architecture](../architecture/ARCHITECTURE.md) in the same change when a runtime
  boundary changes. Keep user-facing product scope in [Product](../product/PRODUCT.md).
- Never change a test or benchmark expectation simply to match candidate
  output. Preserve the failure and flag a suspected specification or grader
  issue for review; see [Testing](../testing/TESTING.md).

## Useful links

- [Testing and evaluation](../testing/TESTING.md)
- [Performance measurements](../testing/PERFORMANCE.md)
- [Release runbook](../operations/RELEASE.md)
- [Mintlify deployment](../operations/DOCS-DEPLOYMENT.md)

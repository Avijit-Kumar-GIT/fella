# Electron desktop shell

Electron 44.5.1 is Fella's only maintained desktop shell (macOS 13 or newer).
The Svelte renderer owns
presentation; Electron's main process owns native windows, dialogs, external
links, update downloads, and the narrow preload bridge. The Rust process is the
analytics engine and local persistence layer, not a second UI framework.

```text
Svelte renderer
  └─ context-isolated, sandboxed preload
       └─ Electron main process
            ├─ native dialogs, window controls, safe external links
            └─ Rust engine sidecar (newline-delimited JSON)
                 └─ analytics, local SQLite, provider credentials
```

The renderer cannot access Node or the filesystem directly. Its typed API is
in `src/lib/ipc.ts`; the preload allowlists requests in
`electron/preload.cjs`; `electron/engine.mjs` correlates JSON-lines requests
and streamed events with the Rust engine. The Rust crate currently lives under
`src-tauri/` for repository-path compatibility, but it no longer depends on or
starts Tauri.

## Develop on Windows

Install Node 22+, pnpm 11, and Rust 1.93+ with the Visual Studio C++ Build
Tools. Then, from PowerShell at the repository root:

```powershell
pnpm install
pnpm electron:dev
```

The project postinstall fetches the Electron runtime binary pinned by
`package.json`, so a successful dependency install is ready to launch. If a
restricted network or interrupted install leaves `node_modules/electron/dist`
without `electron.exe`, rerun `pnpm exec install-electron --no`.

`electron:dev` builds the Svelte app and Rust sidecar, starts Vite, waits for
the dev server, and launches Electron. It is a single-terminal workflow. For a
renderer-only Vite session, use `pnpm dev` and start Electron in a second
terminal with `FELLA_ELECTRON_URL=http://127.0.0.1:1420` set.

Useful checks and builds:

```powershell
pnpm check
pnpm build
pnpm test:electron-bridge
pnpm test:electron-update
pnpm test:chart-renderer
cargo test --manifest-path src-tauri/Cargo.toml --locked
pnpm electron:package
```

The Rust sidecar receives its data directory from Electron. By default, Fella
continues using the existing `dev.fella.app` directory so settings, `auth.json`,
workspace metadata, and conversation history survive the shell migration. On
Windows that is under `%APPDATA%\dev.fella.app`. Set `FELLA_DATA_DIR` only for
an isolated test or benchmark; never run two app instances against the same
data directory while writing to it.

## Packaging and release

`pnpm electron:package` builds the current platform installer locally. Release
automation builds macOS universal (Intel and Apple silicon), Windows x64, and
Linux x64 packages. Each package includes the matching Rust sidecar. The
release workflow first runs its quality gate, then creates a GitHub draft,
attaches installers, and computes `SHA256SUMS`. It does not publish the draft
automatically.

Installers are currently unsigned, as in the previous release. macOS may block
an unnotarized download, and Windows may show a SmartScreen warning. Checksums
detect download corruption or mismatch; they do not establish publisher
identity. See [`RELEASE.md`](RELEASE.md) for the maintainer runbook and
[`ELECTRON-VALIDATION.md`](ELECTRON-VALIDATION.md) for the current test record.

## Data and updater compatibility

The UI and Rust engine retain Fella's local-first, read-only workspace
boundary. Electron does not grant the model direct filesystem access. Folder
selection is an OS dialog; the engine reads only the selected workspace through
its catalog and bounded tools.

`/update` is a user-triggered path available only in a packaged build. It
downloads the matching platform artifact, verifies its SHA-256 against the
release's `SHA256SUMS`, and then applies it. Linux `.deb` installs require a
manual reinstall because replacing them requires package-manager privileges.
The first Electron release is a shell migration from the old Tauri `v0.2.0`
app and must be installed manually; the old shell's updater cannot convert to
Electron. Later Electron releases use the Electron updater.

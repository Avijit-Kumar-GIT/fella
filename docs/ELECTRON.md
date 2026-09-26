# Electron comparison branch

This branch keeps the existing Svelte UI and Rust analytics engine, replacing
only the Tauri shell with Electron. The Rust process is launched as a sidecar
and exposes the same command names over line-delimited JSON:

```
Electron main / preload  →  Rust engine sidecar
        ↓                         ↓
   Svelte renderer          EngineState + SQLite
```

The bridge preserves streamed `ask` events, folder selection, external links,
window controls, conversation history, settings, workspace operations, theme
surfaces, and the explicit update flow. Windows uses Fella's frameless controls;
Linux and macOS retain native window decorations, matching the Tauri configs.
The Electron branch keeps its data under `dev.fella.app-electron` by default so
it can be opened beside the Tauri build. Set `FELLA_DATA_DIR` to compare against
a specific data directory.

## Run it

From Windows PowerShell:

```powershell
pnpm install
pnpm electron:run
```

For a hot-reloading development session, `electron:dev` builds the Rust
sidecar, starts Vite, waits until port 1420 is serving, and then starts
Electron as one process tree:

```powershell
pnpm electron:dev
```

The equivalent two-terminal flow is `pnpm dev` in one terminal and
`pnpm exec electron .\electron\main.mjs` with `FELLA_ELECTRON_URL` set in the
other. The shell waits for that URL before loading, so starting Electron a
little before Vite is ready no longer produces a transient 404 or refused
connection.

`electron:build` builds the existing Rust crate in release mode, prepares the
sidecar, and builds the static Svelte app. The packaged build is:

```powershell
pnpm electron:package
```

## Compare memory

Build the Tauri release on `ui-editorial-pass`, export the measurement helper
from this branch, record its process-tree sample, then switch back and record
the Electron sample:

```powershell
# on ui-editorial-pass
git switch ui-editorial-pass
pnpm tauri build
git show electron-migration:scripts/measure-windows.ps1 | Out-File -Encoding utf8 $env:TEMP\measure-windows.ps1
& powershell -ExecutionPolicy Bypass -File $env:TEMP\measure-windows.ps1 -Shell Tauri

# on electron-migration
git switch electron-migration
pnpm electron:build
.\scripts\measure-windows.ps1 -Shell Electron
```

The script reports the complete child-process tree, not just the visible app
row: working set (RAM currently resident), private bytes, and virtual memory.
Take both samples after the same idle interval, first with no workspace open,
then after opening the same folder. The second comparison matters more for
Fella because it includes the Rust engine and indexed workspace state.

The Linux container used for development may not have Electron's desktop system
libraries installed, so the authoritative comparison should be run on the same
Windows machine using the same release configuration.

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
window controls, conversation history, settings, and workspace operations. The
Electron branch keeps its data under `dev.fella.app-electron` by default so it
can be opened beside the Tauri build. Set `FELLA_DATA_DIR` to compare against a
specific data directory.

## Run it

From Windows PowerShell:

```powershell
pnpm install
pnpm electron:build
node_modules/electron/dist/electron.exe electron/main.mjs
```

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

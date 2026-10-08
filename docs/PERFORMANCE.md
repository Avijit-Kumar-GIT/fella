# Performance measurements

Fella has several different costs: preparing a mounted folder, answering a
question through model round trips, the Rust sidecar, and the full Electron
process tree. Keep those measurements separate and record the OS, build type,
workspace, model, and observation interval with each result.

## Available probes

```sh
./scripts/measure.sh
./scripts/measure.sh --bloat
./scripts/measure.sh --build
./scripts/measure.sh --build-cold
./scripts/measure.sh --min
./scripts/check-memory.sh
```

`measure.sh` reports Rust sidecar dependencies, build time, binary size, and
frontend bundle size. It does not launch Electron or measure full-app memory.
The `--bloat` option relinks the optimized Rust binary; cold builds and the
optional probes can take several minutes.

For a Windows Electron process-tree sample, build and run the app with an
isolated profile:

```powershell
pnpm electron:build
.\scripts\measure-windows.ps1 -Seconds 15
```

This sums Electron and Rust child processes for the development executable.
For installed-app comparisons, install and launch the packaged app, hold the
workspace and idle interval constant, and include the entire process tree.
Working set and private bytes are distinct; virtual address space is not
physical memory use.

`check-memory.sh` exercises repeated SQL-backed and pure-Python Wasm runs. It
checks guest memory behavior and, on Linux, process RSS growth; it is not a
full-app leak test.

Capture a report explicitly when needed, for example:

```sh
./scripts/measure.sh | tee /tmp/fella-performance.txt
```

The measurement script no longer appends results to a committed historical
log. Keep raw, machine-specific runs with the benchmark artifacts or local
test records; do not present single-run diagnostics as a product guarantee.

## Recorded ingestion diagnostics

Linux release-mode scale probes recorded in October 2026 include one run that
mounted a mixed workspace with 5,000 small
table sources, 100 text documents, 102 visible unsupported/malformed paths,
and a 10-GiB CSV containing about 275 million rows. The 10-GiB run took about
578 seconds to publish the complete workspace and used about 11.1 GB of SQLite
scratch; the test process reported about 49 MB peak RSS. It loaded the expected
sources and rows. A separate 5,000-small-source run took about 3 seconds.

These are isolated Linux release-build diagnostics, not controlled
comparisons, packaged-app measurements, multi-large-file coverage, or Windows
results. Workspace publication is atomic after preparation, so the UI does not
query partially prepared sources. Incremental refresh and progressively
queryable sources remain open work. See [Roadmap](ROADMAP.md).

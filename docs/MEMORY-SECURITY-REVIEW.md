# Memory and execution review

Reviewed 2026-09-17 for the personal desktop build. This document records the
current WASM + RustPython direction and the checks that protect the Rust host
from unbounded generated work. It is a release engineering record, not a
formal proof that every dependency or operating-system allocator is defect-free.

## Current execution boundary

Model-generated Python runs in the checked-in
src-tauri/resources/fella-python-sandbox.wasm artifact. It is compiled from
python-sandbox/src/lib.rs for wasm32-unknown-unknown and executed by Wasmi in
src-tauri/src/engine/analytics/pyexec.rs. The module is deliberately not WASI:
there are no filesystem, network, environment, clock, or subprocess imports.
The host provides only captured output, OS entropy for interpreter startup, and
a bounded read-only SQL bridge.

The same guest and host policy run on Linux, macOS, and Windows. That keeps the
personal package lightweight and avoids three native Python sandbox
implementations. This is a capability boundary inside the process. If Fella
later executes arbitrary native subprocesses, the OS-worker design in
docs/PYTHON-SANDBOX.md must be added around it; a native subprocess must never
be treated as equivalent to this guest.

## Bounds

| Resource | Current limit | Enforcement |
| --- | ---: | --- |
| Generated source | 64 KiB | host checks before guest execution |
| Captured stdout/stderr | 64 KiB each | bounded host sinks |
| Guest linear memory | 256 MiB | Wasmi store limiter |
| Guest value stack | 2 MiB | Wasmi engine configuration |
| Guest tables/instances | 10,000 table elements, 2 instances, 4 tables, 1 memory | Wasmi store limiter |
| Fuel | 1 billion units | Wasmi fuel metering |
| User cancellation | 20 million unit slices | resumable Wasmi call checks an atomic stop flag between slices |
| Python wall time | 60 seconds by default | host deadline; FELLA_PYTHON_TIMEOUT_SECS overrides |
| SQL query | 64 KiB text, 10,000 rows, 1 MiB JSON response | host and guest ABI checks |
| SQL wall time | 15 seconds by default | SQLite interrupt watchdog; FELLA_QUERY_TIMEOUT_SECS overrides |
| Delimited input | 2 million rows and 256 MiB retained input | SQLite ingestion |
| Document cache | 32 MiB | bounded LRU-style cache in the engine |
| Conversation memory | 24 active sessions | LRU eviction |
The limits bound accidental memory spikes and runaway analysis. They do not
The limits bound accidental memory spikes and runaway analysis. They do not
make a vulnerable Wasmi, RustPython, or SQLite dependency harmless.

## Allocation and teardown review

Each Python call creates a fresh Wasmi Store. Interpreter objects and the guest
allocator therefore cannot carry state into the next question. The Store owns
both temporary buffers and the rest of the guest allocation lifetime:

1. the host allocates and writes the source buffer;
2. the guest allocates a fixed SQL response buffer when sql() is called;
3. the guest copies the response into owned JSON/Python values;
4. dropping the Store releases both buffers, the remaining RustPython heap, and
   Wasm memory.

Cancellation and timeout paths use the same cleanup path before returning the
bounded result. SQLite watchdog threads have a shared completion flag and are
joined before the read-only connection is released. Workspace replacement clears
the PDF cache and retains only scratch leases held by in-flight operations.

The host-side review also covers saturation of evidence usage counters,
bounded catalog samples, clipped tool results, and the absence of unbounded
frontend polling or conversation session growth.

## Repeatable checks

The focused Python integration suite covers output capture, non-zero exits,
the SQL bridge, a blocked host-file read, bounded output, fuel exhaustion, and
user cancellation:

```sh
cargo test --manifest-path src-tauri/Cargo.toml --locked --test python_tool
```

The release memory probe runs a warm-up followed by 32 SQL-backed and
pure-Python executions:

```sh
./scripts/check-memory.sh
```

On Linux it compares the post-warm-up RSS at the end of the run with the
baseline and fails if growth exceeds 128 MiB by default. It also reports peak
guest linear memory. Set FELLA_MEMORY_ITERATIONS and
FELLA_MEMORY_MAX_GROWTH_MB for longer or tighter runs.

Observed release result on 2026-09-17: an extended 256-iteration run reached a
6.1 MiB peak guest store and Linux RSS grew 9.9 MiB from the post-warm-up
baseline. The probe passed the 128 MiB threshold.

This is a useful regression signal: a repeated Store or ABI allocation leak
should produce continuing guest or process growth. A stable result does not
prove the absence of leaks because allocators can retain arenas, the operating
system can defer reclamation, and non-Linux hosts need a native measurement.
For a platform investigation, use heaptrack or Valgrind on Linux, Instruments
on macOS, and Windows Performance Recorder/Analyzer or Application Verifier on
Windows. Run those against a release binary with the same probe workload.

## Release interpretation

The current personal release can responsibly describe Python as an embedded,
bounded, read-only analytics capability. It should describe the Wasmi/RustPython
versions and the checked-in guest artifact as part of the trusted computing
base. It should not claim that arbitrary native code is safely sandboxed until
the OS-worker acceptance tests in docs/PYTHON-SANDBOX.md pass on every
supported platform.

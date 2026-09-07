# Performance & size

Fella claims to be "small, fast, anti-bloat, <1s startup, low idle memory."
This file is how we hold that claim to account with actual numbers.

## Run it

```sh
cargo install cargo-bloat hyperfine   # one-time, no sudo, ~5 min each

./scripts/measure.sh                  # quick: sizes, deps, bundle, startup, memory (~2 min)
./scripts/measure.sh --bloat          # + which crates fill the binary (relinks it, ~10 min)
./scripts/measure.sh --build          # + time an edit-one-file rebuild
./scripts/measure.sh --build-cold     # + time a full clean rebuild (~20 min)
./scripts/measure.sh --min            # + build the size-minimised profile
```

Every run appends a dated block to the bottom of this file, so you can watch the
numbers move as the code changes. Already installed and used automatically:
`time`, `du`, `strip`, `size` (binutils), `cargo tree`, `cargo build --timings`.

## What each number means

| Number | What it is | Rough target |
|---|---|---|
| **binary size with symbols** | the `target/release/fella` file as built | |
| **binary size stripped** | the same file with debug info removed; **this is what actually ships** (Tauri's installer strips it) | low tens of MB is fine for an app that embeds DuckDB |
| **`size` text/data/bss** | machine code / initialised data / zeroed data sections | text dominates; watch its trend |
| **unique crates in the graph** | every third-party crate compiled into the build (`-e normal`, so runtime deps only) code to compile, audit and trust | fewer is better; adding a dependency adds to this |
| **duplicate versions** | the same crate pulled in at two versions wasted compile time and binary bytes | 0, or a small known list (a Tauri app always has a handful) |
| **cargo-bloat, by crate** (`--bloat`) | how many **bytes of the binary** each crate's code occupies. `libduckdb_sys` sits at the top that's the "does DuckDB earn its weight" question from `AUDIT.md`, now with a number | |
| **incremental rebuild** | change one file, `cargo build` again your dev feedback loop | seconds |
| **cold rebuild** | from `cargo clean`; dominated by DuckDB's C++ (~15 min). Only changes when dependencies change | one-time pain |
| **`cargo-timing.html`** | a Gantt chart of which crate took how long to compile. Open it in a browser | |
| **cold start → interactive** | milliseconds from launching the binary to the UI being ready, from the `fella: interactive in N ms` line the app prints | **< 1000 ms** once the OS disk cache is warm (i.e. not the very first launch) |
| **main process RSS** | resident memory the main process holds while idle. WebKit helper processes add more on top | idle around **100–150 MB** for the main process is healthy |
| **frontend bundle** | the JS/CSS the webview loads. Already ~190 KB uncompressed, ~60 KB gzipped | not a concern; the number's here to catch a regression |

## How to read it, as a beginner

- **Sizes**: `du -h` gives a human size; the script also prints exact bytes so a
  100 KB change is visible. The **stripped** number is the honest one.
- **`cargo bloat --crates`**: the `File` column is a share of the whole binary.
  If one crate is 40% of the binary, that's where a size cut would come from.
- **`hyperfine`**: runs a command several times and reports `mean ± σ`, so one
  slow run from a busy laptop doesn't mislead you. Without it the script falls
  back to a single `time` measurement.
- **`time -v`**: the line that matters is *Maximum resident set size* (peak RAM,
  in KB divide by 1024 for MB).
- **cold start**: the first launch after a build is always slow (cold disk
  cache); the 2nd and 3rd runs are the real number.

## Before / after the SQLite migration

| Metric | DuckDB default (`d057220`) | **SQLite default (`5b71f54`)** | change |
|---|---|---|---|
| **stripped binary** | 54 MB | **11 MB** | −80 % |
| with symbols | 67 MB | 18 MB | |
| `.text` (machine code) | 52 MB | **8 MB** | −85 % |
| unique crates (runtime) | 359 | 336 | −23 (a `--no-default-features` build drops ~36 more) |
| **cold `cargo build --release`** | ~9 min | **~3 min** | −65 % |
| idle RSS (main process) | 173 MB | ~177 MB | unchanged (webview-dominated; the win only shows with a large workspace loaded) |
| frontend bundle | 39 KB gzip | 39 KB gzip | |
| cold start → interactive | 942 ms / ~516 ms warm | *(probe unchanged; not re-captured WSLg display wedged after many launches this session)* | |

### What the binary is made of now (`--bloat`)

No crate dominates any more the sign of a small binary.

| Crate | Share of `.text` | Size | (was) |
|---|---|---|---|
| `[Unknown]` (std/tauri/generics) | 19.8 % | 1.6 MiB | 16.6 MiB (DuckDB C++) |
| `std` | 15.2 % | 1.2 MiB | |
| `tauri` | 11.5 % | 947 KiB | |
| `tokio` | 6.7 % | 548 KiB | |
| `fella_lib` | 5.4 % | 442 KiB | |
| `rustls` + `ring` (TLS) | 7.5 % | 611 KiB | `aws_lc_sys` 1.3 MiB |
| `lopdf` + `pdf_extract` (`--features pdf`) | 3.3 % | 271 KiB | |
| `calamine` (`--features xlsx`) | 4.0 % | 174 KiB | |
| `libsqlite3_sys` | 1.3 % | 110 KiB | (arrow_array alone was 290 KiB) |

`libduckdb_sys` + all 12 `arrow-*` + `lexical-*` + `comfy-table` + `crossterm` are
**gone** from the default build. `cargo build --features duckdb` brings them back
(~50 MB, ~15 min cold build) for Parquet / large-file speed.

(cargo-bloat's own caveat: "numbers are a result of guesswork… not 100 % correct".)

## Agent-loop latency (question → answer)

`measure.sh` covers *startup* and *size*. The other number a user feels is how
long a question takes. It is dominated by **model round trips**, not by Fella's
own code (a `run_sql` is ~10–50 ms; a model turn on a small local model is
seconds). The loop's job is to keep the round-trip count low and the model warm.

**What we do about it**

- **Keep the model resident.** Every Ollama request sends `keep_alive` (default
  `30m`, `FELLA_OLLAMA_KEEP_ALIVE`), and opening a workspace or changing the
  model fires a warm-up load, so a question doesn't wait 10–20 s for a cold
  reload. `FELLA_SKIP_MODEL_WARMUP` disables the warm-up (tests).
- **Size the context window.** `num_ctx` defaults to **8192**
  (`FELLA_OLLAMA_NUM_CTX`). Ollama's own default (2–4k) is smaller than Fella's
  prompt, so it silently truncates the schema or the question — which makes the
  model flail and *adds* round trips. Bigger `num_ctx` = more accurate but a
  slower first token and more RAM; 8192 is the balance for a low-spec box.
- **Bound generation.** `num_predict` / `max_tokens` default to **1024**
  (`FELLA_MODEL_MAX_OUTPUT`) — a tool call or a normal answer fits well under
  that; this only stops a runaway.
- **Fewer trips.** The system prompt ships the schema + sample rows and tells the
  model to go straight to `run_sql`, to batch independent lookups into one turn,
  and to stop as soon as it can answer. A turn's tool calls run **concurrently**.
- **Cheaper verify.** Cited queries are re-run once each (deduped), and an
  already-slow or truncated query is trusted rather than re-run.

**Measure it**

Run `pnpm tauri dev` with logging on and watch for these lines:

```
model response ← 200 streamed 47 chars in 3.1s        # one model round trip
agent step 1/20 done in 3.2s (1 tool call(s))
agent run: 8.4s, 2 model call(s), 1 tool call(s), 1 evidence
```

A simple question on a small dataset should be **one or two model calls** and
land in well under 30 s once the model is warm. If `agent run` shows 4+ model
calls for a simple question, the model is flailing — check that `num_ctx` is
large enough that the whole system prompt survives (grep the Ollama server log,
or raise `FELLA_OLLAMA_NUM_CTX`).

## Deeper tools (optional, need `sudo apt-get install`)

Not set up reach for these only when chasing a specific problem.

- **`bloaty`** byte-level breakdown of the binary by section and symbol, more
  detail than `cargo bloat`.
- **`heaptrack`** or **`valgrind --tool=massif`** heap allocation over time;
  use if idle memory looks wrong or grows.
- **`cargo flamegraph`** CPU profile of a hot path. Needs `perf`, which on WSL2
  usually needs a custom kernel; `valgrind --tool=callgrind` is the easier route
  there.

---

<!-- ./scripts/measure.sh appends dated runs below this line -->

## 2026-08-27 15:03  ·  commit 6f849bf

### Toolchain

rustc 1.98.0 (88d9e12ae 2026-08-18)
cargo 1.98.0 (797e8a9bc 2026-08-05)
v24.20.0 11.24.0
Linux 6.6.87.2-microsoft-standard-WSL2

### Dependencies

unique crates in the graph : 336
direct dependencies        : 18

duplicate versions (same crate at >1 version wasted size + build time):
bitflags v1.3.2
bitflags v2.13.1
cpufeatures v0.2.17
cpufeatures v0.3.0
flate2 v1.1.9
foldhash v0.2.0
getrandom v0.2.17
getrandom v0.3.4
getrandom v0.4.3
hashbrown v0.12.3
hashbrown v0.17.1 (*)
heck v0.4.1
heck v0.5.0
indexmap v1.9.3 (*)
indexmap v2.14.0 (*)
libc v0.2.189
log v0.4.34
miniz_oxide v0.8.9
proc-macro-crate v1.3.1 (*)
proc-macro-crate v2.0.2 (*)
quick-xml v0.37.5
quick-xml v0.41.0
semver v1.0.28
serde v1.0.229
serde_core v1.0.229
serde_json v1.0.151 (*)
serde_spanned v0.6.9 (*)
serde_spanned v1.1.1 (*)
simd-adler32 v0.3.10
smallvec v1.15.2
stable_deref_trait v1.2.1
syn v1.0.109
syn v2.0.119
syn v3.0.4
tauri-utils v2.9.3 (*)
thiserror v1.0.69 (*)
thiserror v2.0.20 (*)
thiserror-impl v1.0.69 (proc-macro) (*)
thiserror-impl v2.0.20 (proc-macro) (*)
time v0.3.55
time v0.3.55 (*)
toml_datetime v0.6.3 (*)
toml_datetime v1.1.1+spec-1.1.0 (*)
toml_edit v0.19.15 (*)
toml_edit v0.20.2 (*)
uuid v1.26.0 (*)
winnow v0.5.40
winnow v1.0.4

### Release build

    Finished `release` profile [optimized] target(s) in 0.28s
wall 0.32s · peak 120700KB

per-crate compile times: cd src-tauri && cargo build --release --timings
  then open src-tauri/target/cargo-timings/cargo-timing.html

### Binary size

with symbols : 18M  (18321976 bytes)
stripped     : 11M  (11529928 bytes)  <- what actually ships

     text	   data	    bss	    dec	    hex	filename
  11043726	 479120	  18072	11540918	 b019b6	src-tauri/target/release/fella

### Binary composition (cargo-bloat)

how many bytes of the binary each crate's code occupies:
 File  .text     Size Crate
 9.1%  19.8%   1.6MiB [Unknown]
 7.0%  15.2%   1.2MiB std
 5.3%  11.5% 947.0KiB tauri
 3.1%   6.7% 548.3KiB tokio
 2.5%   5.4% 442.4KiB fella_lib
 1.8%   4.0% 324.4KiB rustls
 1.6%   3.5% 287.3KiB ring
 1.1%   2.4% 193.9KiB lopdf
 1.0%   2.1% 174.2KiB calamine
 0.8%   1.7% 141.8KiB tauri_runtime_wry
 0.6%   1.4% 112.6KiB reqwest
 0.6%   1.3% 110.1KiB libsqlite3_sys
 0.5%   1.0%  83.4KiB serde_json
 0.5%   1.0%  81.4KiB hyper_util
 0.4%   1.0%  78.8KiB x11_dl
 0.4%   0.9%  77.0KiB pdf_extract
 0.4%   0.9%  75.2KiB muda
 0.4%   0.8%  66.2KiB http
 8.4%  18.4%   1.5MiB And 139 more crates. Use -n N to show more.
45.9% 100.0%   8.0MiB .text section size, the file size is 17.5MiB

Note: numbers above are a result of guesswork. They are not 100% correct and never will be.

### Frontend bundle

.svelte-kit/output/server/_app/immutable/assets/_page.DInR8-w1.css      5.99 kB │ gzip:  1.67 kB
.svelte-kit/output/server/entries/pages/_layout.ts.js                   0.15 kB │ gzip:  0.13 kB
.svelte-kit/output/server/env.js                                        0.22 kB │ gzip:  0.14 kB
.svelte-kit/output/server/entries/pages/_layout.svelte.js               0.23 kB │ gzip:  0.18 kB
.svelte-kit/output/server/chunks/env.js                                 0.28 kB │ gzip:  0.17 kB
.svelte-kit/output/server/internal.js                                   0.40 kB │ gzip:  0.19 kB
.svelte-kit/output/server/chunks/internal.js                            0.88 kB │ gzip:  0.43 kB
.svelte-kit/output/server/chunks/index-server.js                        4.77 kB │ gzip:  1.75 kB
.svelte-kit/output/server/entries/fallbacks/error.svelte.js             4.77 kB │ gzip:  1.76 kB
.svelte-kit/output/server/chunks/exports.js                             9.57 kB │ gzip:  3.06 kB
.svelte-kit/output/server/chunks/uneval.js                             17.25 kB │ gzip:  4.78 kB
.svelte-kit/output/server/entries/pages/_page.svelte.js                17.48 kB │ gzip:  4.64 kB
.svelte-kit/output/server/chunks/internal2.js                          21.40 kB │ gzip:  6.56 kB
.svelte-kit/output/server/chunks/shared.js                             29.89 kB │ gzip:  8.07 kB
.svelte-kit/output/server/chunks/utils.js                              37.45 kB │ gzip: 10.84 kB
.svelte-kit/output/server/remote-entry.js                              55.45 kB │ gzip: 12.13 kB
.svelte-kit/output/server/chunks/server.js                            130.49 kB │ gzip: 33.72 kB
.svelte-kit/output/server/index.js                                    134.75 kB │ gzip: 33.72 kB
✓ built in 1.96s
  Wrote site to "build"

build/ on disk  : 200K
all JS, gzipped : 39 KB

### Cold start -> interactive

(a window opens briefly for each run)
run 1: no timing line (no display / WSLg?)
run 2: no timing line (no display / WSLg?)
run 3: no timing line (no display / WSLg?)

### Idle memory

main process RSS : 177 MB   (+ 2 WebKit helper process(es), not summed)
  Elapsed (wall clock) time (h:mm:ss or m:ss): 0:08.02
  Maximum resident set size (kbytes): 184144

### Notes

- GUI metrics (cold start, memory) need a display WSLg on Windows.
- `time -v` "Maximum resident set size" is the main process only; WebKit
  helpers add ~20-60 MB more.
- `du --apparent-size` = file bytes, not blocks-on-disk.
- First `cargo build --release` is slow (DuckDB C++); later ones are fast.

## 2026-08-28 12:32  ·  commit d0dd675

### Toolchain

rustc 1.98.0 (88d9e12ae 2026-08-18)
cargo 1.98.0 (797e8a9bc 2026-08-05)
v24.20.0 11.24.0
Linux 6.6.87.2-microsoft-standard-WSL2

### Dependencies

unique crates in the graph : 337
direct dependencies        : 19

duplicate versions (same crate at >1 version wasted size + build time):
bitflags v1.3.2
bitflags v2.13.1
cpufeatures v0.2.17
cpufeatures v0.3.0
flate2 v1.1.9
foldhash v0.2.0
getrandom v0.2.17
getrandom v0.3.4
getrandom v0.4.3
hashbrown v0.12.3
hashbrown v0.17.1 (*)
heck v0.4.1
heck v0.5.0
indexmap v1.9.3 (*)
indexmap v2.14.0 (*)
libc v0.2.189
log v0.4.34
miniz_oxide v0.8.9
proc-macro-crate v1.3.1 (*)
proc-macro-crate v2.0.2 (*)
quick-xml v0.37.5
quick-xml v0.41.0
semver v1.0.28
serde v1.0.229
serde_core v1.0.229
serde_json v1.0.151 (*)
serde_spanned v0.6.9 (*)
serde_spanned v1.1.1 (*)
simd-adler32 v0.3.10
smallvec v1.15.2
stable_deref_trait v1.2.1
syn v1.0.109
syn v2.0.119
syn v3.0.4
tauri-utils v2.9.3 (*)
thiserror v1.0.69 (*)
thiserror v2.0.20 (*)
thiserror-impl v1.0.69 (proc-macro) (*)
thiserror-impl v2.0.20 (proc-macro) (*)
time v0.3.55
time v0.3.55 (*)
toml_datetime v0.6.3 (*)
toml_datetime v1.1.1+spec-1.1.0 (*)
toml_edit v0.19.15 (*)
toml_edit v0.20.2 (*)
uuid v1.26.0 (*)
winnow v0.5.40
winnow v1.0.4

### Release build

   Compiling tokio-macros v2.7.2
   Compiling fella v0.1.0 (<repo>/src-tauri)
   Compiling tokio v1.53.1
   Compiling hyper v1.11.0
   Compiling tauri v2.11.5
   Compiling tower v0.5.3
   Compiling tokio-rustls v0.26.4
   Compiling hyper-util v0.1.20
   Compiling hyper-rustls v0.27.9
   Compiling tower-http v0.6.11
   Compiling reqwest v0.13.4
   Compiling tauri-plugin-fs v2.5.1
   Compiling tauri-plugin-log v2.9.0
   Compiling tauri-plugin-dialog v2.7.2
    Finished `release` profile [optimized] target(s) in 1m 17s
wall 78.05s · peak 1133944KB

per-crate compile times: cd src-tauri && cargo build --release --timings
  then open src-tauri/target/cargo-timings/cargo-timing.html

### Binary size

with symbols : 18M  (18391888 bytes)
stripped     : 12M  (11583240 bytes)  <- what actually ships

     text	   data	    bss	    dec	    hex	filename
  11097587	 480288	  17992	11595867	 b0f05b	src-tauri/target/release/fella

### Binary composition (cargo-bloat)

(skipped pass --bloat; it relinks the LTO binary, ~10 min)

### Frontend bundle

.svelte-kit/output/server/_app/immutable/assets/_page.B4WMUur0.css      7.74 kB │ gzip:  1.97 kB
.svelte-kit/output/server/entries/pages/_layout.ts.js                   0.15 kB │ gzip:  0.13 kB
.svelte-kit/output/server/env.js                                        0.22 kB │ gzip:  0.14 kB
.svelte-kit/output/server/entries/pages/_layout.svelte.js               0.23 kB │ gzip:  0.18 kB
.svelte-kit/output/server/chunks/env.js                                 0.28 kB │ gzip:  0.17 kB
.svelte-kit/output/server/internal.js                                   0.40 kB │ gzip:  0.19 kB
.svelte-kit/output/server/chunks/internal.js                            0.88 kB │ gzip:  0.43 kB
.svelte-kit/output/server/chunks/index-server.js                        4.77 kB │ gzip:  1.75 kB
.svelte-kit/output/server/entries/fallbacks/error.svelte.js             4.77 kB │ gzip:  1.76 kB
.svelte-kit/output/server/chunks/exports.js                             9.57 kB │ gzip:  3.06 kB
.svelte-kit/output/server/chunks/uneval.js                             17.25 kB │ gzip:  4.78 kB
.svelte-kit/output/server/chunks/internal2.js                          21.40 kB │ gzip:  6.56 kB
.svelte-kit/output/server/entries/pages/_page.svelte.js                22.79 kB │ gzip:  6.25 kB
.svelte-kit/output/server/chunks/shared.js                             29.89 kB │ gzip:  8.07 kB
.svelte-kit/output/server/chunks/utils.js                              37.45 kB │ gzip: 10.84 kB
.svelte-kit/output/server/remote-entry.js                              55.45 kB │ gzip: 12.13 kB
.svelte-kit/output/server/chunks/server.js                            130.49 kB │ gzip: 33.72 kB
.svelte-kit/output/server/index.js                                    134.75 kB │ gzip: 33.72 kB
✓ built in 1.55s
  Wrote site to "build"

build/ on disk  : 208K
all JS, gzipped : 41 KB

### Cold start -> interactive

(a window opens briefly for each run)
run 1: no timing line (no display / WSLg?)
run 2: no timing line (no display / WSLg?)
run 3: no timing line (no display / WSLg?)

### Idle memory

main process RSS : 177 MB   (+ 2 WebKit helper process(es), not summed)
  Elapsed (wall clock) time (h:mm:ss or m:ss): 0:08.02
  Maximum resident set size (kbytes): 184688

### Notes

- GUI metrics (cold start, memory) need a display WSLg on Windows.
- `time -v` "Maximum resident set size" is the main process only; WebKit
  helpers add ~20-60 MB more.
- `du --apparent-size` = file bytes, not blocks-on-disk.
- First `cargo build --release` is slow (DuckDB C++); later ones are fast.

## 2026-08-30 04:43  ·  commit 67c3508

First measurement since the `mcp` connector feature landed (`bd54898`) and the
packs marketplace install path (`d19d0e0`, `f24c7e8`). Branch
`feat/providers-login-sql-timeout`.

### Toolchain

rustc 1.98.0 (88d9e12ae 2026-08-18)
cargo 1.98.0 (797e8a9bc 2026-08-05)
v24.20.0 11.24.0
Linux 6.6.87.2-microsoft-standard-WSL2

### Dependencies

unique crates in the graph : 385   (was 337 on d0dd675)
direct dependencies        : 26

### Release build

Default features (`pdf`, `xlsx`, `mcp`) the shipped app:

    Finished `release` profile [optimized] target(s) in 1m 39s (incremental)
    wall 99.49s · peak 1503940KB
    (from a d0dd675-era target dir the first rebuild was 3m 05s, peak 1621132KB)

### Binary size the shipped app (default: pdf + xlsx + mcp)

with symbols : 24M  (24396568 bytes)
stripped     : 16M  (16525672 bytes)  <- what actually ships

     text	   data	    bss	    dec	    hex	filename
  15812834	 708440	  20128	16541402	 fc66da	src-tauri/target/release/fella

Change since d0dd675 (12M stripped): +4.9 MB stripped, split below.

### Isolating the `mcp` feature cost

Same commit, `cargo build --release --no-default-features --features pdf,xlsx`
(everything except `mcp`), same toolchain and target dir only the flag differs:

| | no `mcp` (pdf+xlsx) | default (+ `mcp`) | `mcp` costs |
|---|---|---|---|
| stripped binary    | 14,668,584 B (14M) | 16,525,672 B (16M) | +1.86 MB  (+12.7%) |
| with symbols       | 22,075,112 B (22M) | 24,396,568 B (24M) | +2.32 MB |
| `.text`            | 13,998,542 B       | 15,812,834 B       | +1.81 MB |
| unique crates      | 379                | 385                | +6 |
| incremental relink | ~2m 10s            | ~3m 05s            | +~55s |

Crates `mcp` pulls that a no-`mcp` build does not: `rmcp`, `sse-stream`,
`tokio-stream`, `futures`, `chrono` (+ one transitive dep resolving to a second
version). The pluggable Streamable-HTTP transport means no reqwest 0.12 / quinn
the existing reqwest 0.13 client backs `FellaHttp: StreamableHttpClient`.

Of the +4.9 MB since 2026-08-28: ~1.9 MB is `mcp`, ~3.0 MB is the rest of the
branch (marketplace install + SHA-256 verification, `/connect`, markdown render).

A `--no-default-features` build (drops `pdf` + `xlsx` + `mcp`) is smaller still;
connector packs then report "this build has no connector support".

### Frontend bundle

nodes/2.CxHsvwP0.js (client)   89.32 kB │ gzip: 28.44 kB   (markdown render is new)
server/chunks/server.js       130.64 kB │ gzip: 33.76 kB
server/index.js               134.75 kB │ gzip: 33.72 kB
✓ built in ~1.6s

build/ on disk  : 276K   (was 208K)
all JS, gzipped : 60 KB   (was 41 KB the markdown renderer in Message.svelte)

### GUI metrics (cold start, idle memory)

Not re-measured no display under this shell (WSLg). Last known good
(d0dd675): main process RSS ~177 MB, no cold-start timing line.

### Notes

- The `mcp` number is a clean A/B on one commit: same toolchain, same target
  dir, only the feature flag differs.
- `cargo tree` name-dedup lists 5 added crates; the graph count moves by 6.

---

## 2026-09-03  ·  commit edba382  ·  v0.1 pre-flight

First capture with `strip = "debuginfo"` in `[profile.release]` (was `false`;
the deb/AppImage bundler does not strip, so the shipped binary carried ~5 MB of
DWARF nobody needs). Symbol table kept, so `cargo bloat` and backtraces still
work. Also the first recorded **installer** sizes.

### Binary

| | bytes | |
|---|---|---|
| release binary, `strip = false` (before) | 24,841,552 | 24 MB |
| release binary, `strip = "debuginfo"` (**ships now**) | 19,828,968 | 19 MB |
| fully stripped (`strip = true`, for reference) | 16,935,016 | 17 MB |

`opt-level=3`, `lto="thin"`, `codegen-units=1`, `panic="abort"`, default
features (`pdf`, `xlsx`, `mcp`). `calamine` is 0.36 here (bumped from 0.30 to
drop the vulnerable `quick-xml 0.37`).

### Installers  (Linux, `pnpm tauri build`)

| Artifact | bytes | |
|---|---|---|
| `Fella_0.1.0_amd64.deb` (was 9,029,176 with the unstripped binary) | 7,711,452 | 7.7 MB |
| `Fella_0.1.0_amd64.AppImage` | not measured here `xdg-open` unavailable in this shell; CI (`ubuntu-22.04`, has `xdg-utils`) builds it. Expect ~28-32 MB (bundles the ~20 MB binary + a squashfs runtime). |

`.dmg` / `-setup.exe` / `.msi` are matrix-only (`release.yml`); size them from
the first `v0.1.0-rc.1` draft.

### Dependencies

| | |
|---|---|
| unique crates in the graph | 386 (was 385; `calamine` bump net +1) |
| direct dependencies | 26 |
| duplicate-version crates | ~26 (53 tree lines) unaudited, mostly the Tauri stack |

### Frontend bundle

`build/` on disk: 452 KB. All JS+CSS gzipped: ~72 KB. `pnpm build` ~2 s.

### GUI metrics (cold start, idle RSS)

Still not captured no display in this shell (WSLg). Last known good
(2026-08-28, d0dd675): main-process RSS ~177 MB. **Must be measured on a real
display during the RC smoke test** (`docs/RELEASE.md` §1).

### `agent_bench` baseline

Captured 2026-09-04 (commit 69d46c4): `provider=ollama-cloud`
`model=gemma4:31b`, `num_ctx=8192`, `keep_alive=30m`, 3 iterations/question,
against the fixtures `examples/agent_bench.rs` builds (tiny/medium/large CSVs
+ notes, `agg_large` = 120k rows). Every question resolved in a single
tool-call round trip and passed full verification; `agg_large` is as fast as
`agg_tiny` because the model pushes the aggregation into `run_sql` instead of
reading rows itself.

| question | n | total s | model s | tool s | model calls | tool calls | verif |
|---|--:|--:|--:|--:|--:|--:|:-:|
| chitchat | 2 | 2.2 | 2.2 | 0.0 | 1.0 | 0.0 | 1/1 |
| agg_tiny | 2 | 1.4 | 1.3 | 0.1 | 2.0 | 1.0 | 2/2 |
| agg_medium | 2 | 1.2 | 1.2 | 0.1 | 2.0 | 1.0 | 2/2 |
| group_medium | 2 | 1.8 | 1.7 | 0.1 | 2.0 | 1.0 | 2/2 |
| multi_step | 2 | 1.3 | 1.3 | 0.1 | 2.0 | 1.0 | 2/2 |
| agg_large (120k rows) | 2 | 1.6 | 1.5 | 0.1 | 2.0 | 1.0 | 2/2 |
| doc_lookup | 2 | 2.2 | 2.2 | 0.0 | 2.0 | 1.0 | 1/1 |

(`n` = warm iterations averaged; cold-start deltas were all within ±0.9s of
warm, no meaningful cold penalty at this context size.)

Session-memory follow-up (same conversation, second question reuses the first
turn's context): turn 1 (fresh) 2.7s, turn 2 (follow-up) 2.7s no measurable
discount against a hosted cloud model network + inference time dominate over
the small context-reuse savings visible against a local model.

Run against **local** Ollama still not captured the numbers above are a
hosted-cloud model over the network, not the "local, private (default)" path
most users will actually run. Worth a second row here once measured on a real
machine with local Ollama and a comparable model size.

### `agent_eval` the scored harness

`agent_bench` times the loop; **`agent_eval` scores it** correctness,
answer-closeness, wasted tool calls, tokens per correct answer and sweeps
that across prompt ablations, folder sizes and models. Dev-only, behind the
`eval` Cargo feature, never run in CI.

```
cd src-tauri
AGENT_EVAL_DATA_DIR=/path/to/copied/data-dir \
  cargo run --release --features eval --example agent_eval -- <subcommand> [opts]
```

Subcommands: `accuracy`, `prompt-ablation`, `folder-scale`, `model-ladder`,
`robustness`, `session-memory`, `all`. Opts: `--models "a,b,c"`,
`--judge <model>` (opt-in LLM rubric on top of the deterministic closeness
score), `--only <id-substr>`, `--json <path>`, `--compare <old.json>` (Δ acc
/ tokens vs a prior run the "did my change help" answer).

A `--models` entry is a bare model on the configured provider, or
`provider/model` to switch provider too so one run can compare across
providers if the data dir's `auth.json` has each key:

```
--models "ollama-cloud/gemma4:31b,openai/gpt-5.6-luna,xai/grok-4.3"
```

Fixtures are deterministic (`engine::testkit`), so the golden answers are
exact. The grader matches the answer's **headline figure(s)** within
tolerance; multi-row tables and prose are not parsed. "number present" can
false-pass if the model prints the right value for the wrong reason
acceptable for a local dev tool.

The ~18-case battery spans question kinds, not just spending: a single
aggregate, a filter, min/max, a top-N group, a time-series month, a rounded
ratio, a two-figure multi-step, a `parse_num` trap (AVG over text amounts), a
cross-file total, a **non-financial** table (`workouts.csv`), two document
questions, an honest empty result (a category with no rows), a
needs-no-tool definition, and a must-decline "how much will I spend next
month".

**Methodology the battery is frozen.** The `EvalCase` list and every `gold`
value are the spec of what a good answer is you don't edit a case or its
expectation to make a number move (that's teaching to the test). A grader
(`grade()`) fix needs a trace of the model's actual behaviour and an argument
that it makes the grader match ground truth. Improve Fella against the
baseline, not the baseline against Fella.

#### Baseline 2026-09-07 (frozen battery, 18 cases, `--iters 5`)

`model-ladder --models "ollama-cloud/gemma4:31b,openai/gpt-5.6-luna,xai/grok-4.3" --iters 5`

| model | acc | close(det) | waste/case | tok/correct | $/100 | mean wall s |
|---|:-:|--:|--:|--:|--:|--:|
| ollama-cloud/gemma4:31b | 17/18 | 0.84 | 0.17 | 4249 | n/a | 1.5 |
| openai/gpt-5.6-luna | 17/18 | 0.84 | 0.00 | 3333 | $1.28 | 2.0 |
| xai/grok-4.3 | 16/18 | 0.80 | 0.06 | 4550 | $9.21 | 2.9 |

Read: three different models land within one case of each other on a frozen
battery, so headline accuracy has little room to move the harness work is
about holding that accuracy while cutting tokens and wasted calls, and about
where it breaks (folder scale, older models, traps). `luna` is the reference
row cheapest priced, zero waste, fewest tokens. Every harness change from
here is `--compare`d against this file's JSON (`/tmp/baseline.json`).

#### After 2026-09-07 (same frozen battery, `--compare` vs the baseline above)

| model | iters | acc | close(det) | waste/case | tok/correct | Δ tok/correct |
|---|:-:|:-:|--:|--:|--:|--:|
| ollama-cloud/gemma4:31b | 5 | 18/18 | 0.85 | 0.00 | 3650 | **−14%** |
| openai/gpt-5.6-luna | 5 | 18/18 | 0.87 | 0.00 | 3175 | −3% |
| xai/grok-4.3 | 3 | 16/18 | 0.82 | 0.06 | 4544 | ≈0 |

Per-case correctness is unchanged everywhere except `time_series` (a grader
fix all three models compute the right month, some render it "2021-11"). No
case regressed. grok stays 16/18: its baseline misses (`refusal` flaky,
`max_txn` low closeness) are unchanged and model-side, not harness.

**What moved it**

- **`run_sql` spells out an empty aggregate.** A `SUM`/`AVG` over no matching
  rows is one all-NULL row, which rendered as a blank cell; a smaller model
  read that as a failed query and fired 2-3 more calls to check the category
  existed. The tool result now says "nothing matched an empty SUM/COUNT is
  0". `empty_cat` on gemma: **4 tool calls → 1, ~10.3K tokens → ~3.9K**, still
  correct. It was luna's one baseline miss; now fixed. Smaller win on grok.
- **Three `verify.rs` precision fixes.** Making the self-check *actionable*
  (the corrective re-ask) turned three long-standing imprecisions from
  harmless fold-warnings into re-asks that cost a model call each: (a) a NULL
  aggregate isn't the number 0, (b) a float `SUM` re-serialises with a low bit
  different on re-run, (c) a digit inside an identifier (`txns_00`) was read
  as a stated figure. Each fixed the fold shows fewer bogus warnings now too.
- **The corrective re-ask is narrowed to the re-run checks.** Measured across
  all three models, triggering it on the fuzzy "a figure appears in no result"
  check was net-negative: on grok it turned a correct "≈18%" into the raw
  ratio `0.176…` (grounded, but wrong and unreadable), and it caused every
  false positive above. It now fires only on "different result now" / "no
  longer runs" a query that demonstrably changed, where "restate to match the
  re-run" is a safe tool-free fix. On a read-only workspace that's basically
  never, so the re-ask is effectively a dormant net rather than a live cost.
- **Prompt minimalism: tested, no change.** `prompt-ablation` on gemma every
  section above the core rules + schema either loses a case
  (`background_rule`), drives a shipped feature (`note_rule` the activity
  display), or is under-tested by the battery (`docs_rule`). Dropping the
  schema's sample rows sends waste from 0 to 11 (the model re-discovers what
  it was told). The shipped prompt is already at its Pareto point here.

#### Still queued

- **Few-shot** deferred: ablation shows no prompt slack to trade for it.
- **Folder-scale.** `folder-scale --models "ollama-cloud/gemma4:31b"` adaptive
  vs `fixed 8192`. If long runs flail on history bloat, then `trim_history`
  by token budget.
- **Older/cheaper models.** `model-ladder` down a dated list the point where
  bad SQL/arithmetic (model decay) overtakes a correct refusal (tool ceiling).

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

Every run appends a dated block to [`PERFORMANCE-LOG.md`](PERFORMANCE-LOG.md),
so you can watch the numbers move as the code changes. Already installed and
used automatically: `time`, `du`, `strip`, `size` (binutils), `cargo tree`,
`cargo build --timings`.

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


## Log

Every run of `./scripts/measure.sh` appends a dated entry to
[`PERFORMANCE-LOG.md`](PERFORMANCE-LOG.md) that file only grows; this one
stays a stable reference.

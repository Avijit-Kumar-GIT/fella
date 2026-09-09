# Harness comparison — how tiny can a local folder-QA loop be?

The thesis this benchmark exists to test:

> For the everyday questions people ask their own files (totals, trends,
> top-N, lookups, "what does this doc say"), how close does a **free,
> open, offline, 16 MiB** loop get to a well-funded data-analysis agent —
> and how far down the model ladder does it hold?

Fella is **not** a data-science agent: no charts, no ML, no cleaning
pipelines, no notebook. It answers questions, deterministically, and checks
the numbers it cites. The comparison is scoped to that.

## What runs against what

**Locked-harness protocol** (per *Stop Comparing LLM Agents Without Disclosing
the Harness*, arXiv:2605.23950): one runner drives every harness the same way,
on the same task set, with the same budget.

| harness | what it is | how it's driven |
|---|---|---|
| **Fella** | this repo's 6-tool read-only loop + verifier | `agent_eval bench --dir <d>` (default) |
| **OpenAI `code_interpreter`** | ChatGPT Advanced Data Analysis, via the Responses API | `agent_eval bench --dir <d> --harness openai-ci` |
| **Claude analysis tool** | Anthropic code execution, via the API | adapter — *not built yet* |
| **Julius AI** | funded data-analyst product ($11M, 2M+ users) | hand-run ~15 tasks, dot + error bar on the chart |

The `openai-ci` harness embeds each case's (small) files in the prompt and calls
`POST /v1/responses` with the `code_interpreter` tool. The key comes from
`<AGENT_EVAL_DATA_DIR>/auth.json` (`apikey:openai`) or `OPENAI_API_KEY`; base
URL from `OPENAI_BASE_URL`. It costs money — a code_interpreter session is
~$0.03 plus tokens, so run a subset (`--only`) on a cheap model first.

Models: `gemma4:31b → 12b → 4b`, `qwen3:8b`, one mid open model, one frontier
closed model as the ceiling reference.

## Task sets

Scored only on the simple-question tier:

- **`bench/folder-qa/`** — a hand-built 15-case battery in this repo: spending,
  workouts, a lease note; totals / filters / group-by / top-N / a trend / doc
  lookups / one refusal / one no-tool. Deterministic gold (`gen.py`). Runs on
  `gemma4` for free and is the shared set for the paid `openai-ci` pilot.
- **Fella's own frozen battery** (`agent_eval accuracy`) — the synthetic
  "messy personal folder" shape.
- **DABStep — *easy* tier** ([Adyen/HF](https://huggingface.co/spaces/adyen/DABstep),
  arXiv:2506.23719). Single dataset + minimal context, numeric/word answer.
- **InfiAgent-DABench** ([site](https://infiagent.github.io/), arXiv:2401.05507)
  — 257 questions over 52 real CSVs; use the single-file subset.

Not scored (run ~10 as a *capability-boundary* observation): DABStep hard,
DA-Code, anything needing plots / ML / multi-file joins. The interesting datum
there is that Fella *declines cleanly* while the funded tools produce something.

## `agent_eval bench`

```
AGENT_EVAL_DATA_DIR=/copy/of/fella.db+auth.json \
  cargo run --release --features eval --example agent_eval -- \
  bench --dir <bench-dir> --models "ollama-cloud/gemma4:31b,openai/gpt-5.6-luna" \
        --iters 3 --json out.json [--compare old.json] [--only <id-substr>]
```

Runs the JSONL battery through the same engine + metrics as `accuracy`, one
fresh workspace per case, and prints a per-case table plus `$/100` (list price,
from `price_per_100`). `--json` / `--compare` work as elsewhere.

### `<bench-dir>` layout

```
<bench-dir>/
  cases.jsonl        one JSON object per line; blank lines and `#` comments skipped
  payments.csv       the data files cases.jsonl names (paths relative to <bench-dir>)
  manual.md
  ...
```

### `cases.jsonl` schema

```jsonc
{
  "id": "dabstep-easy-0007",           // unique; used as the case id
  "question": "Total transaction amount for merchant X in 2024?",
  "files": ["payments.csv", "merchant_data.json"],   // staged into the workspace
  "gold": {"figures": [123456.78]},     // see below
  "tier": "easy",                       // optional; shown as the category
  "category": "aggregate",              // optional; used if tier absent
  "reference": "The total is $123,456.78."   // optional; for closeness scoring
}
```

`gold` shapes (map onto the internal `Gold` enum):

| JSON | meaning |
|---|---|
| `{"figures": [a, b, …]}` | every figure must appear in the answer, within ~1% |
| `{"approx": [value, abs_tol]}` | one figure within an absolute tolerance (ratios, exact 0) |
| `{"contains": ["Rent", "1250"]}` | each substring present (case-insensitive; bare int matches "1,250"); `"a\|b"` = either |
| `"refusal"` | the answer must decline and cite no figure |
| `"notool"` | the answer must need no tool call |

## Converting the public sets

`bench/convert.py` (stdlib only) turns a download into `cases.jsonl`:

```
python3 bench/convert.py dabstep  <tasks.jsonl> <context_dir> > bench/dabstep-easy/cases.jsonl
python3 bench/convert.py infiagent <da-dev-questions.jsonl> <csv_dir> > bench/infiagent/cases.jsonl
```

then copy the data files next to the `cases.jsonl`. It filters DABStep to the
easy tier, folds `guidance`/`format` into the question, and parses gold into
`{"figures":[…]}` / `{"contains":[…]}`. The field names come from each set's
published schema — eyeball the first few rows after fetching, since neither
download was available to test against here.

## The chart (once the adapters land)

- **A** — accuracy (combined easy circuit) vs **tokens per correct answer**,
  log x. One line per harness, points = models. Fella's line bottom-left.
- **B** — capability matrix: rows = task types, cells ✓ / ✓verified / declines / ✗.
- **C** — the tiny-and-open table: binary size, deps, offline?, license,
  $/1k-answers. The funded tools leave cells blank.

## Running the pilot

```
DATA=/copy/of/fella.db+auth.json

# Fella down the model ladder (gemma is free) — the primary experiment
AGENT_EVAL_DATA_DIR=$DATA cargo run --release --features eval --example agent_eval -- \
  bench --dir bench/folder-qa --models "ollama-cloud/gemma4:31b" --iters 3 --json fella-gemma.json

# OpenAI code_interpreter on the same set (costs ~$0.03/case in sessions) —
# start with a subset on a cheap model
AGENT_EVAL_DATA_DIR=$DATA cargo run --release --features eval --example agent_eval -- \
  bench --dir bench/folder-qa --harness openai-ci --models "openai/gpt-5.6-luna" \
        --iters 1 --json ci-luna.json --only fqa-total
```

`BENCH_PAUSE_MS` (default 400) spaces cases so a hosted endpoint doesn't
degrade mid-run; `--iters 3` majority-votes over transient flakiness.

**Don't run two `bench` (or `accuracy`/`model-ladder`) processes against the
same `AGENT_EVAL_DATA_DIR` at once** — they share `fella.db` and race on the
provider/model row, so requests go to the wrong endpoint and every SQL call
errors. Put all models in one `--models "a,b,c"` list (they run in series) or
use a separate data-dir copy per process. `--harness openai-ci` is exempt (it
never writes settings).

## Status

- [x] `bench --dir` subcommand + JSONL loader + `$/100` column + unit test
- [x] `--harness openai-ci` — Responses API + `code_interpreter`, same grader
- [x] `bench/folder-qa/` — 15-case hand battery + `gen.py`
- [x] `bench/convert.py` — DABStep-easy / InfiAgent-DABench → `cases.jsonl` (untested vs a live download)
- [ ] Claude-analysis adapter (`--harness claude-analysis`)
- [ ] fetch DABStep-easy + InfiAgent, run the converters
- [ ] the full pilot run + the 3-panel chart

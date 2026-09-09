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
| **Fella** | this repo's 6-tool read-only loop + verifier | `agent_eval bench` (below) |
| **OpenAI `code_interpreter`** | ChatGPT Advanced Data Analysis, via the API | adapter — *not built yet* |
| **Claude analysis tool** | Anthropic code execution, via the API | adapter — *not built yet* |
| **Julius AI** | funded data-analyst product ($11M, 2M+ users) | hand-run ~15 tasks, dot + error bar on the chart |

Models: `gemma4:31b → 12b → 4b`, `qwen3:8b`, one mid open model, one frontier
closed model as the ceiling reference.

## Task sets

Scored only on the simple-question tier:

- **Fella's own frozen battery** (`agent_eval accuracy`) — the "messy personal
  folder" shape nobody else has.
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

Both ship a manifest + data. A small converter (Python is fine, keep it out of
the Rust) writes `cases.jsonl`:

- **DABStep**: each task has `question`, `level` (→ `tier`), `guidance`
  (formatting), and a shared `context/` dir of CSV/JSON + `manual.md`. Gold
  answers grade with adaptive numeric tolerance → `{"figures": […]}` for
  numbers, `{"contains": […]}` for strings/multiple-choice. Point every
  easy-tier case's `files` at the shared context files it needs.
- **InfiAgent-DABench**: each question names one CSV and a
  `format` + `answer` (often `@key[value]` pairs). One CSV per case →
  `"files": ["<that>.csv"]`; parse the expected values into `figures` /
  `contains`.

## The chart (once the adapters land)

- **A** — accuracy (combined easy circuit) vs **tokens per correct answer**,
  log x. One line per harness, points = models. Fella's line bottom-left.
- **B** — capability matrix: rows = task types, cells ✓ / ✓verified / declines / ✗.
- **C** — the tiny-and-open table: binary size, deps, offline?, license,
  $/1k-answers. The funded tools leave cells blank.

## Status

- [x] `bench --dir` subcommand + JSONL loader + `$/100` column + unit test
- [ ] Python converters for DABStep-easy and InfiAgent-DABench
- [ ] `code_interpreter` / Claude-analysis adapters (a `Harness` seam in `run_case`)
- [ ] first pilot run + the chart

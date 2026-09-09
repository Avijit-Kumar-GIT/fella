# Harness benchmark — does the loop earn its place, and how tiny can it be?

Fella's philosophy: **something powerfully tiny that reads your files.** The
people who'll use it aren't comparing it to ChatGPT or Julius — they use it
because it's fun, fast, local, and cheap/open on principle. So the benchmark
isn't a bake-off against funded products. It answers two questions:

1. **Does the harness lift the model?** — `bare` (model + files in the prompt,
   no tools) vs `fella` (the full loop). `Δacc = acc(fella) − acc(bare)` is the
   number. If Δ→0 on a model, the loop isn't earning its place *for that model*.
2. **How far down the model ladder does the lift hold?** — run both conditions
   across a ladder of cheap/small models; find where the model, not the
   scaffold, becomes the bottleneck.

Scoped to the everyday-question tier Fella targets — numeric and categorical
lookups, free-text search, and small multi-file joins across a person's own
files — not multi-step data-science workflows.

## First-class metrics

Always shown, and what rolls up to a chart:

| metric | what it tells you |
|---|---|
| **acc** = correct / n (majority over `--iters`) | headline |
| **consistency** = fraction of cases correct on *all* k iters | right 3/5 ≠ right 5/5 |
| **Δacc vs bare** | the harness's lift, per model — *the thesis number* |
| **tok / correct** (in + out) | efficiency |
| **$ / correct** | the money version (list price) |
| **wasted calls / case** | the loop's own overhead; small models flail here |
| **round trips / answer** | provider-independent latency proxy (Fella targets ≤ 2) |
| **wall s** | reported, caveated (network + provider load) |
| **self-catch rate** | of the wrong answers, how many `verify` flagged — *Fella-only*, and the trust story |
| **policy adherence** | correctly declines the forecast case vs fabricates a number |

`bare` has no evidence trail, so self-catch / grounding / waste are 0 for it by
construction — that contrast is the point.

## The battery

**`bench/folder-qa/`** — 24 hand-built cases across five life domains, so it
isn't a finance benchmark: spending + budget, workouts (minutes and km), a
reading log + an authors table, trips, and a plain-text journal. Question
shapes: numeric aggregate / filter / average / multi-step, categorical
group-by / top-N / distinct-count, boolean filter, free-text lookup / count /
search, one refusal (no forecasting), one no-tool, and a 4-case multi-file
tier where each needs a JOIN or a table + a document (two of them non-finance:
pages by British authors = books ⋈ authors; minutes on journal-noted run days
= journal ∩ workouts). Deterministic gold (`gen.py`, stdlib). Runs free on
`gemma4:31b` and any ollama-cloud model.

## Running it

```
DATA=/copy/of/fella.db+auth.json

# bare baseline, down the ladder (free on ollama-cloud models)
AGENT_EVAL_DATA_DIR=$DATA cargo run --release --features eval --example agent_eval -- \
  bench --dir bench/folder-qa --harness bare \
        --models "ollama-cloud/gemma4:31b,ollama-cloud/deepseek-v4-flash:0731,ollama-cloud/glm-5.3-flash" \
        --iters 3 --json bare.json

# same ladder, full Fella loop
AGENT_EVAL_DATA_DIR=$DATA cargo run --release --features eval --example agent_eval -- \
  bench --dir bench/folder-qa --harness fella --models "…same…" --iters 3 --json fella.json

# Δacc = per-model acc(fella) − acc(bare)
python3 bench/aggregate.py "bare / gemma=bare.json:ollama-cloud/gemma4:31b" \
                           "fella / gemma=fella.json:ollama-cloud/gemma4:31b" --prices
```

- `BENCH_PAUSE_MS` (default 400) spaces cases so a hosted endpoint doesn't
  degrade mid-run; `--iters 3` majority-votes over transient flakiness.
- **Don't run two `bench` processes against the same `AGENT_EVAL_DATA_DIR` at
  once** — they share `fella.db` and race on the provider/model row. One
  `--models "a,b,c"` list runs in series.

## Model ladder (revealed-preference cheap models, Sept 2026)

From OpenRouter usage rankings + pricing. **ollama-cloud lists many models but
gates most behind credits** — on a plain key only these four are actually free:
`gemma4:31b`, `gpt-oss:20b`, `gpt-oss:120b`, `nemotron-3-nano:30b`. Everything
else is OpenRouter (one key).

| model | OR $/1M in–out | free tier |
|---|--:|:-:|
| GPT-OSS 20B | ~free | ollama-cloud ✅ |
| Nemotron 3 Nano 30B | ~free | ollama-cloud ✅ |
| Gemma4:31b | ~0.10 / 0.20 | ollama-cloud ✅ |
| GPT-OSS 120B | ~free | ollama-cloud ✅ |
| DeepSeek V4 Flash (0731) | 0.05 / 0.16 | OpenRouter |
| GLM 5.3 Flash | 0.075 / 0.25 | OpenRouter |
| Nemotron 3.5 Lightning | 0.08 / 0.20 | OpenRouter |
| GPT-5.6 Luna | 0.20 / 1.20 | OpenAI direct |
| Gemini 3.8 Flash | 0.75 / 3.75 | OpenRouter |
| Muse Glimmer 30B | 0.30 / 1.10 | OpenRouter |
| Inkling Small (Thinking Machines) | 0.45 / 1.20 (`:free` exists) | OpenRouter |
| Grok 4.3 | 1.25 / 2.50 | xAI direct |
| Muse Spark 1.3 | 1.25 / 4.25 | OpenRouter |

Full ladder × {bare, fella} × `--iters 3` on the 24-case battery ≈ **$1** via
OpenRouter (4 rungs free on ollama-cloud); ~half of the paid part is Grok +
Muse Spark.

## Not in scope

Comparison against funded data-analysis harnesses (ChatGPT code_interpreter,
Claude analysis, Julius). The `--harness openai-ci` adapter stays in the code
for anyone who wants it, but on this task tier every execution harness saturates
near the top on accuracy — the interesting axes are efficiency, cost, locality,
and trust, which `bare` vs `fella` already covers. DABStep's published
smolagents baseline is the number to cite if an external anchor is ever needed.

## Status

- [x] `bench --dir` + JSONL loader + `$/100`
- [x] `--harness bare` — the no-harness baseline (`EngineState::ask_once_usage`)
- [x] `--harness openai-ci` — kept, not the focus
- [x] `bench/folder-qa/` battery + `gen.py`; `bench/convert.py` (DABStep/InfiAgent, untested)
- [x] `bench/aggregate.py` — roll dumps into a comparison table
- [x] `aggregate.py --lift` — pair a bare + fella dump, emit Δacc per model
- [ ] the ladder run + the chart

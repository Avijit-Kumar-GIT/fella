# Roadmap

A wish-list, not a commitment. Everything here is scoped to **not grow the base**:
each item reuses a dependency Fella already ships, or is frontend-only. Ideas that
would need a positioning decision are in the last section, kept separate on purpose.

Fella's locked shape still applies to all of it: personal analytics for
non-developers, read-only, local-first, every answer a deterministic computation
that shows its working. See [`DECISIONS.md`](DECISIONS.md) and
[`EXTENSIBILITY.md`](EXTENSIBILITY.md).

## Answer trust, the "shows its working" promise

- **Copy an answer with its evidence.** One button copies the reply plus the exact
  queries, row counts and file names as Markdown. *(frontend only)*
- **Re-run a past answer against current data.** A button on an old reply
  re-executes its cited queries and flags what moved since. *(reuses
  `verify::rerun_queries`)*
- **Data-caveats line.** Surface the ingest notes Fella already records ("amounts
  were stored as text", "a trailing totals row was dropped") above the answer, not
  only inside the evidence fold. *(existing `ColumnInfo.note` / `SourceInfo.note`,
  plus frontend)*
- **Plain-language query gloss.** One English sentence per cited query in the
  evidence panel, derived from the SQL. *(string work, no dependency)*
- **Units from `fella.md`.** Let the workspace context declare "amounts are GBP" or
  "weights in kg" so answers render `£4,850` and `72 kg`. *(formatting only)*

## Working with the data

- **Union same-shape files.** `statement-2023.csv` and `statement-2024.csv` with
  identical columns, offered as one table `statements`. *(ingest logic plus SQLite;
  the "five years of bank statements" case)*
- **`.ods` spreadsheets.** `calamine` already reads OpenDocument; wire it in next to
  `.xlsx`. *(no new dependency)*
- **Simple personal formats.** `.srt` / `.vtt` subtitles and `.ics` calendars as
  line-parsed structured sources; `.eml` as text with a parsed header. *(tiny
  hand-written parsers)*
- **Markdown tables as data.** Pipe tables inside a `.md` note, offered as queryable
  rows. *(`regex` plus the existing ingest path)*
- **`/describe <table>.<column>`.** A text summary of one column on demand: min,
  max, quartiles, top values, null rate. *(reuses `run_sql`)*
- **Relative-date vocabulary.** Teach the prompt "YTD", "last quarter", "trailing 12
  months" and map them to `strftime` / `date`. *(prompt only)*

## Model and connection UX

- **Speed hint in `/model`.** Tag each model fast or slow from a one-shot probe.
  *(reuses `provider_health` timing)*
- **Per-question model override.** "Use the bigger model for this one" without
  changing the global setting. *(settings plumbing exists)*
- **Retry with another model.** A button on a weak or failed answer to re-ask
  elsewhere. *(reuses the ask path)*
- **Run readout.** A small `2 steps, 4.2 s, llama3.1` line under each answer. The
  agent already logs this; it just needs surfacing. *(one event field plus
  frontend)*
- **Model-ready indicator.** The status line shows loading versus ready, from the
  warm-up state that already exists. *(frontend plus a flag)*

## Discoverability, for an audience that does not read docs

- **Catalog-aware example questions.** The empty screen suggests questions built
  from the actual files: "you have `workouts.csv`, try 'average pace by month'".
  *(reuses the catalog)*
- **"What can I ask about this?"** Click a table in the catalog and the model
  proposes three or four answerable questions. *(reuses the ask path)*
- **`/whatsnew`.** Render the in-repo `CHANGELOG.md` in the app.
- **One example per command.** Extend the completion menu (`completionsFor()`) to
  show a sample invocation, not just the argument list.

## Extensibility that ships as words, not code

- **Starter skills.** Vetted Markdown packs for personal-finance, fitness and
  health-export vocabulary. Zero code, zero size, real value for non-developers.
- **A starter theme or two.**
- **Vetted-connector list in `/connect`.** Even before the marketplace is live,
  `/connect` with no argument can name the connectors the core team has reviewed.
- **Skill and theme scaffold.** A template and a short "how to write one" so
  contributors can add them without touching the base.
- **Hosted pack marketplace.** The in-app `/packs` flow works today: local
  `/packs add`, and by-id `/packs install` with hash-checked downloads against
  the `fella-extensions` catalog. `fella-extensions` is public and now carries
  the `packs/_template/<kind>/` scaffold + `docs/WRITING-A-PACK.md`; the
  `fella-web` browse page is built and its copy is ready. What remains: choose
  starter packs, then flip the `fella-web` `/packs` route on (sync the catalog
  snapshot, drop the two `_redirects` lines, add the nav link) and point the
  app's `MARKETPLACE_URL` at the site. The install-counter proxy
  (`fella-web/packs-proxy/`) stays deferred until counts are wanted. See
  [`DECISIONS.md`](DECISIONS.md), 2026-09-02 and 2026-09-09.

## Harness quality, measured

`examples/agent_eval` (dev-only, `--features eval`) now scores the agent loop
correctness, answer-closeness, wasted tool calls, tokens/correct-answer
across prompt ablations, folder sizes and models (`docs/PERFORMANCE.md`; the
running log and the design rationale are in `docs/HARNESS.md`). Decide from its
output, not from a hunch. The floor model is the **`gemma4` series** an
optimisation that costs it isn't taken.

- **Per-folder playbook memory** *(shipped 2026-09-08, `engine::memory`).* A
  session in a folder starts already knowing that folder this user's
  vocabulary, caveats from last time carried forward. A small learned context
  block beside `fella.db`, per-folder, never leaving the machine — no new
  dependency. See [`HARNESS.md`](HARNESS.md#shipped) and
  [`FOLDER-MEMORY.md`](FOLDER-MEMORY.md).
- **Case-mismatch on a uniformly-cased column** *(next, unfixed)* and
  **semantic near-duplicate category labels** *(next, no candidate fix yet)*
  two distinct gaps a new benchmark pass surfaced 2026-09-12, neither caught
  by the shipped case-sensitivity flag (which only fires on a real collision
  in the data, not a model-invented wrong-case literal against clean data).
  gemma4:31b: 5/9 on the new messiness axis, the worst of ten axes measured.
  See [`HARNESS.md`](HARNESS.md#next).
- **Correction-trigger breadth** *(open question, unmeasured).*
  `memory::is_correction()` only recognises a fixed marker-word list; whether
  it needs to recognise more natural phrasings hasn't been tested, only
  assumed. See [`HARNESS.md`](HARNESS.md#next).
- **Few-shot worked examples** in the system prompt for small local models
  *deferred:* `prompt-ablation` on gemma4:31b (2026-09-07) shows no prompt slack
  to trade the shipped prompt already loses a case or a feature at every cut
  above the core rules + schema.
- **Terser prompt for `o1`/`o3`/`o4`/`gpt-5*`** a second `PromptProfile` preset,
  gated on the eval agreeing. gpt-5.6-luna is already the leanest of the three
  measured models on the shipped prompt, so this is low priority.
- **Corrective re-ask on `verify::hard_fail`** *(shipped 2026-09-07, default on,
  `FELLA_VERIFY_REASK=0` to disable; [`DECISIONS.md`](DECISIONS.md) amended).* One
  tool-free turn to reconcile or withdraw a figure whose query no longer
  reproduces it. Confirmed on the frozen battery: after fixing three `verify`
  imprecisions it exposed (NULL-vs-0, float jitter, identifier digits) it fires
  on nothing there a dormant net at zero cost. Real value needs a genuine
  unbacked figure, which the battery's models don't currently produce.
- **`trim_history` by token budget** if `folder-scale` shows long multi-step
  runs flailing on history bloat (`num_ctx` is already a growing floor).

## Small engine and UI niceties

- **Streaming Markdown that does not flicker.** Render partial tables and lists
  cleanly as tokens arrive. *(frontend, reuses `markdown.ts`)*
- **Keyboard-first.** `/` focuses the composer, `j` and `k` move through answers,
  `?` opens help. *(frontend only)*
- **Mid-run progress for slow queries.** "This query is taking a while" once a query
  passes a few seconds. *(`FELLA_QUERY_TIMEOUT_SECS` already exists)*
- **Cancel one tool call**, not only the whole run. *(frontend plus a flag)*
- **Pinned questions.** Star an answer; it re-runs on open, so a recurring question
  ("this month's spend") shows its current value. *(existing SQLite DB and query
  path)*
- **History search.** `/history <term>` greps the `conversations/` archive Fella
  already writes. *(`regex` plus `walkdir`)*

## Would need new infrastructure

Not a positioning question these don't touch a locked constraint, and the
direction is already endorsed by the harness work itself. Recorded separately
because each needs real tooling that doesn't exist yet, so "reuses a
dependency Fella already ships, or is frontend-only" (this doc's own scoping
rule, above) doesn't apply to them.

- **Distill a small model on Fella's own verified tool-use traces.** The eval
  harness (`examples/agent_eval`, `bench/`) now generates exactly what this
  needs: real recorded tool-call trajectories, plus a grader that already
  knows which ones were actually correct. The idea: run the harness (or real
  sessions) at volume, keep only the verified-correct trajectories, and
  fine-tune a small open-weight model (gemma-class) on them so habits Fella
  currently has to prompt for every time such as case-insensitive text
  matching, checking a suspicious empty result before reporting it, not
  over-calling tools get baked into the model's weights instead. This is the
  concrete mechanism for the longer-standing "a tiny model native to Fella"
  idea (the s1-mini-style exploration from earlier), not a separate proposal.

  Not close to a same-session task. Needs real training infrastructure
  (LoRA/QLoRA is the realistic approach for a model this size — no full
  fine-tune), and needs far more, and more *varied*, verified trajectories
  than the ~150 hand-built benchmark cases currently produce — real folder
  shapes and phrasings, or the fine-tune risks memorising the test fixtures
  instead of learning the general habit. The grading rigor this session's
  benchmark work put in (catching several false-positive/false-negative
  grading bugs before trusting a result) is exactly the trustworthy "was this
  actually right" filter this depends on — that hardening work is the real
  prerequisite already done, not effort spent only on reporting scores.

## Would need a positioning decision

Recorded, not planned. Each touches a locked constraint; picking one up means
amending [`DECISIONS.md`](DECISIONS.md) first, the way the 2026-08-29 entry amended
the "No MCP" non-goal.

- **Web search and fetch tools.** `web_search` plus `web_fetch`, off by default,
  key-gated, so the model can answer what the folder cannot. Touches "the base
  makes one network call, to the model the user chose" and grows the fixed tool
  set. No new crate needed (`reqwest` plus `serde`), though turning HTML into text
  wants a hand-rolled stripper or a small parser.
- **Export a result.** Save a query result as CSV or Parquet. Against "Fella
  produces answers, not files" and "no generated artifacts"
  ([`AUDIT.md`](AUDIT.md), [`ARCHITECTURE.md`](ARCHITECTURE.md)). Distinct from
  the 2026-09-10 `augment` amendment: an augment saves a file **you** type into
  a tab; this item is the agent's own *computed output* becoming a file, which
  is still undecided.
- **A new `augment` capability.** `engine/augment.rs`'s `CAPABILITIES` is
  `buffer` and `grid` only. A third capability is app code shipped to every
  install regardless of who uses it, so it's gated exactly like a new built-in
  tool or the item below: a GitHub issue, real demand, and a `DECISIONS.md`
  entry first (2026-09-10) not something a pack idea gets to add on its own.
- **stdio MCP transport.** Connect local-subprocess MCP servers, not only remote
  HTTP ones ([`DECISIONS.md`](DECISIONS.md), 2026-08-29 deferred this). Pulls a few
  more `rmcp` transport crates, the only item here with a real, if modest, size
  cost.
- **Local embeddings and semantic doc search.** `LlmClient::embed()` and the
  `embeddings` provider fields are dormant infra; the index-free `grep_files` /
  `read_file` design was a deliberate 2026-08-29 choice. Reviving embeddings adds
  an index step and a vector store.

## Related

General-knowledge answers, when the files genuinely cannot help (clearly marked
"not from your data", with no measured figures), are a smaller change than the web
tools and do not need the network-guarantee reversal. Not yet built.

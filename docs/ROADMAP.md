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
  the `fella-extensions` catalog. Held until there's demand for customisation:
  deploy the browse site (`fella-web/marketplace/`), make `fella-extensions`
  public with real packs, the install-counter proxy (`fella-web/packs-proxy/`),
  and the scaffold + tutorial above. See [`DECISIONS.md`](DECISIONS.md),
  2026-09-02.

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
  ([`AUDIT.md`](AUDIT.md), [`ARCHITECTURE.md`](ARCHITECTURE.md)).
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

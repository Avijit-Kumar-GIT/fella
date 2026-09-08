# Per-folder memory

GitHub #42. Constraints from `WHY.md`: local, auditable, no new dependency,
small enough that one person can read all of it.

**Status: v1 built** on `feat/folder-memory` (`engine::memory`). What's below is
the full design; the [Implementation](#implementation-v1) section at the end
says what v1 does, what it defers, and what the first benchmark showed.

## The goal

A session in a folder should start already knowing that folder. Today every
session is cold: the model re-derives which table is which, what the user's
words mean, and the exact SQL that copes with this folder's mess — every time.

## What's worth remembering (and what isn't)

| Keep | Why | How it grows |
|---|---|---|
| **Schema semantics** — what a table/column *means*, its grain, a cryptic code's values, "`amount` is text, cast it", "`rent.csv` has a totals row" | The model shouldn't re-learn the shape of the data each session | with **file/column count** |
| **Vocabulary** — "my rent" → `category IN ('rent','housing')`; "spending" excludes transfers; "last quarter" starts in April | The user's words don't match the data's words; this is learned from corrections | slowly; ~dozens over a folder's life |
| **Recipes** — a question shape → the SQL that *verified* for it before (esp. the messy-column `CAST`/`REPLACE` incantation) | A weak model re-deriving `parse_num` logic every time is where it burns tokens and sometimes gets it wrong | with **distinct question shapes** |
| **Preferences** — GBP, round to whole pounds, no `Background:` line | Presentation, set once | fixed, tiny |
| **Corrections** — "gym is under `health`, not `fitness`" | Must durably override the model's guess | slowly |

**Don't keep:** computed figures or prior answers. Fella recomputes
deterministically every time; a cached total is a stale total and breaks the
"every number is fresh arithmetic" contract. Cache the *question → query*, never
the *result*.

## Episodic and semantic

Two layers, the way most cognitive-memory designs split it:

- **Episodic** — a capped, timestamped log of *what happened*: the question
  asked, the query that ran, the answer reached, any correction. Append-only,
  rotated (keep ~the last N sessions or ~M events). Cheap to write (it's the
  evidence Fella already produces). Used for "what did we conclude last time
  about X", as the raw material the semantic layer is distilled from, and as
  the audit trail for *why* a semantic fact exists.
- **Semantic** — the curated `memory.md`: vocabulary, schema notes, preferences,
  validated recipes. Distilled from episodic + ingest + corrections; deduped;
  superseded on change. This is the small, stable, always-useful part.

Semantic is **always-on and small**; episodic is **recall-only** (below). A
correction writes to both — an episode ("user said gym is under health") and a
semantic note (the mapping). The optional end-of-session pass is what promotes
recurring episodes into semantic notes.

## Does size actually make this a retrieval problem?

Partly. Rows don't matter — memory never looks at row data. What scales is
**schema notes** (∝ columns) and **recipes** (∝ question variety). A folder with
40 tables and a year of use could plausibly reach ~800 column-notes and ~50
recipes — low tens of KB. Too much to prepend in full every turn (and doing so
is the *add-scaffolding* anti-pattern from `HARNESS.md` — it taxes the strong
model on questions that don't need it).

But it is **not** a vector-DB problem:

- Schema notes aren't retrieved by similarity — they attach to the **catalog**
  and ride along with the schema block Fella already sends, tiered the same way
  (`folder-scale` showed the tiering degrades gracefully past ~13 tables). Full
  notes → names+notes → names-only. Bounded by schema size, which is already
  handled.
- Recipes and vocabulary are keyword-shaped ("rent", "monthly", a table name).
  **SQLite FTS5** (already bundled) does the selection. A question mentioning
  "rent" pulls the rent-related notes and recipes. No embeddings, deterministic,
  inspectable.

So the "hard part" of agent memory — semantic retrieval over a large corpus —
mostly dissolves: the big part (schema) isn't retrieved, and the retrieved part
(recipes/vocab) is small and lexical.

## Storage: a `MEMORY.md`-style artifact, files as truth

One human-readable file per folder, sections, index-style if it grows:

```
memory.md                # index + the always-on core (prefs, vocab, top recipes)
  └─ (spills to)  notes/schema.md   notes/recipes.md   when a section gets long
```

```markdown
# What Fella has learned about this folder
<!-- edit or delete anything here. Fella reads this; it won't overwrite your
     edits without asking. Delete the file to make Fella forget. -->

## Always applies
- Amounts are GBP; round to whole pounds.
- "spending" excludes rows where category = 'transfer'.

## Vocabulary
- "rent"  → category IN ('rent','housing')     [from a correction, 2026-09-06]
- "gym"   → category = 'health'                [corrected 2026-09-07]

## Tables
- txns_2021..2025 : monthly bank exports, same schema. `amount` is text ($1,200) — cast it.
- rent.csv        : trailing totals row (Fella drops it on load).

## Recipes  — SQL that verified before; adapt to the question, don't paste blindly
- monthly spend by category | used 7x, last 2026-09-07
    SELECT strftime('%Y-%m',date) m, category,
           SUM(CAST(REPLACE(REPLACE(amount,'$',''),',','') AS REAL)) total
    FROM txns_ALL GROUP BY 1,2
```

Why a plain file:
- It's the **learned sibling of `fella.md`** (today's user-authored context).
  They compose.
- **Auditable** — `WHY.md`'s whole thesis. The user opens it and sees exactly
  what Fella believes about their folder, and edits or deletes it. No opaque
  store.
- **Rebuildable** — any FTS index is a cache derived from the file(s); losing it
  isn't data loss (memweave's principle).
- Moves with nothing it shouldn't; survives Fella upgrades.

**Where it lives.** Default: Fella's per-workspace app-data dir (same place as
`fella.db` and saved conversations), keyed to the folder path — so the
"Fella only ever *reads* your folder" guarantee stays absolute. A `/memory`
command opens it for viewing/editing. Opt-in: keep it in `<folder>/.fella/` for
users who want it committed alongside their data.

## Writing: prefer deterministic signals over LLM extraction

Most of the content comes from things Fella **already computes** — no extra
model call:

| Signal Fella already has | Becomes |
|---|---|
| `verify` pass succeeded + answer not corrected | a **recipe**: `(question text, SQL, tables, ts, use-count)` |
| User corrects an answer | a **vocabulary / semantics** note (the correction text, tied to the tables in play) |
| Ingest coerced a column / dropped a totals row | a **schema note** on that column/table |
| `/model`-independent preference the user states | a **preference** line |

An **optional** end-of-session LLM pass can tidy: dedupe near-identical
recipes, fold three vocabulary notes into one, drop a recipe not used in N
sessions. It's a nice-to-have, gated behind a setting, never the core
mechanism.

**Supersede, don't append** (Zep's idea, not its graph): a new note about
`category` values *replaces* the old one; a corrected mapping overrides. The
file doesn't accumulate contradictions. Each entry carries a date; conflicts
resolve newest-wins with the old value kept as a struck-through comment for one
cycle.

**Permission.** Writing memory is the one new kind of persistence. Default:
Fella proposes at end of session — "Learned: *gym is under `health`*. Keep
this? [yes] [edit] [no]" — one confirm, then it's in the editable file. A
per-workspace toggle switches to "learn silently" for users who'd rather not be
asked.

## Retrieval: split by memory type ("static first, dynamic last")

The recall-tool-vs-`memory.md` fork resolves to **both, split by what the memory
is**:

| layer | delivery | why |
|---|---|---|
| **Semantic core** — preferences, all vocabulary, the top ~5 recipes by use-count (~300–500 tok) | **In the system prompt.** Static, so it's prompt-cached after turn 1. | Always available, and on repeat turns it costs ~10% of its tokens. No round-trip, no reliance on the model choosing to fetch it. |
| **Episodic log + the long semantic tail** — specific past interactions, the 40+ rarely-used recipes/notes | **Behind a `recall(topic)` tool.** FTS5 lookup, kept *out* of the cached prefix. | Paid only when the model reaches for it. Scales without bound. Keeps the dynamic part after the cache boundary so it doesn't invalidate the cached core. |

Reasoning behind the split — **tool use is the more expensive axis**, not token
use:

- A `recall()` call is a full extra round-trip (~1–3 s on the tested models);
  agentic workflows already run 2–30× the tokens of a plain chat, and every
  step is a fresh chance for a weak model to derail. Prompt tokens on a *stable*
  prefix are cheap and, with prompt caching, ~90% off on repeat.
- So the always-useful part belongs in the cached prompt (cheap, reliable), and
  only the rarely-needed, unbounded part pays the round-trip.
- If a performance gain is real, the delivery cost is noise either way — see
  §Measuring it. The split is about *default* efficiency, not gating the
  feature.

**Still to decide from the eval:** whether `gemma4` reliably calls `recall()`
when it should. If not, pre-inject the top-K tail matches instead of exposing a
tool — same FTS lookup, no round-trip, at the cost of some cache churn.

Schema notes are in neither layer — they attach to the catalog and ride the
schema block's existing size-tiering (full → names+notes → names-only).

## Staleness — self-healing

- On `/reindex` or any catalog change, a recipe/note whose referenced table no
  longer exists is marked `(stale: table X gone)` — flagged, not deleted, and
  shown to the model as such.
- A recipe reused whose SQL now fails `verify` is demoted; two failures and
  it's dropped. Memory that stops working removes itself.

## Measuring it

Extend `agent_eval session-memory`: two runs on one workspace — run 1 has a
correction turn, run 2 is a *cold* conversation with memory loaded. Gold: run 2
applies the correction unprompted. Report run-2 accuracy/closeness with memory
on vs off, and the token cost of the core block.

**The bar, per the maintainer's stated preference** (`DECISIONS.md`
2026-09-07): performance beats token minimalism. If memory improves correctness
on `gemma4`, the always-on core's tokens are accepted — the "don't add
scaffolding" rule is for *correctness-neutral* additions. What we still watch:
the core block must not *regress* the base battery (a distraction cost on
questions that don't need memory), and the design stays permissive — memory is
reference the model may use, never a rule it must follow.

## Open questions

- **Core-block contents.** Vocabulary + preferences are clearly worth always-on.
  The top recipes are the marginal call — include the top ~5, or FTS-gate all
  recipes and keep the core to vocab + prefs? Decide from `session-memory`.
- **`recall()` vs. pre-inject the tail.** Does `gemma4` call the tool when it
  should? If not, pre-inject the top-K (no round-trip, some cache churn).
- **Recipe-match precision.** A "2024 spend" recipe pulled for "2023 spend" and
  reused with the stale filter. `verify` catches the wrong answer but it's a
  wasted turn. How aggressively to genericise stored recipe text (strip literal
  years/dates/file names to placeholders on store?).
- **Episodic retention.** How many sessions / events before it rotates; whether
  the user ever sees the raw log or only the distilled `memory.md`.
- **Multi-folder.** A user with `~/money` and `~/health` — memory is strictly
  per-folder, no cross-folder sharing (matches the folder-is-the-world model).

## Implementation (v1)

`engine::memory` (`FolderMemory`), ~560 lines, no new dependency. Wired into
`agent.rs` (a `PromptProfile.folder_memory` section, byte-identical when empty)
and `state.rs` (`folder_memory_block()` read each turn; a record hook in
`ask()`). `FELLA_MEMORY=0` turns read+write off; `=ro` reads but records nothing.

**What v1 does**

- **Store.** One `.md` per workspace under `<data_dir>/memory/`, plus an
  append-only `<mem>.episodes.jsonl` (raw record, capped at 200; not read back
  yet). The `.md` is the source of truth — lenient parse (an unreadable line in
  a managed section is kept), authoritative re-render of the managed sections,
  a user `## Notes` section and any unknown sections preserved verbatim. Read
  fresh from disk every turn, so a hand edit lands immediately.
- **Deterministic writer**, no per-turn LLM call:
  - a `run_sql` answer that `verify::reran_clean` confirmed on **one** query →
    a recipe (`question → SQL`, use-counted, superseded by normalised question,
    least-used trimmed past 40);
  - a follow-up that `is_correction()` flags (starts with "no,", "actually",
    "that's wrong", …) → a vocabulary note keyed on the corrected topic.
- **Semantic core** (`semantic_core()`): preferences + all vocabulary + table
  notes + the top-5 recipes by use, within ~1.6 KB, prepended after the schema
  block. **Empty memory renders nothing** — a fresh folder pays zero tokens.
- **Staleness.** `mark_stale()` flags recipes whose tables left the catalog;
  stale recipes drop out of the core.

**Deferred**

- **`recall()` tool / episodic retrieval.** The core-block half is built; the
  long-tail-via-tool half is not. The episode log is written but unread.
- **Ingest-note sync.** Considered and cut: coercion / totals-row notes are
  *already* in the schema block (small folders) or in `describe_schema` on
  demand (names-only folders). Duplicating them into memory is redundant and
  was the main source of prompt-token cost. Memory holds only what the schema
  doesn't: vocabulary, recipes, corrections.
- **End-of-session tidy pass**, `<folder>/.fella/` opt-in location, `/memory`
  command.

**Benchmarks** (`agent_eval memory`).

*Same-conversation, clean folder* — prime an aggregate, cold-ask a different
aggregate on the same table (gemma4:31b, synthetic workspace): **no accuracy /
step / waste difference; ~+130 prompt tokens** for the primed recipe. Clean
synthetic tables with guessable column names → a recipe isn't load-bearing, the
model writes the right SQL from the schema block alone.

*Cross-session, messy folder* — the case memory is actually for. One file
(`spend.csv`) with cryptic columns (`txn_dt`, `amt`, `cat`) and rent showing up
as `Rent` / `rent` / `HOUSING` / `mortgage` / `housing`. **Session 1**: the user
lists the categories, then corrects Fella — "actually, for rent totals count
HOUSING and mortgage as rent too". The correction lands as a vocabulary note.
**Session 2** is a *cold conversation* (memory is the only carry) asking for
total rent spending; gold is 7 350 (all rent-ish rows), literal-`rent`-only is
3 700.

Two rounds: `[a]` memory alone, `[b]` memory + the case-sensitivity flag added
after (`case_collision` — a label column whose values collapse under
case-folding gets a schema/`run_sql` note).

Round `[a]` = memory alone. Round `[b]` = memory + the case-sensitivity flag.

| model | round | memory on | memory off |
|---|---|---|---|
| **gpt-5.6-luna** | [b] | **✓ 7 350** · `WHERE lower(cat) IN ('rent','housing','mortgage')` · +199 tok | ✗ 3 700 · `WHERE lower(cat) = 'rent'` |
| **xai/grok-4.3** | [b] | **✓ 7 350** · `WHERE lower(cat) IN ('rent','housing','mortgage')` · +167 tok | ✗ 3 700 · `WHERE lower(cat) = 'rent'` |
| **gemma4:31b** | [a] | ✗ 4 850 · `WHERE cat IN ('Rent','HOUSING','mortgage')` (case-sensitive) | ✗ 6 150 |
| **gemma4:31b** | [b] | **✓ 7 350** · `WHERE cat COLLATE NOCASE IN ('Rent','Housing','Mortgage')` · +187 tok | ✗ 6 150 |

The mechanism **conveys** the idea across the session boundary every time — the
correction text is in the prompt. **Interpret + apply**, with the flag in
place: **all three models go memory-on ✓ 100 % (7 350), memory-off ✗ 0 %**, for
~+180 prompt tokens. Every model folds case (`lower(cat)` or `COLLATE NOCASE`)
and applies the carried `IN ('rent','housing','mortgage')`; without memory none
can know `mortgage` counts, so all three are correctly wrong.

Round `[a]` (gemma4, no flag) is kept above to show what the flag fixed: gemma
carried the correction and added the synonyms but wrote a case-sensitive `IN`
and missed the lowercase rows.

**Reading:** memory earns its keep on the exact cross-session messy-folder case
it was designed for — on a frontier model *and* the `gemma4` floor — once the
correction is paired with a schema flag that removes the case ambiguity. v1
ships default-on (a fresh folder costs nothing).

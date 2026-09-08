# Per-folder memory — design exploration

Status: exploration for GitHub #42. Not a spec. Ships as its own PR after the
`agent_eval` branch. Constraints from `WHY.md`: local, auditable, no new
dependency, small enough that one person can read all of it.

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

## Retrieval: two-tier

1. **Core block, always prepended** (~300–500 tokens): preferences, all
   vocabulary, the top ~5 recipes by use-count. Cheap, covers the common case.
2. **The tail, selected per question**: FTS5 over recipes + notes, top-K by
   lexical overlap with the question and the tables it touches. Either
   pre-injected ("recipes that worked for similar questions: …") or behind a
   `recall(topic)` **tool** the model calls when it wants one. The tool form is
   honest about cost but leans on a weak model remembering to call it — decide
   from `agent_eval` on `gemma4`.

Schema notes are not in either tier — they're part of the catalog / schema
block, tiered as that already is.

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
on vs off, and the token cost of the core block. **Gate:** `gemma4` must not
regress on the base battery with memory enabled — the core block can't tax the
questions that don't need it.

## Open questions

- Core-block budget vs. the prompt-minimalism finding — every always-on token
  is scrutinised. Maybe the core block is *only* vocabulary + prefs, and even
  the top recipes are FTS-gated.
- `recall()` tool vs. pre-injection — a real fork; `gemma4` behaviour decides.
- Recipe matching precision — a "2024 spend" recipe pulled for "2023 spend" and
  reused with the stale year filter. `verify` catches the wrong answer, but
  it's a wasted turn. How aggressively to genericise stored recipe text.
- Multi-folder: a user with `~/money` and `~/health` — memory is strictly
  per-folder, no cross-folder sharing (matches the folder-is-the-world model).

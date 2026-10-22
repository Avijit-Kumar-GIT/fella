# OpenUI reliability contract v1

This revision records the user's explicit requirement that reopening an answer
must display its saved presentation immediately, rather than a preparing screen.
It supersedes the loading-first assertion in the v2 presentation contract; this
is a specification change before validation, not an adjusted passing baseline.

Acceptance criteria:

- Valid persisted or precision-safe legacy presentations render without waiting
  for artifact IPC or a window reload. Caches cannot cross workspace/run identity.
- Invalid presentations receive at most two presentation-only correction calls
  per creation attempt, grounded in saved results and actual validation errors.
  Correction never reruns analytics or invents missing result bindings.
- Cosmetic aliases and supported chart formatting do not reject otherwise valid
  answers. Malformed programs remain rejected with a recoverable display error.
- Shared presentation tokens establish consistent typography, spacing, surfaces,
  borders and controls; narrow panes do not require horizontal page scrolling.
- Analysis steps start collapsed. Review badges represent actionable recorded
  issues, rather than a generic status applied to every answer.
- Generation has sufficient bounded output capacity for a rich presentation.

Validation is scoped to presentation contracts, reported program compatibility,
answer disclosure, frontend checking, and the backend repair path. These checks
do not establish model accuracy, all possible OpenUI programs, or Windows GUI
behavior. Any live-model or desktop checks are reported separately.

## Validation record

- Presentation contract: 76/76 passed. The first run was 75/76 because a
  prompt wording change broke an existing assertion; restored the wording,
  without changing that assertion, before rerunning.
- Compatibility contract: 25/25 passed, including interpreted Vega rendering.
- Cache/repair orchestration: three focused checks passed, including workspace
  isolation and a strict two-attempt cap with invalid output left as a failure.
- Answer disclosure: 8/8 passed. Initial browser setup hit sandbox networking
  restrictions, then a shared Vite prebundle invalidation. Isolating the test
  cache fixed the environment; assertions were unchanged.
- Rust repair tests: 4 passed. OpenUI-filtered Rust tests: 11 passed.
- Frontend type checking: zero errors and warnings.
- Browser layout: 320px and 1440px passed with painted Vega canvases, no
  horizontal scrolling, one/three metric columns respectively. Cached initial
  mount and reopen used no IPC, preparing state or second page navigation.
- Real Gemma 4 31B via Ollama: one fresh category-comparison desktop case passed
  with a chart and unchanged browser Document (no reload). Two real failed saved
  programs—malformed syntax and missing presentation—each repaired in one call;
  canonical analysis files remained byte-identical.
- A third real saved program with literal metrics failed both initial correction
  attempts. Gemma wrapped copied numbers in `@Round` instead of binding them.
  This result is retained, not counted as a pass. A subsequent prompt revision
  supplies concrete saved-result binding recipes; its result is recorded below.
- After that prompt revision, the literal-metric case repaired successfully in
  one real Gemma call (3.0 seconds), with the original canonical run unchanged.
  This is a post-change retest, not a replacement for the failed initial result.
- Production frontend and release Rust engine builds passed. The build retains
  the existing large-chunk warning; this work does not claim to solve bundle size.

The saved-program tests operate on temporary copies of the user's Windows
history. They never change the original profile. Screenshots and diagnostic
programs remain under `/tmp`, not in the repository. These narrow checks are
not an end-to-end accuracy benchmark or coverage of all fifteen user questions.

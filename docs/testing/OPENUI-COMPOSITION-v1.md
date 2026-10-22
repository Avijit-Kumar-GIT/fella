# OpenUI composition regression v1

Defined before candidate changes on 2026-10-22. Baseline: `843ad5a`.

Expected behavior:

- Charts, metrics, and controls remain visible inside Row, Grid, Columns, Tabs,
  and combinations of those layouts. A sibling table must not be the only
  rendered content after valid components have been composed into a layout.
- Restoring a selected tab renders that tab's content.
- Generated instructions describe the reactive syntax the host supports and
  document saved-result row binding. They do not prescribe tables for all
  comparisons. The model chooses a presentation appropriate to the question.
- Explicit display formats may label fields and round displayed numbers while
  preserving exact canonical values for inspection and export. Unformatted
  output and existing saved programs preserve their previous semantics.
- Nonnegative cosmetic spacing overflow clamps to the existing spacing scale;
  it cannot invalidate or hide an otherwise valid analytical answer. The
  existing spacing scale remains bounded and data bindings remain unchanged.
- No chart presence/variety claim is inferred from parser, SSR, or fixture
  tests. A separate frozen live-provider task set measures rendered model
  output on ranking, trend, scalar, and record-retrieval questions.

`node scripts/test-openui-composition.mjs` checks the first three expectations.
The test programs contain real registered components and use the actual
OpenUI parser, evaluator, and Svelte host; the server render establishes
component mounting, not completed browser chart drawing.

Baseline execution: all seven checks failed. Five layout checks failed on
their first assertion because nested metrics were absent; the selected tab's
content was absent too, and the generated prompt contained the blanket table
heuristic. This directly reproduces a product failure independently of model
choices.

User-approved task-set correction v1.1: the chart-target check now accepts
Svelte's generated CSS class alongside `chart-target`. Its original exact-class
match failed after child rendering was fixed. The test still requires the same
chart element to mount. The frozen v1 baseline failures above are retained;
v1.1 is not reported as an untouched v1 comparison.

Candidate results: all ten composition/formatting checks pass (the original
seven, plus explicit formatted-table/export, invented-format-field and
cosmetic-spacing-overflow checks).
All 76 existing presentation contract checks pass without changing their
assertions; eight focused decimal-formatting checks pass too.

The first real Electron composition run passed the five existing OpenUI
desktop cases but failed both new chart-drawing cases. The screenshots showed
charts collapsed to a narrow strip: Vega-Lite's default discrete step width
overwrote a width set after compilation. The runtime now supplies an available
numeric width before compilation and preserves an explicitly authored numeric
width. With that product correction, both new drawing/tab-switching cases pass
under unchanged criteria. Original failed screenshots and traces remain under
`/tmp/fella-generative-workspace-v1.3-results`; the corrected run is under
`/tmp/fella-openui-composition-width-fixed`.

Unrelated Studio lifecycle cases moved out of the OpenUI suite, retaining all
four original test bodies and assertions in `studio-artifacts.desktop.spec.mjs`.
That dedicated suite passes all four cases. Nothing was deleted to hide a
failure. New OpenUI tests check actual drawn chart pixels and selected content,
while live-provider tests separately check automatic chat presentation.

## Research and compatibility decisions

- The [Svelte package](https://github.com/thesysdev/openui/tree/main/packages/svelte-lang)
  documents `ComponentRenderProps` and a `renderNode` snippet. Its installed
  `Renderer.svelte` handles an individual node, an array, and scalar values.
  Fella's evaluated-results host now follows that same child-rendering contract.
  The previous array-only host was the cause of disappearing composed content.
- [Component definitions](https://www.openui.com/docs/openui-lang/defining-components)
  establish prop order from the Zod schema. Table formatting is an optional
  appended prop, so existing saved positional calls retain their meaning.
- [System prompts](https://www.openui.com/docs/openui-lang/system-prompts)
  describe generation from the same library and enabling bindings. Fella keeps
  the pinned schema/prompt generator, enables its reactive language instructions,
  and replaces the pinned generator's generic table/comparison heuristic. Local
  function documentation and analytical binding requirements accompany it.
- [Rendering](https://www.openui.com/docs/openui-lang/renderer) and
  [interactivity](https://www.openui.com/docs/openui-lang/interactivity)
  describe state hydration and action callbacks. Fella retains its existing
  saved-result evaluator, local state and explicit follow-up routing; generated
  presentations do not gain analytical execution authority.
- [Open Intelligent UI](https://github.com/thesysdev/open-intelligent-ui/tree/main)
  composes charts, metrics and controls into shared layouts and uses reactive
  values for useful interaction. This informed the checks for actual nested
  content, task-appropriate presentation, and immediate chat rendering.
- The [Intelligent UI announcement](https://openai.com/index/gpt-6-for-everyone/)
  is a product-experience reference: presentation follows the question and can
  combine text, visuals and interaction. It is not a library dependency or a
  reason to change the configured analytical model.

The app remains on its pinned OpenUI Svelte/core packages. Current website
examples may show newer APIs; changes here use APIs verified in the installed
versions and the public Svelte package. Analytical execution and verification
behavior is unchanged. The agent's presentation instructions now also receive
the shared generated design guidance in the system prompt. The final OpenUI-only
desktop run passed all seven cases, and all ten Rust tests selected by the
`openui` filter passed. The production desktop build and Svelte checks passed;
the existing large-chunk build warning remains. The separate live-provider
ledger records its initial failures and the corrected candidate's results,
including chat rendering without reload.

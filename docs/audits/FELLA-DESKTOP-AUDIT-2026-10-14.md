# Fella desktop: product and UX audit

**Audit date:** 14 October 2026 · **Scope:** Current desktop app, analyst workflow, visible product model, documentation alignment, and available UI test evidence. · **Code changes:** None. This report and the accompanying screenshots are the only deliverables from this audit.

## Executive assessment

Fella is no longer merely a chat window placed over an analytics engine. The current implementation has the outline of a coherent local-file analytics product: a Workspace defines the data boundary; an Investigation gives a bounded analytical effort a durable home; it owns a linked Conversation and ordered Questions; an Analysis turn records an execution; and evidence, verification, Findings, visualizations, and a report support review and reuse.

That is a meaningful product foundation. The strongest parts are the constrained, read-only local analytics runtime; the durable links between a Question and its run; clear source-scope and provenance concepts; the low-ceremony first-use path; and the newer shell model in which conversation remains primary and Tools are optional. The engine and domain model are more coherent than the interface and some of the older product documents imply.

The main weakness is legibility. A first-time user can start a conversation without learning the domain model, but the relationship between a freeform chat, a Question, an Analysis run, an answer, a Finding, and a shareable report is not yet consistently obvious. Some workflow state is split across the Conversation and the Investigation tool. Several older documents still describe the retired Focus / Investigation / Workbench top-level modes and multi-pane environments as if they were current. The most concrete visual defect found is in the Questions section: its heading and save controls collapse into an extremely narrow column at an ordinary desktop width. The right-hand Tools pane is also narrow enough that the six-section Investigation navigation feels crowded.

My overall read: **strong analytical runtime and increasingly coherent records; a promising but still uneven analyst-facing workflow; and a visible documentation/UX contract drift that makes the product feel less settled than its underlying model.** This is not a recommendation to rebuild the engine. The next work should make the existing model easier to understand and operate, fix the responsive layout defect, and bring the canonical docs into line with the app that now exists.

## Scope and evidence

This audit inspected the active Svelte/Electron shell and relevant components, the Investigation command path, the product and interaction documents, the current workspace E2E suite, and rendered screens at 1440 × 960. The images are browser-rendered captures of the desktop renderer in a deterministic UI fixture. They show real layout and interaction affordances, but fixture content and assistant answers are not live-model output. The images use the light theme and do not include the native Windows or macOS frame.

Validation performed:

- `pnpm check` completed with 0 errors and 0 warnings.
- The unchanged `pnpm test:e2e:workspace` run completed with **98 passed and 2 failed out of 100**. No tests were changed, skipped, or rerun to improve the score.
- Those E2E tests install a deterministic mock for the desktop bridge. They provide useful evidence about UI navigation, state persistence, accessibility hooks, and interaction wiring. They do **not** establish correctness of model-generated breakdowns, SQL, Python analysis, analytical answers, or behavior on arbitrary real-world folders.
- No live-model benchmark, real-file correctness benchmark, novice usability study, or analyst observation session was run as part of this audit. Those remain important evidence gaps.

The failures are reported in the test section below with their exact contract tension. They should remain visible until the expected product behavior is adjudicated; this report does not silently convert either failure into a pass.

## What the app is today

### The central product object

The closest current product description is in [PRODUCT-MODEL.md](../PRODUCT-MODEL.md): Fella is an AI analytics harness for analysts working with files, and an Investigation is the durable unit of work. This model separates several facts that are easy to conflate in a chat-first product:

- A **Workspace** is the explicit local data boundary, usually a mounted folder.
- A **Source** is a cataloged file, sheet, or supported document in that Workspace.
- An **Investigation** is a bounded effort to answer an analytical problem.
- Its **Brief** records goal, audience, desired output, and constraints.
- A **Question** is the root request or a smaller answerable piece of it.
- A linked **Conversation** is where the user directs, clarifies, and reviews the work.
- An **Analysis turn** is one bounded execution. It carries the run identity and context.
- **Evidence** and verification records substantiate or challenge an answer.
- A **Finding** is a durable, reviewable claim; a chart or report is a reusable output.

This is a cohesive model because the objects answer different analyst questions: What problem are we solving? What sources did we use? What did we ask? What ran? What supports the claim? What can be handed to somebody else? The model also preserves ownership: the transcript does not become the source of truth for run provenance, and changing a displayed layout should not change data access or analytical state.

The important qualifier is that the product-model document is explicitly a north star, not a declaration that all of these capabilities are fully implemented. The audit therefore distinguishes current UI and persistence from future product intent.

### Current shell and navigation

The active shell is simpler than the older UX architecture documents:

1. A global sidebar holds General, mounted workspaces, their conversations and Investigation entry points, plus Search and Settings.
2. The main content is a Conversation with its composer. The composer remains inside the conversation surface.
3. A workspace-level Tools pane can be shown or hidden. With no tool open, it offers Investigations, Data & context, and Terminal. Investigations are selected from a dropdown; opened tools become pane tabs.
4. An Investigation has its own internal sections: Brief, Data, Questions, Analysis, Results, and Share. Its Conversation is the Investigation’s linked Conversation, not an unrelated secondary chat.
5. Workspace-level Data & context is separate from the Investigation’s catalog/scope view. The former edits or displays workspace guide/context; the latter shows cataloged data and the Investigation’s source scope.

This is a better conceptual fit than a model where Focus, Investigation, and Workbench are presented as three peer destinations. It lets the user keep the conversation visible while opening a relevant tool, without making analysis screens look like independent applications. The current implementation also has request guards for quick tool-tab switches, and the suite includes coverage for Investigation/conversation identity and rapid tab navigation.

However, the shell still carries a few traces of the older model. In the clean first-launch screenshot the titlebar says “Focus,” even though the visible page is a new Conversation and the current app’s primary interaction is chat. That leftover vocabulary subtly tells the user there is a mode or view to understand before they have done anything.

### First use and everyday interaction

The first-launch screen is restrained and understandable: Fella identity, General workspace selection, New conversation, a choice to choose a folder or try sample data, a short privacy/provider disclosure, and a composer. It does not force a user to configure an Investigation before asking their first question. This supports the desired “easy to get into, hard to get good at” principle.

When a folder is selected, the conversation becomes scoped to it, and the UI surfaces the active scope. The user can continue to ask in chat and open an Investigation from the conversation or Tools pane. This is an appropriate progressive-disclosure strategy: the beginner can ask; the analyst can inspect sources, frame an Investigation, and work with persistent Questions and results.

The caveat is that first-use simplicity pushes some important distinctions into the next step. A user can ask an open-ended prompt and later discover that the durable Investigation has a Brief, Questions, an Analysis record, Findings, and Share. The UI should make it easier to answer “Did my prompt create an Investigation?”, “Is this chat attached to that Investigation?”, and “What does Analyze do that sending a chat message does not?”

### Investigation work surface

The Investigation is organized into six internally navigable sections rather than one long continuous page. This is directionally strong: it gives the work a readable sequence and keeps dense details out of the initial Brief. It is not a rigid wizard; the user can move among sections and the model can propose or adapt the question plan.

- **Brief:** goal, audience, deliverable, constraints, and a route to the linked Conversation.
- **Data:** catalog observations, source readiness, and source selection for future runs. The UI correctly warns that catalog observations do not themselves prove a source answers the Question.
- **Questions:** an editable parent Question and optional ordered child Questions; optional definitions and assumptions; suggested breakdown; automatic saving; coverage counts; and per-Question Analyze actions.
- **Analysis:** linked run history, run details, evidence, method, provenance, and Findings.
- **Results:** Findings and visualizations, question coverage, and links back to the analysis that produced them.
- **Share:** a report/export projection of saved work rather than a copy of the raw transcript.

This decomposition maps well to analyst work: frame, discover, decompose, execute, review, communicate. Its current friction is not that the app lacks analytical sections; it is that those sections and their relationships are packed into a narrow Tools pane and use several different kinds of status and artifact.

The Questions screen illustrates the issue most clearly. The intended capabilities are good, but the page puts optional semantic context, Definitions & assumptions, Question plan, autosave state, completion, breakdown suggestion, coverage summary, empty state, and add-question controls close together. It is a lot of concepts for the highest-cognitive-load part of the workflow. The navigation helps divide the overall Investigation, but the Questions section itself could still use clearer hierarchy and progressive disclosure.

### Analytics engine and trust boundary

The code and architecture docs describe a deliberately bounded runtime. The desktop renderer is sandboxed; local data is read through the Rust engine and an allowlisted bridge. The shipped engine uses bundled SQLite; supported tabular formats include CSV/TSV, JSON/NDJSON, and supported Excel inputs. Supported text and extractable-text PDF files are searched as documents; PDFs need a selectable text layer. No OCR or embedding index is provided in the default path. Parquet depends on a custom DuckDB build and is not part of the standard release.

For computation, the model can use read-only SQL, a bounded sandboxed Python route, document search/read, and structured chart production. The runtime enforces limits; the app does not give the model unrestricted shell or filesystem writes. Model calls go to the configured provider. This is a strong point of product coherence: the promise is not “an unconstrained agent that can do anything,” but “a model that can investigate local files through visible, bounded analytical tools.”

The interface exposes some of this well through source scope, run records, evidence, checks, and caveats. Still, there is a meaningful distinction between an answer that sounds right, an answer supported by the recorded evidence, a verification check that passed, and a Finding someone has reviewed. The implementation has vocabulary and records for these states, but the user should not need to infer those distinctions from status colors or investigate several screens to find them.

## Analyst-workflow audit

| Analyst step | What is already present | Strength | Gap / uncertainty |
|---|---|---|---|
| 1. Get the problem | Start with a natural-language prompt; an Investigation brief can capture goal, audience, deliverable, and constraints. | Very low entry cost. The user can begin in the Conversation rather than learning an analytics product first. | The brief is not yet the unavoidable or consistently visible framing step. A prompt can become durable analytical work without the audience, decision, or expected deliverable being made explicit. |
| 2. Learn what data is available | Workspace catalog, source profiles, readable/skipped source states, source selection, and Data/context surfaces. | Strong local-file boundary and evidence that data inspection precedes computation. The UI cautions that availability is not proof of answerability. | Workspace guide/context and Investigation data inventory have similar names but different jobs. The path from “I inspected these files” to “these are the sources this Question may use” needs to stay explicit. |
| 3. Decompose the question | Durable parent and linked child Questions; reorder/remove/edit; suggested breakdown; definitions and assumptions. | This is a genuinely analyst-shaped capability, not just one chat prompt. The plan can change as understanding improves. | The quality and depth of model suggestions are not established by the mocked E2E suite. Users need a visible signal that a proposed breakdown is a draft, and a clear way to judge whether it covers the original request. |
| 4. Answer smaller questions | Each active Question has a dedicated Analyze action and can be linked to an individual run. | Atomicity is good: the analyst can inspect a discrete question and its provenance rather than relying on one giant answer. | The boundary between structured Question analysis and ordinary follow-up chat is ambiguous. One E2E failure directly exposes this (see below). The UI needs to make clear whether a freeform follow-up belongs to a Question or is unlinked conversation. |
| 5. Build the query/method | The model chooses among bounded SQL, Python, document, and chart routes; recorded plans and evidence can show method. | The user does not need to author SQL to get started; the method is inspectable after a run. | There is not a clear analyst-operable query notebook or scratchpad where a user can revise the query itself and rerun it. Steering largely happens through the natural-language Question and follow-up. This may be intentional, but it is a product choice worth making explicit. |
| 6. Run the analysis | Persisted Analysis turns, current source scope and context snapshots, run history, stop/retry behavior, and run linkage. | Good continuity and traceability. A run is more than an ephemeral assistant response. | End-to-end engine correctness on realistic, messy folders is not evidenced by this UI audit; the mocked tests do not invoke the real model/engine path. |
| 7. QA the answer | Verification results, evidence inspection, coverage states, outdated-result detection, run context, caveats, and explicit gap handling. | This is a major differentiator from “chat with a chart.” The app has a meaningful place to challenge or qualify an answer. | A passing deterministic check is not the same as a correct business interpretation. The UI must say what was checked and what remains a judgment. “Partial answer” can read as a system failure unless its scope and next action are immediately clear. |
| 8. Visualize | Structured chart evidence and interactive visualization displays; charts can be reviewed and exported. | The visualization belongs to the analysis and can retain its run/evidence lineage. | A generated chart is not automatically a reviewed Finding. That distinction is conceptually correct, but the handoff from a chart/answer to a durable reusable output may feel like extra curation unless the reason is clear. |
| 9. Hand it over | Findings can be captured/revised with evidence links, caveats, assumptions, and revision history; Share can produce a report; Projects can curate references. | There is a path from analysis to a durable deliverable and provenance is not discarded. | The path is comparatively manual. The user must understand what is a transcript answer versus a Finding versus a report. External handoff and recipient-oriented review are limited to the current export/projection path; no live team collaboration or external PM workflow was validated. |
| Keep work alive for a week or month | Investigation and run history are durable; edits can make prior answers outdated; linked Conversation preserves direction and clarification. | Strong foundation for long-running analysis rather than a one-shot chat. | The audit found no explicit timeline, due-date, reminder, or “what changed since last visit” concept in the primary Investigation flow. Do not add these by default without validating demand, but continuity summaries are a likely future opportunity. |

## Strengths worth preserving

### 1. The model is becoming coherent around analyst work

Workspace, Source, Investigation, Question, Conversation, Analysis turn, Evidence, Finding, and Artifact are not arbitrary UI labels. Their ownership boundaries address real analyst needs and reduce the tendency to put every fact into a transcript. This should remain the conceptual backbone.

### 2. The engine is purposefully constrained

Read-only source access, bounded execution, explicit workspace scope, and provenance are strong product decisions. They help make local-file analysis safer and more explainable. There is no reason from this audit to disturb the analytics engine’s core execution loop.

### 3. The app preserves an approachable starting point

New Conversation, choose a folder, sample data, and an always-visible composer give a new user a clear first action. The advanced Investigation model can be discovered progressively instead of becoming a mandatory setup wizard.

### 4. Conversation and Investigation identity are explicitly connected

The Investigation owns one canonical Conversation in the current workspace model. Opening it should not create a second transcript or make chat identity ambiguous. Tests that exercise opening an Investigation’s linked Conversation and fast tab changes passed in the full suite.

### 5. Scope and provenance are treated as first-class concerns

Source scope, run snapshots, citations/evidence, stale results, checks, caveats, and revision history are represented in the domain and UI. This directly supports trust and later handoff.

### 6. Visual language is restrained at the shell level

The captures show a calm neutral palette, restrained borders, pill navigation, curved controls, and useful whitespace in the conversation area. It avoids the dense tile-grid/dashboard look. The overall shell has a recognizable Fella mark and consistent component vocabulary. The main visual problems are local hierarchy and responsive behavior, not an absence of design taste everywhere.

## Gaps and risks, ordered by impact

### P1 — Questions heading breaks down in the normal split layout

At 1440 × 960, with the Tools pane open, the Questions section’s “Question plan” heading and its explanatory copy render in a very narrow column and wrap awkwardly. The Save state is displaced to the far right; the surrounding tabs also feel tight. This is a visible usability and polish defect, not merely a hypothetical responsive edge case.

The main workspace split allocates 62% to the Conversation and 38% to Tools, with minimums of 320px and 280px, and only stacks the panels below a workspace-area width of 1000px. The Investigation component then has a container rule for panes under 900px that turns the Question heading into a column. The rendered result suggests the heading’s content/flex sizing is not expanding to the available line width under that layout. The screenshot establishes the visual behavior; the CSS is a plausible implementation lead, not proof of a single root cause.

Evidence: [05 — Investigation Questions](assets/05-investigation-questions.png), [workspace split CSS](../../src/routes/+page.svelte#L501), [Question responsive rule](../../src/lib/components/InvestigationBriefView.svelte#L3249).

### P1 — Chat, Question, run, answer, and Finding need a clearer relationship

The durable model is good, but the user-facing mental model is not fully self-explanatory. The app supports ordinary Conversation prompts and explicit per-Question Analyze actions. A user needs to know whether a follow-up is simply chat, is implicitly attached to the selected Question, or creates a new Question/run. If the answer is not obvious, transcript content and structured analysis can drift apart even when their records are individually sound.

This is not a call to put a whole Question plan into every chat prompt. The product model should preserve the user's original wording and the atomicity of individual Questions. It is a call to expose the relationship and linkage clearly, including when an ordinary follow-up is intentionally not linked to a Question.

### P1 — Canonical docs describe different products

The current product model calls itself canonical and describes the durable Investigation. In contrast, [INTERACTION-MODEL.md](../INTERACTION-MODEL.md) still declares Focus / Investigation / Workbench as three primary views and describes environments and pane arrangements. [COMPOSABLE-INTERFACE.md](../COMPOSABLE-INTERFACE.md) still says it is the contract for an environment-tab redesign and models up to four workspace panes. The active app instead keeps a Conversation primary, offers a Tools pane, and opens tool tabs within that pane. Some older product-vision text still describes Investigation/Question/Definition/Finding objects as missing or future work.

That contradiction costs more than documentation neatness: it makes product decisions harder to discuss, risks reintroducing retired interaction patterns, and makes it unclear which feature promises have shipped. The docs should label historical plans as superseded or update the active contracts.

### P2 — “Focus” is legacy vocabulary in the visible shell

The clean first-launch screen is a new Conversation, while the titlebar displays “Focus.” “Focus” was previously a view/mode; in the current model the user’s stable object is the Conversation in a Workspace. Retaining this label adds an unnecessary concept and weakens the simple first impression.

### P2 — Investigation navigation is dense for a narrow pane

Six sections—Brief, Data, Questions, Analysis, Results, Share—are a useful separation of content, but they compete for horizontal room inside the right pane. The Questions screenshot shows the tabs compressed and Share partly at the edge. The layout should communicate that these are the Investigation’s own sections without making the right pane feel like a miniature full-width application.

### P2 — “Data” and “Data & context” sound like the same destination

Within an Investigation, Data means cataloged sources and this Investigation’s scope. At Workspace level, Data & context means workspace context/guide content and broader source context. Their scopes differ, but the naming is close enough to cause confusion. The UI should name their job and ownership distinctly, not merely their data theme.

### P2 — Partial coverage needs to remain informational, not punitive

The app has status descriptions and next-step guidance in a shared status helper; for example, incomplete coverage means the answer covers only part of the requested scope and suggests adding data, narrowing the Question, or documenting the limitation. That is the right semantic direction. But the visible label “Partial answer” can still look like a failed run or a guaranteed defect, especially when repeated in Results, Findings, or curated Project references. Status, reason, and available action should appear together and proportionately. A documented and accepted gap must remain distinguishable from an execution failure.

### P2 — Disabled Terminal is visible product chrome without a present capability

The Tools chooser shows Terminal with “Not available in this version.” That is honest, but the control still asks the user to process an unavailable feature. It also risks implying that shell access is part of the product’s core workflow even though the runtime’s safety boundary intentionally does not give the model unrestricted shell access. The long-term meaning of “Terminal” should be decided before it becomes a persistent tool category.

### P3 — No clear “return to my work” summary for long-running analyses

Persistence and run history exist, but the primary flow does not foreground a compact return summary such as the current question, what has changed, which questions remain unresolved, and what needs the analyst. This could matter for work lasting a week or a month. Validate with actual users before treating it as a required feature.

### P3 — Layout behavior outside the captured desktop state is not fully audited

This visual pass covered one desktop viewport and light theme. It did not systematically assess the full width range, Windows/macOS native titlebar interaction, keyboard-only flow, dark theme, zoom/scaling, or screen-reader experience. The E2E suite covers many accessible labels and interactions, but that is not a substitute for visual/device testing.

## Test evidence and unresolved failures

The current workspace suite completed **98/100**. The other 98 tests passed, including a broad set of interaction journeys for workspace selection, Investigation tabs, linked conversations, rapid tab switching, autosave, Question lifecycle, run history, evidence, Findings, report export, source search, Settings, and Project editing. That is meaningful regression evidence for the deterministic interface contract.

### Failure 1: follow-up chat is expected to carry Question-plan context

Test: “an analyst can edit the main Question and linked subquestions without rewriting the request.”

The test saves a parent Question, “Measure conversion by channel,” and child Questions including “Compare conversion by region” and “Compare conversion by device.” It then sends the ordinary chat message “Which device has the strongest change?” The test expects the resulting request sent to the mock bridge to include the selected child Question and parent-plan context while keeping that context out of the visible user transcript. The observed bridge request contained the ordinary submitted message instead, so those assertions failed.

The explicit Analyze action has a different, intentionally structured path: it loads the saved Question, sends that Question’s text, and carries the Investigation and Question IDs into the run. Ordinary chat uses the ordinary ask path. The failure therefore exposes a real unresolved product contract: should a freeform follow-up automatically bind to the currently selected Question, or should it remain unlinked unless the user explicitly invokes Analyze? The latter is consistent with the principle of preserving the user's actual message and analyzing Questions individually; the former is what this test currently asserts. Do not resolve this by silently appending a plan to chat or by weakening the test. Adjudicate the intended interaction, then update behavior or test contract with an explicit rationale.

Evidence: [test](../../tests/e2e/workspace.spec.mjs#L2211), [explicit Question-run path](../../src/lib/commands.ts#L1101).

### Failure 2: Project curation expects an internal status enum

Test: “Projects curate Findings, visualizations, definitions, and open Questions by canonical reference.”

The fixture Question’s internal answer state is `incomplete`, with the reason “Regional detail is not available.” The test expects the curated UI string to contain “Explain weekly sales · incomplete.” The UI uses the shared user-facing status mapping and renders “Explain weekly sales · Partial answer,” so the exact text assertion fails.

This appears to be a vocabulary/contract mismatch rather than evidence that the canonical Question reference was lost: the app deliberately maps an internal state to a human-readable label. Yet “Partial answer” is a sensitive label in this product because it can be mistaken for a broken result. The correct resolution is a product-language decision: preserve an internal enum in a diagnostic surface, use the human-readable status with its reason, or choose a more neutral visible label. The failure remains a failure in this run; it should not be regraded as passed on the basis of this interpretation.

Evidence: [test](../../tests/e2e/workspace.spec.mjs#L5383), [failing label assertion](../../tests/e2e/workspace.spec.mjs#L5465), [shared status labels](../../src/lib/investigation-question-status.ts#L1), [curation mapping](../../src/lib/components/ProjectCuration.svelte#L153).

### What the test run does not prove

Because the suite uses a deterministic bridge mock, a passed “analysis” journey usually proves that the UI submitted the expected data and rendered a fixture result. It does not prove that the model will propose a good breakdown, that the generated SQL answers the Question, that a chart has the right grain, or that source profiling handles every messy file. Real-model and real-folder quality should be measured separately, with fixed benchmark questions and implementation-independent acceptance criteria.

## Visual evidence

All captures below are from the same 1440 × 960 light-theme deterministic UI session. Text/data in the mock is illustrative; the visible layout and controls are from the app renderer.

### 1. First launch: a strong low-friction start

The initial action is clear and the privacy/provider disclosure is quiet. The titlebar’s “Focus” label is the visible inconsistency.

![First launch](assets/01-first-launch.png)

### 2. Folder-scoped conversation

The workspace is visible in the sidebar and top context; the composer remains the main interaction. The user can begin in plain language without opening a tool first.

![Workspace conversation](assets/02-workspace-chat.png)

### 3. Tools chooser

The optional Tools surface is a distinct place for workspace-level tools. The Investigation selector is a dropdown, Data & context opens as a tab, and Terminal is present but disabled.

![Tools chooser](assets/03-tools-chooser.png)

### 4. Investigation Brief

The Investigation has an internal navigation and exposes its goal, audience, deliverable, and constraints. This is a useful durable frame for the analysis.

![Investigation brief](assets/04-investigation-brief.png)

### 5. Questions: the clearest visual defect

The Question-plan heading and explanatory copy collapse into a narrow column. The tab row is also tight. This is the strongest direct visual finding in the audit.

![Investigation Questions](assets/05-investigation-questions.png)

### 6. Investigation data and scope

The source inventory and scope controls are visible, with a warning that catalog observations do not establish that a source answers the Investigation.

![Investigation data](assets/06-investigation-data.png)

### 7. Analysis history

The Analysis section has an explicit home for run history and saved Findings, even when empty.

![Investigation analysis](assets/07-investigation-analysis.png)

### 8. Results and unresolved work

The Results section separates Findings, visualizations, and Questions needing resolution. The empty state also reveals how much of the product’s usefulness depends on actually completing the linked analysis lifecycle.

![Investigation results](assets/08-investigation-results.png)

### 9. Workspace Data & context

This is a workspace guide/context surface, not the same screen as the Investigation’s source inventory. The difference in ownership is useful; the similar naming is not.

![Workspace Data and context](assets/09-workspace-context.png)

### 10. Settings and analysis capabilities

Analysis capability controls are present in Settings. This supports discoverability and user control, though the full settings view requires scrolling.

![Settings](assets/10-settings.png)

## Recommended order for follow-up work

This audit does not implement these changes. Based on user impact and dependency, a sensible order for future decisions is:

1. **Fix the Questions pane layout and validate width breakpoints.** It is a direct, reproducible visual failure in the ordinary split view.
2. **Decide the chat-to-Question contract.** Specifically, distinguish ordinary follow-up from “Analyze this Question,” and define whether either action links a run to a Question. Resolve the existing failing test only after that decision.
3. **Make the object relationship legible in the UI.** Explain how a prompt becomes or relates to an Investigation; show the active Conversation/Investigation relationship; show where an answer lives and what becomes a reviewed Finding.
4. **Reduce vocabulary collisions and legacy residue.** Revisit “Focus,” “Data” vs “Data & context,” and the visible disabled Terminal option.
5. **Align current docs with the shipped navigation and object model.** Mark superseded multi-pane/environment documents as historical or replace their status and diagrams. Keep canonical product intent distinct from shipped capability.
6. **Run a real-data and analyst-quality evaluation.** Use a fixed, messy-folder benchmark with expected evidence and metric grain, run it against the real engine/model path, and add an analyst review of whether the generated work is understandable and handoff-ready.
7. **Evaluate long-duration work with analysts.** Only then decide whether return summaries, timeline, reminders, or recurring analysis belong in the product.

## Bottom line

Fella has the beginnings of a distinctive product: **a local-file analytics harness that turns an open-ended request into a durable, scoped Investigation, while preserving a conversation for direction and a traceable record for evidence and handoff.** The engine’s bounded execution and the domain model support that story.

The app does not yet communicate that story with the same consistency. The visible experience still carries some old navigation language; the Investigation/Conversation/Question relationships require inference; one key Question layout is visibly broken; and canonical UX documents disagree with the current shell. The highest-value path is refinement and alignment around the model already built—not expanding the feature list or replacing the analytics engine.

# Interaction-model validation

This record covers the automated portion of UX issue #251, “Validate the new
interaction model with novice and analyst workflows.” It is an engineering
validation report, not a usability-study result. No novice or analyst
participants were available, so the comprehension and learnability criteria
remain untested with people.

## What the checks establish

The Playwright workspace-board suite runs the desktop UI against deterministic
application fixtures. It can establish rendered controls, navigation,
workspace/conversation scope, persistence, and the behavior encoded by those
fixtures. It does not establish that users understand the UI, that the model
will adapt its analysis steps correctly on arbitrary data, or that an answer
is analytically correct.

## Untouched full-suite baseline

On 2026-10-12, before the focused fixes below, the full command
`pnpm test:e2e:workspace-board` completed with **71 passed and 12 failed out of
83**. The result is preserved as the untouched baseline; no test assertions
were changed during that run.

The failures were:

1. `first launch offers a clear folder or sample path and discloses provider data flow` — the page was white at the assertion timeout. A later DOM snapshot showed the onboarding heading, folder/sample actions, disclosure, and composer. This is unresolved cold-start/render-timing evidence, not proof the screen never renders.
2. `environments compose General and repository panes while conversations retain their original scope` — the workspace “New conversation” action left the pane on Investigation, so the expected composer did not appear.
3. `a saved answer opens its canonical run and returns to the same conversation result` — returning from a run was blocked because opening the run's prefilled Finding draft was treated as a user edit.
4. `Workbench panes resize accessibly, respect minimum widths, persist, and reset independently of analysis` — after reload, the test expected a Workspace-board tile despite the app restoring Workbench. Following the approved explicit-board-entry design, the test now checks the restored Workbench splitter directly; the updated test was rerun separately below.
5. `each Question opens its saved answer with explicit partial or complete coverage and run provenance` — the expected `Question run` region was not found.
6. `a multi-question Investigation carries coverage, QA, and run links into its report` — the expected `Question run` region was not found.
7. `Finding caveats and assumptions are visible, editable, and preserve provenance` — the expected Focus composer was absent after opening/switching from the run.
8. `Finding and chart lineage flag artifacts from an earlier Question run` — the expected `Question run` region was not found.
9. `Finding revisions preserve prior versions and compare the supporting run context` — the expected `Question run` region was not found.
10. `Composer source search filters and attaches a workspace source` — “New conversation” did not leave Investigation for the composer.
11. `CommandPalette filters and results use shared buttons without losing search behavior` — the baseline test expected a Workspace board after choosing a repository result, while the captured UI opened the workspace in Focus. The owner later approved updating this destination assertion; its focused rerun passed.
12. `Workspace starter prompts use shared outline buttons and submit the selected prompt` — “New conversation” did not leave the Workspace board for Focus, so its prompt locator was looking in the wrong view.

The four missing `Question run` landmarks were baseline failures. In the
post-fix full-suite run below, those same Question/run, report, Finding, and
lineage checks passed without changing their expectations. The untouched
baseline remains preserved as historical evidence.

## Focused changes and revised checks

Two defects demonstrated by the baseline were fixed:

- Creating a new conversation now sets the pane to Focus, so the scoped
  composer is immediately available even if the pane was previously showing
  Investigation or Workbench.
- A run's prefilled Finding suggestion no longer counts as an unsaved user
  draft. Editing the claim, caveats, assumptions, Question, evidence selection,
  or revision fields marks it as touched and retains the navigation guard.
- The docked multi-workspace board remains supported. A labeled sidebar control
  opens it from Focus when an environment has multiple panes, and selecting it
  clears the active pane's analytical-view preset so the board can be restored.

The prior question-plan dirty-draft confirmation was not weakened; its focused
regression test passed.

With owner approval, two E2E expectations were corrected to match the intended
and captured interaction:

- After starting a new conversation, the multi-workspace flow now switches to
  General through the sidebar instead of expecting the Workspace board to
  remain in front of the Focus composer.
- The starter-prompt test now selects “What changed over time in these files?”,
  which is actually rendered for the fixture, instead of the absent
  “How did my spending change this year?”. The test continues to verify that
  the prompt is a shared outline button and submits into the conversation.

These are UI E2E expectation/fixture corrections, not FQA-Bench edits or an
updated analytics-quality score. The original full-suite denominator and
failures above remain unchanged.

The resize test also had a stale navigation step: after reload the captured UI
already restored Workbench, but the test tried to click a board tile that is
not present in that view. With approval for an explicit board entry point, the
test now verifies the restored Workbench splitter directly. This preserves the
purpose of the check—layout persistence—without treating the board as an
implicit reload destination.

The focused investigation had two intermediate test issues. First, the
multi-workspace journey's reply locator matched two identical fixture replies;
it was narrowed to the newest assistant reply without changing the expected
text. Then the drag placement check exposed a real 52 px target mismatch. The
placement preview was absolutely positioned relative to the whole board even
though its target is the tile grid below the board heading. Moving the preview
inside the tile grid and anchoring it to that grid fixed the coordinate error;
the existing strict drag-geometry assertion then passed. Board reload
validation was isolated in its own test so it does not interrupt the drag
journey.

After these fixes, a focused seven-test selection covering the multi-workspace
journey, reopening/restoring the board, saved-answer navigation, Finding
caveats, source search, starter prompts, and the Question-plan dirty-draft
guard completed with **7 passed** (18.6 seconds). This is targeted post-fix
validation, not a rerun of the full suite or a replacement for its untouched
71/83 baseline. The later post-fix full-suite comparison is recorded below.

That full-suite comparison has now been run once after the fixes and approved
test-path updates: **83 passed, 1 failed out of 84** (2.5 minutes). The
additional test is the new board-entry/persistence regression. The former
baseline failures for first launch, workspace scope, saved-answer navigation,
Workbench resize/reload, Question-run landmarks, Finding caveats/lineage, and
composer source search passed. The only failure in this run was the Command
Palette test's final expectation that opening a repository renders a board tile. The
captured state instead shows the palette closed, northwind-sales active in
Focus, and its scoped composer visible. Its filtering, shared-button, and
palette-close checks passed. The owner approved correcting this destination
assertion while retaining those search checks. This is a documented E2E
task-set revision, not a change to product behavior or the analytics
benchmark; the 83/84 full-suite result above remains the pre-correction result.

The Workbench resize/reload test was separately run after its approved
navigation-path correction and passed (1/1); it also passed in the subsequent
full-suite run. The seven focused checks above and this standalone run are
diagnostic selections, not additional full-suite score.

The corrected Command Palette test was rerun independently and passed (1/1).
The 83 passing tests remain evidence from the same implementation, but the
full suite was not rerun after this test-only correction. No
post-correction full-suite score is claimed.

## Acceptance still needed

- Run the novice first-launch and analyst Investigation/report journeys with
  actual participants; record where they hesitate, misunderstand scope, or
  mistake the step record for a required checklist.
- Resolve the cold-start blank-window observation using a fresh desktop launch.
- Test model-driven skipped/revisited steps with an explicit, versioned
  evaluation; these mocked UI journeys do not establish adaptivity.
- Include keyboard, zoom/dense desktop layouts, and a screen-reader review with
  people using the actual application.

Until those items have evidence, issue #251 should remain open. Scripted E2E
success is not a substitute for participant comprehension or analytical
quality.

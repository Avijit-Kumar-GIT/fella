# Fella interaction model

**Status:** UX architecture contract for [#240](https://github.com/Avijit-Kumar-GIT/fella/issues/240) and [#250](https://github.com/Avijit-Kumar-GIT/fella/issues/250). This defines product direction and terminology; it does not claim every view or behavior is implemented.

Fella has one central unit of analytical work: the **Investigation**. **Focus**, **Investigation**, and **Workbench** are three ways to work with or inspect that same Investigation—not separate products, separate conversations, or different data permissions.

The model chooses an appropriate analysis and adapts it as the question and evidence develop. The interface presents useful actions, results, and evidence in a form the analyst can understand and steer. The runtime enforces workspace scope and execution boundaries. The analyst remains responsible for consequential business meaning and whether a result is fit to use.

## Information architecture

```text
Fella desktop
├── App shell
│   ├── Environment controls — arrange Workspace panes; do not grant data access
│   ├── Search — app-level utility
│   └── Settings — app-level utility
└── Active environment — a layout, not a shared analytical scope
    ├── Workspace pane(s)
    │   └── Workspace — a mounted folder; the data boundary
    │       ├── Data — available sources, context, and explicit source scope
    │       └── Investigations
    │           └── Investigation — the durable work record
    │               ├── Focus — ask, clarify, and get the current answer
    │               ├── Investigation view — review the adaptive work and results
    │               ├── Workbench — optionally inspect technical details
    │               └── Handoff — report/export projection of saved work
    ├── General — unbound conversation unless a scope is explicitly chosen
    └── Projects — optional curation of selected knowledge
```

An environment may show multiple Workspace panes, but arranging panes does
not combine their data. An Investigation belongs to one Workspace. General
is not a hidden Workspace or Investigation; a General conversation remains
unbound unless the user explicitly selects a data scope. The canonical object
ownership and relationships are defined in the
[product model](PRODUCT-MODEL.md); the existing environment and pane contract
is in [Composable interface](COMPOSABLE-INTERFACE.md).

## View contracts

### Focus

**Job:** Make it easy to start or continue an analysis and understand the
current answer without first learning Fella's internal model.

**Show:**

- The active Workspace or explicit unbound General scope before submission.
- The question, conversation, current answer, and relevant visualization.
- A direct path to the Investigation and an optional way to inspect how the
  answer was produced.
- A lightweight first-use path to choose a folder or ask a question.

**Allow:** Ask, clarify, and follow up in the same Conversation and
Investigation. A workspace-scoped question should not require the user to
create or configure an Investigation first.

**Do not:** Make the user approve routine analysis steps, expose dense run
metadata by default, or make an incomplete check hide an otherwise supportable
answer. Keep material assumptions and limitations available and clear.

### Investigation

**Job:** Provide a durable, understandable account of the analytical work and
its current result, including when the work continues across sessions.

**Show:** The goal and scope; relevant Questions and definitions; the current
answer and Findings; visualizations; unresolved limitations; and an ordered
record of meaningful analysis actions and their outputs. Link steps and
conclusions to their supporting evidence, source revisions, and analysis runs.

The ordered record is descriptive, not a prescribed workflow. The model
chooses which steps are useful and may add, skip, or revisit them. Show only
work that actually happened as completed; distinguish a possible next action
from a recorded result. A skipped or unavailable step does not by itself fail
the run or suppress the best-supported answer. State material uncertainty and
gaps instead. A report or handoff is generated from the Investigation's saved
brief, Questions, Findings, artifacts, checks, and provenance; it is a
shareable projection, not another source of truth.

**Allow:** Correct a material interpretation, add or refine a Question, inspect
a result, continue the analysis, review coverage, and prepare a handoff without
rebuilding context from the transcript.

**Do not:** Require every Investigation to have the same sections or sequence,
present raw internal reasoning, or imply that a successful query or check
proves the business interpretation is correct.

### Workbench

**Job:** Offer an optional, more detailed surface for analysts who want to
inspect the method, data, outputs, or checks more closely.

**Show:** Supported technical details and useful analysis context, such as
source profiles, query or calculation inputs, result previews, evidence, QA
checks, and visualizations. Keep each item linked to the same Investigation
and recorded run that produced it.

**Allow:** Inspect and arrange supported views of the work. Returning to Focus
or the Investigation view preserves the same Workspace, Conversation, scope,
and analytical state. Workbench layout changes presentation only.

**Do not:** Make Workbench necessary for a successful analysis, create a
second copy of the Investigation, imply that panes share data, or turn Fella
into a general-purpose Jupyter kernel or unrestricted code environment.
Detailed panel composition and customization are scoped to [#247](https://github.com/Avijit-Kumar-GIT/fella/issues/247).

#### Workbench panel contract

Workbench is a small, typed panel set rather than a freeform dashboard:

- **Focus** shows the active Conversation, its question and current answer.
- **Investigation** is the composite analysis panel. Its supported content
  includes Questions and coverage, Data & context, run history and details,
  evidence, Findings, and visualizations. These stay attached to the selected
  Investigation instead of becoming separately cloned widgets with independent
  state.
- **Chart or source preview** is an optional companion panel for one artifact
  opened from the active Conversation. It preserves the artifact's Workspace
  and revision and reports when the current catalog differs.

The default Workbench shows Focus and Investigation. The companion appears when
the user opens a chart or source; it can be hidden or arranged with the other
panels. Users can group, reorder, and hide panels, resize the Focus/Analysis
split, and reset the arrangement. These preferences change presentation only.
The Workspace and Investigation summary remain visible in the context bar, and
empty/loading states belong to the relevant panel. The composer continues to
target the active Workspace; it is not an independent panel or a way to combine
Workspace data.

## State and recovery contract

Use the same state distinctions in every view:

- **Empty** means the relevant request completed successfully and returned no
  records or content. Say what is absent and offer the action that creates or
  supplies it. A failed request must never be presented as an empty result.
- **Loading** identifies the operation in progress. Keep the active Workspace,
  Conversation, and any already-saved content visible where possible; prevent
  duplicate submissions and announce completion or failure.
- **Success** identifies what was loaded or completed. Do not imply that a
  successful tool call proves the business interpretation is correct.
- **Partial** preserves the supported answer and labels the remaining coverage
  gap. **Unsupported** says which input, format, or operation is unsupported and
  what the user can change. Neither state should be relabeled as a generic
  failure or hide a useful result.
- **Offline / unavailable** distinguishes an unreachable model/runtime from a
  folder that cannot be mounted. Keep saved history available; do not borrow a
  different Workspace's catalog or imply that old results were refreshed.
- **Error** names the action that failed, preserves saved content and editable
  drafts, and offers a relevant retry or recovery route. Do not show an empty
  state beside an error for the same failed load.

| Surface | Empty / not started | Loading and success | Partial or unsupported | Offline / error and next action |
| --- | --- | --- | --- | --- |
| **Focus** | General can answer without a folder. For file analysis, explain that no Workspace is in scope and offer **Choose a folder** or sample data. | Keep the active target visible while a run is in progress; when an answer lands, link to its Investigation and evidence. | Keep the best-supported answer visible; state which sources or checks were unavailable and offer a scoped follow-up. | Keep the Conversation and prior saved answers. For a provider/runtime issue, show the failed action and retry or Settings; for an unavailable folder, offer reconnect and history-only access. |
| **Investigation** | Without a Workspace, explain the scope requirement. With a Workspace and zero Investigations, offer to start from the current Conversation or create one. | Identify whether records or a particular run are loading; show the saved Investigation when available. | Keep Question coverage, answer status, and the reason for each gap visible. A documented gap may be closed as incomplete, but must not be labeled fully answered. | A failed list/run load is an error, not “No investigations yet.” Preserve the selected Investigation and offer retry; do not replace existing records with an empty placeholder. |
| **Workbench** | An optional companion that has not been opened is simply absent; explain when a requested preview or detail is unavailable. | Show progress in the affected panel; on success, retain its link to the Investigation, run, and source. | Keep other panels usable and identify the specific unsupported artifact, skipped step, or failed check. | Show error and retry state inside the affected panel. Do not clear the Conversation or Investigation because one detail panel failed. |
| **Data & context** | A successful empty catalog or absent `fella.md` is distinct from a failed read. Explain what is available and how to add context. | Distinguish scanning/loading from a completed inventory or loaded context note. | Identify skipped or unsupported files and keep the available inventory and context visible. | On a failed read, preserve existing text and do not present an empty editable document that could overwrite it. Offer retry; on save failure, keep the draft and make retry available. |
| **Projects and history** | Explain that no Project or saved Conversation exists in this Workspace and offer the relevant create/open action. | Indicate which list is loading; after success, retain Workspace ownership and saved links. | Keep existing curation and history visible even if one linked record is unavailable. | A missing folder is history-only, not deleted history. Keep saved Conversations and Project records reachable and offer reconnect; a failed list load must not be shown as a successful empty list. |

### Unsaved changes

Changing Focus/Investigation/Workbench, switching the Workspace pane, selecting
another Workspace or Environment, opening a different Conversation, closing a
Workspace/Environment, or deleting a Workspace must not silently discard an
editable draft. Before leaving, the user can save in the editor; otherwise a
platform confirmation offers **Discard** or **Stay**. Staying keeps the draft.
Choosing Discard reverts it to the last persisted version. A failed save keeps
the draft and exposes retry; navigation is blocked while a save is in flight.
A successfully saved or autosaved draft needs no confirmation. Running analysis
is separate from a draft: switching views must not cancel it; cancellation
requires an explicit stop action. OS-level quit protection is not covered by
the current in-app navigation guard.

The policy applies to the Investigation brief, Question plan, definitions,
Finding edits, workspace context, and any other editable form. An autosaving
surface must flush a pending save before leaving or use the same confirmation
when it cannot. A saved report only reflects saved records; it must disclose
unsaved edits rather than implying they are included.

### Migration and restoration

The navigation change is a non-destructive remapping of access, not a rewrite
of analytical records:

| Existing surface | Destination | Preservation rule |
| --- | --- | --- |
| Ask and its Conversations | Focus | Keep Conversation IDs, messages, Workspace scope, and run links unchanged. |
| Work plan and Results | Sections of the Investigation | Keep Investigation, Question, answer, Finding, run, evidence, and report records; do not create parallel copies. |
| Sources and Guide | Data & context | Keep source identity and the existing Workspace-local `fella.md`; do not change folder contents during navigation migration. |
| Workspace board and environment controls | Environment composition and Workspace selection | Preserve Environment, pane membership/order, and each pane's Conversation association. Layout must not merge Workspace data. |
| Conversation history | The owning Workspace's history | Keep unavailable-folder history accessible and label it history-only until reconnect. |
| Projects | The same Workspace's optional curation | Preserve Project IDs and links to their selected Investigation records. |

On first launch, open an unbound General Conversation in Focus. On return,
restore the saved Environment, active Workspace pane, and its Conversation.
Restore the last analytical view (Focus, Investigation, or Workbench),
Workspace subview, and selected Investigation for that pane when those records
still exist in that Workspace. If a saved Investigation no longer exists, select
the Investigation linked to the active Conversation; otherwise select the
most recently updated active Investigation. If there is none, show the normal
empty state. If the saved Workspace is unavailable, retain its Conversation
history, show the reconnect action, and do not restore a catalog-dependent
view. Settings, Project curation, and the multi-Workspace board are utility or
composition destinations; reopening the app returns to the saved analytical
view rather than unexpectedly reopening one of them.

These are target interaction rules, not a claim that every screen already
implements them. The session store now persists each pane's last analytical
view, Workspace subview, selected Investigation, and Conversation association.
Navigation guards protect drafts in Data & context and the Investigation brief,
Question plan, definitions, Finding forms, and editable proposal; an in-flight
save blocks navigation. Investigation-list and context-file read failures show
retry states instead of successful empty/editor states. Other surfaces still
need to adopt this shared state contract, and the restoration behavior needs
visual verification in the desktop app.

### Environment and Workspace scope

An Environment is a saved arrangement of open panes, not a shared analysis
scope. The Workspace board is the access point for the panes open in that
Environment; the Workspaces sidebar remains the way to open another folder.
General may appear beside folder Workspaces, but it remains an unbound
conversation rather than a data source.

- Exactly one pane is active at a time. Its header marks it **Active**, and the
  composer names its current message target immediately above the input.
- Inactive panes are previews, not second live work surfaces. They identify the
  Workspace, its associated Investigation and coverage/status, and the recent
  conversation; selecting one makes its own conversation active.
- Switching panes changes which Conversation receives a new message. It does
  not move an existing request, transcript, Investigation, or source scope. If
  a request is still running after a switch, its original Workspace preview
  continues to show that work as in progress.
- A multi-pane Environment does not combine sources or authorize
  cross-Workspace analysis. Any future comparison across Workspaces must be an
  explicit capability with a separately defined scope and user action.

## How the views relate

### Scroll ownership and long content

- Focus owns one keyboard-focusable conversation scroll region. In Workbench,
  each visible panel scrolls independently; scrolling a result or Investigation
  must not move another panel.
- The full Investigation view owns one main vertical scroll region. Dense,
  bounded lists (for example, a Question's source set) may scroll internally
  when their item count warrants it; those lists remain keyboard-scrollable,
  expose a count or continuation cue, and wrap full source names and paths.
- Long Investigation, run, and source titles wrap instead of disappearing
  behind ellipses. Large query/output blocks and wide result tables may have
  their own focusable scroll region where needed. A shortened inactive-pane
  preview is clearly identified as a preview and opens the complete content
  when selected.

These rules keep ordinary reading in one predictable scroll area while making
large, dense data inspectable without clipping or forcing unrelated panes to
move together.

- A user can ask in Focus without choosing a view first. Fella creates or
  associates the appropriate workspace-scoped Investigation as part of the
  analysis flow. In General, the conversation remains unbound unless the user
  explicitly chooses a data scope.
- Focus, Investigation, and Workbench present the same underlying records.
  Moving between them does not duplicate a Conversation, rerun analysis,
  change source scope, or rewrite provenance.
- A follow-up within the same work stays attached to its Investigation. Moving
  to another Workspace or Investigation must make that change clear; an
  environment layout alone never changes scope.
- The interface may reveal more structure as the analysis warrants it. A short
  question should remain lightweight; a complex or continuing analysis should
  make its Questions, steps, evidence, and unresolved work easier to inspect.
- The model decides the analytical work. The analyst can steer at any point,
  but is not required to prescribe or approve each routine step.

## Mapping current surfaces to the target model

This mapping gives existing capabilities a coherent role; it does not remove
or relocate them by itself. Navigation order and migration details are scoped
to [#242](https://github.com/Avijit-Kumar-GIT/fella/issues/242).

| Current surface or capability | Target role |
| --- | --- |
| Ask and conversation composer | Focus: the simple entry point for a question and its answer. |
| Conversation history | Workspace-owned history that reopens the Conversation and its linked Investigation; not a separate analytical work object. |
| Work plan and Results modes | Sections of one Investigation view, rather than competing destinations. |
| Analysis details, run history, and evidence | Contextual details in the Investigation record, with deeper inspection available in Workbench. |
| Guide and workspace context | Supporting Workspace context that can inform an Investigation; not a separate primary analysis mode. |
| Sources and source previews | Workspace Data and source-scope support. They may remain a separate utility, but do not replace the Investigation as the main work surface. |
| Workspace board and environment tabs | Environment composition and Workspace selection. They do not represent additional analysis views or imply cross-Workspace analysis. |
| Projects | Optional curation of selected Findings and artifacts, separate from the execution history of an Investigation. |
| Search and Settings | App-level utilities, not core analytical views. |

## Navigation and return behavior

The sidebar is organized around starting a conversation and selecting a data
scope. **New conversation** creates a thread in the currently selected scope;
it does not create an Investigation until the user submits a workspace
question. Workspace rows show which Workspace is active, keep its saved
conversations grouped with it, and provide access to its Investigations and
optional Project outputs. An unavailable folder keeps its saved history
available and offers a reconnect action.

For a fresh install, Fella opens an unbound **General** conversation. The user
can type a question first, choose a folder, or try the sample workspace. Choosing
a folder enters Focus for that Workspace and shows the readable and skipped
files. Submitting a Workspace question creates or associates its Investigation
automatically; the user does not have to create an Investigation object or
choose a view first.

For returning users, Fella restores the saved active Environment, pane, and
Conversation. The selected Workspace and current Investigation are identified
in the Workspace context bar. Switching an Environment changes the selected
layout; selecting a different Workspace changes the analysis scope. Neither
action combines Workspace data. Focus, Investigation, and Workbench are view
choices for that scoped work, while **Data & context** remains supporting
Workspace information. Search and Settings remain available in the sidebar's
utility area. Project curation stays attached to its Workspace, and saved
conversation history stays grouped with the Workspace that owns it.

These navigation changes reorganize access only: they do not delete or rewrite
saved Conversations, Investigations, Findings, reports, Projects, or layouts.

## Terminology and UI language

The [product model](PRODUCT-MODEL.md) is canonical for domain-object
definitions. Use these distinctions consistently in the interface:

| Term | Use it to mean | Avoid using it to mean |
| --- | --- | --- |
| **Workspace** | The explicit data boundary for a mounted folder. | General, an environment tab, pane arrangement, or Investigation. |
| **General** | An unbound conversation unless the user explicitly chooses a data scope. | A hidden Workspace or an implicit whole-disk search. |
| **Environment** | A saved arrangement of Workspace panes. | A data scope or a combined dataset. |
| **Investigation** | The durable analytical work and its lifecycle. | A single prompt, answer bubble, or notebook cell. |
| **Conversation** | The interaction thread used to ask, clarify, direct, and review. | The complete durable record of analysis. |
| **Question** | The original request or a useful part of it. | A required stage that must be answered before any result can be shown. |
| **Analysis run** | One bounded execution with its own inputs, scope snapshot, results, and provenance. | The Investigation's full lifecycle. |
| **Step** | A concise record of a meaningful operation the model actually performed. | A mandatory checklist item or a transcript of private reasoning. |
| **Evidence** | A source- and revision-linked observation or computation result. | The model's prose about a result. |
| **Finding** | A conclusion linked to evidence, checks, and caveats. | An unsupported confidence label. |
| **Report / handoff** | A shareable projection generated from saved Investigation records, with coverage, evidence, provenance, and limitations. | A separate source of truth or an unlinked summary of chat. |
| **Focus / Investigation / Workbench** | The three views of the same analytical work. | Separate agents, independent scopes, or required workflow stages. |

Use sentence case, direct action labels, and observable status language. Prefer
“Choose a folder,” “Ask a question,” “Open investigation,” “View analysis
details,” and “Share result.” For status and guidance, prefer precise, observable
language over anthropomorphic or misleading copy:

| Prefer | Avoid |
| --- | --- |
| “Checking selected files” | “Fella is thinking” |
| “Partially answered” with the remaining gap | “Failed” when a supported partial answer exists |
| “Not checked” | “Passed” when no check ran |
| “View analysis details” | “View the model’s private reasoning” |

Distinguish states such as “Partially answered,” “Needs your input,” and “Not
checked” from a failed run; explain what is missing and what the user can do
next. Do not use a single confidence badge to collapse observations,
calculations, model proposals, and analyst-confirmed meaning.

## Boundaries for this issue

- This document defines view responsibilities and terminology; it does not
  implement navigation or redesign the screens.
- It does not change the analytics engine, execution loop, or data-access
  boundary.
- It does not make a fixed set of steps, approval gates, or technical details a
  prerequisite for answering a question.
- It does not change Workspace composition, source-provider behavior, Projects,
  or stored user data.
- Follow-on visual design, navigation, Investigation layout, Workbench
  composition, scrolling, migration, and usability validation remain in
  [#241](https://github.com/Avijit-Kumar-GIT/fella/issues/241) through
  [#251](https://github.com/Avijit-Kumar-GIT/fella/issues/251).

# Fella interaction model

**Status:** UX architecture contract for [#240](https://github.com/Avijit-Kumar-GIT/fella/issues/240). This defines product direction and terminology; it does not claim every view or behavior is implemented.

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

## How the views relate

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

# Fella product model

**Status:** Current domain model, October 2026. It describes the product's
objects and their ownership. For shipped capabilities see [Product](./PRODUCT.md);
for the user experience see [Interaction model](./INTERACTION-MODEL.md).

## Keep data scope separate from window layout

Fella uses “workspace” for two related but different ideas:

- A **mounted Workspace** is a selected local folder. It defines which files,
  catalog, context, and source revision an analysis can use.
- The **window layout** arranges Chat and the optional Notebook surface. It
  changes where existing work appears; it never changes data access or scope.
- The **app workspace** organizes content in the Fella window and owns Reports.
  It is independent of the mounted folder; each saved output retains its own
  mounted source scope and revision.

The titlebar's Workspace selector refers to the mounted folder. Notebook is a
view of work in the same window, not another mounted folder or analytical
workspace.

## Objects and ownership

| Object | Owns | Does not own |
| --- | --- | --- |
| **Mounted Workspace** | Folder identity, catalog, revision, workspace context, and local knowledge. | Window layout, Reports, a conversation transcript, or a copy of source files. |
| **App workspace** | The window's collection of Reports and their organization. | Source access, catalog state, or analytical execution. |
| **Source** | A workspace-relative file or supported sheet, plus its observed catalog metadata. | A claim that it was used in a particular run. |
| **Conversation** | Ordered messages, the user's immediate requests, and the selected folder scope when applicable. | The canonical analysis trace or a required Investigation. |
| **Investigation** | A named, durable analytical effort: brief, source choices, saved Questions, status, and links to its Conversation and runs. | Duplicated message transcripts, tool outputs, or evidence payloads. |
| **Question** | One answerable part of an Investigation and its work/coverage state. | Proof that a run occurred or that the answer is correct. |
| **Analysis turn** | One execution's request, workspace/source revision, interpretation, tool trace, result, checks, evidence, and rerun lineage. | Later edits to the Investigation or a generated presentation. |
| **Evidence and checks** | Observations, computations, provenance, and what a specific verification check establishes. | A guarantee that the business interpretation is correct. |
| **Saved result / artifact** | A reusable output linked to the producing run, source revision, and presentation revision. | A new analytical execution when it is reopened or rearranged. |
| **Report** | A user-organized app-workspace document that references saved results from one or more mounted folders and can be exported. | A second analytics engine or a silent recomputation. |

## Relationships

```text
Mounted Workspace ──contains──> Sources, context, and revisions
        │
        ├──scopes──> Conversation ──contains──> Messages
        │                │
        │                └──linked when explicitly created──> Investigation
        │                                                       ├── Brief
        │                                                       ├── Questions
        │                                                       ├── Analysis turns
        │                                                       └── Findings and saved results
        │                                                               └── Evidence and checks
App workspace ──owns──> Reports ──reference──> Saved results / artifacts
                                                │
                                                └──retain source scope──> Mounted Workspace / run / evidence
```

A Conversation may exist without an Investigation. An Investigation has a
stable Workspace and a linked Conversation. An analysis turn records the
revision and scope it actually used; later catalog changes do not rewrite that
history.

## Lifecycle and invariants

- **Chat stays lightweight.** Creating a Conversation does not create an
  Investigation. Creating an Investigation is explicit and starts its linked
  Conversation.
- **Ordinary messages stay ordinary.** A follow-up can use conversation and
  Investigation context, but it does not silently add a saved Question or claim
  that a Question-specific run occurred. Explicitly analyzing a saved Question
  records that relationship.
- **Deletion is clear.** Deleting a Conversation deletes its transcript and
  linked Investigation. Saved results, reports, supporting analysis records,
  and revisions remain in the app workspace. Their artifact records retain the
  mounted source scope and evidence that produced them; the confirmation
  explains this.
- **Runs are historical records.** Reruns append new records. They preserve
  their original source revision, scope, method, result, and evidence.
- **Presentation has no analytical authority.** Generated components can
  display or locally filter supplied saved results. They cannot query files,
  run SQL/Python, alter results, or change verification state.
- **Provenance remains attached.** Reopening, arranging, or exporting a saved
  result does not rerun its analysis. Changed inputs can make an output stale;
  they do not silently replace its original values.
- **Meaning is attributable.** Observed data, computed values, model proposals,
  and user-confirmed definitions stay distinguishable.

## Analytical revisions and diffs

An analytical diff is a reviewable comparison between an existing saved result
and a proposed revision. It should show changes by layer: source scope and
revision; definitions, filters, joins, or method; exact output values; findings
and evidence; verification state; and any reports or saved results affected.
Unchanged layers should be identified too. A changed number alone is not enough
to explain why the analysis changed.

Keep the prior revision immutable. A candidate must retain its own run,
provenance, and output; adopting or rejecting it must not overwrite the
baseline. Distinguish analytical changes from presentation-only changes, and
never imply that dependent results were recomputed when they were not. The
[versioned analytical-diff contract](../testing/ANALYTICAL-DIFF-v1.md) records
the detailed acceptance criteria.

## User-facing map

The left sidebar contains **Chat**, **Files**, and **Reports**. In Chat, saved
Investigations and ordinary Questions/conversations remain discoverable.
**Notebook** is an optional view arrangement that places Chat beside the
selected Investigation or tool. The Investigation's inner sections are
**Brief**, **Sources**, **Questions**, **Analysis**, and **Results**. See the
[interaction model](./INTERACTION-MODEL.md) for navigation details.

The app does not have a separate Studio product, Projects/wiki layer, workspace
board, or terminal feature. Older planning documents that used those names are
retired; use **Reports** for saved report documents and **Notebook** for the
optional Chat-plus- Investigation layout.

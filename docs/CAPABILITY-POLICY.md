# Capability policy

**Status:** Experimental local policy for the personal release. This document
also records the enterprise direction for later; the enterprise items are not
implemented in the current product.

## Why this exists

Fella has a small number of distinct ways to analyze a mounted folder. A user
may want to turn one of them off without losing the rest of the product. The
capability policy gives those paths stable names and lets Settings control them
with simple on/off switches.

This is deliberately smaller than a plugin system and narrower than a general
security policy. The policy controls which analysis paths are available to the
agent. Fella's read-only boundary, workspace discovery, evidence capture,
verification, cancellation, and query limits remain core behavior and cannot be
turned off here.

## Current personal implementation

Settings exposes four experimental switches. They are stored locally with the
other Fella settings. Existing installations default every switch to `on`, so
adding the policy does not change the normal product surface.

| Capability id | Includes | Current enforcement |
|---|---|---|
| `table_analysis` | Schema context, table inspection, samples, and read-only SQL | The agent does not receive `inspect_table` or `run_sql`; direct engine calls are rejected too. |
| `document_analysis` | Search and reading for catalogued text and PDF sources | The agent does not receive `grep_files` or `read_file`; direct engine calls are rejected too. |
| `python_analysis` | Sandboxed RustPython/WASM calculations | The agent does not receive `run_python`; direct engine calls are rejected too. |
| `visualizations` | Validated structured charts | The agent does not receive `make_chart`; charts also require `table_analysis`. |

`list_files` remains available because it describes what is mounted rather than
performing an analysis. Evidence and verification remain available so every
answer continues to explain what happened. The model receives a short notice
when one or more paths are disabled and is told to explain when a requested
analysis is unavailable.

The policy is intentionally local and user controlled in this release. There
is no account, organization profile, license check, administrator role, or
remote policy service. It is not an enterprise feature hidden behind a switch;
it is a small personal control surface that gives the engine a future seam.

## Where it lives in the code

- `src-tauri/src/engine/capabilities.rs` defines the stable capability ids,
  defaults, chart dependency, and model-facing notice.
- `src-tauri/src/engine/sqlite.rs` persists the switches as local settings.
- `src-tauri/src/engine/tools.rs` builds the model tool registry from the
  current policy.
- `src-tauri/src/engine/state.rs` enforces the same policy at the engine
  boundary and filters the schema context.
- `src/lib/components/SettingsView.svelte` renders the experimental switches.
- `src/lib/types.ts` keeps the IPC shape shared with the frontend.

The registry and engine checks intentionally overlap. The registry keeps the
model from attempting unavailable work; the engine check protects command and
future call paths that bypass the registry.

## Product rule for the personal release

Keep the policy readable. A person should be able to answer three questions
without learning the internals:

1. What kind of analysis does this switch control?
2. What will stop working if I turn it off?
3. Can I turn it back on without changing my files?

The answer to the third question should remain yes. Capability changes affect
the next run and do not modify the mounted folder.

Do not add a separate profile chooser, capability marketplace, policy editor,
or enterprise login to the personal product merely because this seam exists.
The current switches are useful only if they remain easy to understand.

## Enterprise reference direction

The long-term enterprise opportunity is not primarily more buttons or more
charts. It is controlled context: helping the model understand the language,
definitions, exceptions, and constraints a business has accumulated over many
years while preserving a trustworthy audit trail.

An enterprise edition can use the same local engine and harness, then add
policy and context adapters around them. The following ideas are reference
material for that later work.

### Deployment and identity

- An initial onboarding choice between a personal workspace and an
  organization-managed workspace.
- Organization sign-in only when an organization actually manages the
  workspace; the personal path remains local and BYOK.
- Organization and user identity attached to policy decisions, context
  ownership, and audit events.
- Deployment profiles for a local laptop, a managed desktop, or a controlled
  enterprise host without creating a separate analytics engine.
- Managed folder mounts and approved data locations for centrally governed
  workspaces.

### Entitlements and policy administration

- License or entitlement claims that enable capabilities an organization has
  purchased or approved.
- Organization and user defaults and hard limits for providers, models, tools,
  network access, Python resource budgets, exports, and retention.
- User preferences that can turn an allowed capability off locally.
- A clear policy precedence model:

  ```text
  platform safety ceiling
    > organization policy
    > license entitlement
    > user preference
    > workspace and model availability
  ```

- A reason shown to the user when a capability is unavailable: disabled by the
  user, unavailable in the workspace, blocked by organization policy, or not
  included in the license.
- Policy snapshots attached to an answer so a reviewer can tell which rules
  were active when the answer was produced.

### Context engineering

- Shared business glossaries for terms such as “active customer,” “revenue,”
  “headcount,” or “retention.”
- Metric definitions with owners, formulas, source tables, units, and effective
  dates.
- Relationships between tables, approved join keys, grain, time zones, and
  known duplicate behavior.
- Exceptions and operating rules written in plain language, including when a
  normal definition does not apply.
- Source ownership, freshness, lineage, sensitivity, and confidence metadata.
- Versioned context with review and approval rather than silently replacing
  an old definition.
- Conflict handling when a user instruction, a workspace `fella.md`, a team
  definition, and a source note disagree.
- Context retrieval that selects only relevant definitions for a question so
  the prompt stays small and understandable.
- A visible explanation of which context was used in an answer, alongside the
  existing query and evidence.

### Enterprise analysis capabilities

Advanced analysis can eventually be enabled for either personal or enterprise
users when it is safe and understandable. It should not be called enterprise
only just because an organization may benefit from it. Possible future
capabilities include forecasting, cohort analysis, anomaly detection, richer
statistical tests, semantic joins, and approved external data sources. Each
should have a bounded implementation, a stable id, deterministic evidence, and
an understandable failure mode before it becomes a switch.

### Governance and operations

- SSO and role based access for organization-managed workspaces.
- Separate permissions for viewing data, editing shared context, changing
  policy, and exporting results.
- Audit records for questions, tools, policy state, source snapshots, context
  versions, and exports, with configurable retention.
- Data classification and redaction rules before information reaches a model
  provider.
- Approved provider and model routing, including a local or private endpoint
  requirement for sensitive workspaces.
- Network egress controls and allowlists for any future connector.
- CPU, memory, runtime, row, and output budgets tuned for shared machines.
- Workspace health, source freshness, failed ingestion, and policy diagnostics.
- Controlled export and sharing paths with provenance attached.
- Optional organization managed updates and compatibility checks.

### Extensibility boundary

The future extension point should be reviewed capabilities and host adapters,
not arbitrary code loaded into the core process. A connector or advanced
analysis should declare its data access, network needs, resource limits,
evidence format, and policy requirements. The core harness should still own
the question lifecycle, cancellation, evidence, verification, and final
read-only boundary.

## Revisit criteria

Keep this experimental policy when it helps users answer “what can Fella use
for this question?” without making Settings feel like an administration
console. Revisit the design only when there is a real second analysis family,
a real shared-context need, or a real organization policy requirement. At that
point, extend the stable capability model and documentation before adding a
new runtime subsystem.

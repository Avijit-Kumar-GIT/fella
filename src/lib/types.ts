// Shared types between the UI and the Rust engine. Keep in sync with
// src-tauri/src/engine/*.rs (serde-serialized).

export type Role = 'user' | 'assistant' | 'system';

export interface ChartSeries {
	name: string;
	values: (number | null)[];
}

export type ChartKind =
	| 'auto'
	| 'bar'
	| 'line'
	| 'pie'
	| 'donut'
	| 'scatter'
	| 'histogram'
	| 'box_plot'
	| 'area'
	| 'stacked_area'
	| 'heatmap'
	| 'forecast';

export interface ChartMetadata {
	source_evidence_id?: string;
	source_label?: string;
	fields: string[];
	aggregation?: string;
	filters: string[];
	time_range?: string;
	denominator?: string;
	part_to_whole: boolean;
	missing_treatment?: string;
}

export type ChartPayload =
	| { type: 'scatter'; points: { x: number; y: number; label: string; group?: string }[] }
	| {
			type: 'box_plot';
			groups: {
				label: string;
				low_whisker: number;
				q1: number;
				median: number;
				q3: number;
				high_whisker: number;
				outliers: number[];
				n: number;
			}[];
	  }
	| { type: 'heatmap'; x_labels: string[]; y_labels: string[]; values: (number | null)[][] }
	| {
			type: 'forecast';
			observed: (number | null)[];
			forecast: (number | null)[];
			lower: (number | null)[];
			upper: (number | null)[];
			uncertainty_note?: string;
	  };

/** Typed visualization data from a chart tool (e.g. `make_chart`) -- labels
 *  and numbers only, never markup. Rendered by `$lib/components/Chart.svelte`. */
export interface VisualizationSpec {
	kind: Exclude<ChartKind, 'auto'>;
	title?: string;
	labels: string[];
	series: ChartSeries[];
	unit?: string;
	x_label?: string;
	y_label?: string;
	payload?: ChartPayload;
	metadata?: ChartMetadata;
}

/** Compatibility name used by chart-facing components. */
export type ChartSpec = VisualizationSpec;

export interface EvidenceItem {
	/** Stable within one answer; older archived answers may not have one. */
	id?: string;
	tool: string;
	sources?: EvidenceSource[];
	args: Record<string, unknown>;
	/** One plain sentence the model wrote describing what this step does, for a
	 *  non-technical reader (e.g. "Add up spending by month"). */
	note?: string;
	/** SQL text, when the tool ran a query. */
	sql?: string;
	/** Short human-readable summary of the result. */
	result_summary: string;
	/** Column names, when the result was tabular. */
	columns?: string[];
	/** A capped sample of result rows. */
	rows?: unknown[][];
	row_count?: number;
	/** Free-form text output, e.g. Python stdout/stderr. */
	output?: string;
	/** Structured visualization data, when the tool was `make_chart`. */
	chart?: VisualizationSpec;
	/** Typed derived table published by Python or forecast analysis. */
	result_table?: { columns: string[]; rows: unknown[][] };
	/** Bounded SQL/schema references for data read by a Python computation. */
	python_input_trace?: PythonInputTrace;
	ms: number;
	error?: string;
}

export interface PythonInputTrace {
	complete: boolean;
	queries: {
		sql: string;
		columns: string[];
		row_count: number;
		truncated: boolean;
	}[];
}

/** Observable answer inputs. Context means supplied to the model, not proof of
 *  private reasoning; evidence IDs refer to the answer's evidence list. */
export interface AnswerProvenance {
	evidence_ids: string[];
	context_sections: ContextSection[];
	clarification_of?: string;
}

export interface EvidenceSource {
	table: string;
	source: string;
	note?: string;
}

export interface VerificationCheck {
	label: string;
	ok: boolean;
	detail?: string;
}

export type VerificationStatus = 'verified' | 'needs_review' | 'insufficient_data' | 'failed';

export interface Answer {
	/** Runtime turn id; older archived answers may not have one. */
	turn_id?: string;
	/** Compact execution summary; raw SQL/rows remain in evidence. */
	trace?: ExecutionTrace;
	/** Validated strategy used for this answer, when the runtime produced one. */
	plan?: {
		strategy: 'direct_tools' | 'compiled_sql' | 'python_fallback' | 'document_fallback';
		steps: string[];
	};
	/** Model-proposed semantic interpretation, when contract-first routing ran. */
	contract?: AnalysisContract;
	/** Deterministic field/value probes used to ground that interpretation. */
	grounding?: GroundingReport;
	/** Typed links to evidence, included context, and clarification lineage. */
	provenance?: AnswerProvenance;
	text: string;
	evidence: EvidenceItem[];
	verification: VerificationCheck[];
	/** One user-facing semantic choice that must be answered before computing. */
	clarification?: ClarificationRequest;
	/** Optional for archived answers written before typed verification status. */
	status?: VerificationStatus;
	workspace?: { path: string; revision: string };
	/** Token counts for the whole run, when the provider reported them. */
	usage?: { prompt_tokens: number; completion_tokens: number };
}

export type InterpretationStatus = 'unresolved' | 'grounded' | 'assumed' | 'ambiguous' | 'unsupported';

export interface ClarificationRequest {
	question: string;
	options: string[];
	reason?: string;
}

/** A selection from a pending clarification card, linked to its source turn. */
export interface ClarificationReply {
	turn_id: string;
	response: string;
}

export type ContextSection =
	| 'user_context'
	| 'workspace_schema'
	| 'workspace_model'
	| 'conversation'
	| 'folder_memory';

/** Length-only account of bounded context; never includes the source text. */
export interface ContextSectionAudit {
	section: ContextSection;
	source_chars: number;
	included_chars: number;
	truncated: boolean;
}

export interface ContextAssemblyAudit {
	sections: ContextSectionAudit[];
}

export interface AnalysisContract {
	interpretation: InterpretationStatus;
	subject?: string;
	grain?: string;
	measures: { concept: string; field?: string; operation: string; unit?: string }[];
	filters: {
		concept: string;
		field?: string;
		exclude?: boolean;
		candidate_values: string[];
		resolved_values: string[];
		resolution?: string;
	}[];
	time?: { field?: string; range?: string; bucket?: 'year' | 'month' | 'week' | 'day'; timezone?: string };
	group_by: string[];
	order_by?: { by: string; direction: 'asc' | 'desc' };
	limit?: number;
	derived_metrics?: {
		concept: string;
		kind: 'ratio' | 'difference';
		numerator: string;
		denominator: string;
		unit?: string;
	}[];
	joins?: {
		left_source: string;
		left_field: string;
		right_source: string;
		right_field: string;
		kind: 'inner' | 'left';
	}[];
	comparison_spec?: {
		kind: 'period_over_period';
		current_range: string;
		previous_range: string;
	};
	/** Legacy freeform field retained for archived contracts; it is not a deterministic plan. */
	comparison?: string;
	presentation?: string;
	assumptions: string[];
	unresolved: string[];
	clarification?: ClarificationRequest;
}

export type ProbeOutcome = 'resolved' | 'not_observed' | 'ambiguous' | 'unavailable';

export interface GroundingProbe {
	kind: string;
	target: string;
	outcome: ProbeOutcome;
	detail: string;
}

export interface GroundingReport {
	source?: string;
	sources?: string[];
	probes: GroundingProbe[];
	unresolved: string[];
}

export type RuntimeTurnState =
	| 'received'
	| 'interpreting'
	| 'grounding'
	| 'planning'
	| 'executing'
	| 'verifying'
	| 'accepted'
	| 'clarify'
	| 'retry'
	| 'needs_review'
	| 'unsupported'
	| 'failed'
	| 'cancelled';

export interface ExecutionTraceStep {
	id: string;
	operation: string;
	duration_ms: number;
	success: boolean;
	summary?: string;
	sources: string[];
}

export interface ExecutionTrace {
	id: string;
	turn_id: string;
	workspace_revision?: string;
	steps: ExecutionTraceStep[];
}

export interface WorkspaceColumnSnapshot {
	name: string;
	type: string;
}

export interface WorkspaceSourceSnapshot {
	name: string;
	view?: string;
	kind: string;
	row_count?: number;
	columns: WorkspaceColumnSnapshot[];
	size_bytes: number;
	mtime: number;
}

export interface WorkspaceRevisionSnapshot {
	path: string;
	revision: string;
	sources: WorkspaceSourceSnapshot[];
	skipped: string[];
}

/** Backend-owned record persisted for one analytical turn. */
export interface AnalysisTurn {
	id: string;
	conversation_id: string;
	question: string;
	context_refs?: ContextReference[];
	clarification_of?: string;
	clarification_response?: string;
	workspace?: string;
	workspace_revision?: string;
	workspace_snapshot?: WorkspaceRevisionSnapshot;
	context_audit?: ContextAssemblyAudit;
	provenance?: AnswerProvenance;
	rerun_of?: string;
	state: RuntimeTurnState;
	contract?: AnalysisContract;
	plan: { strategy: 'direct_tools' | 'compiled_sql' | 'python_fallback' | 'document_fallback'; steps: string[] };
	trace: ExecutionTrace;
	verification?: { status: VerificationStatus; checks: VerificationCheck[] };
	result: {
		text: string;
		status: VerificationStatus;
		usage?: { prompt_tokens: number; completion_tokens: number };
		verification: VerificationCheck[];
		evidence: unknown[];
	};
}

export type WorkspaceChangeKind = 'added' | 'removed' | 'changed';

export interface WorkspaceSourceChange {
	name: string;
	kind: WorkspaceChangeKind;
	details: string[];
}

/** Freshness/diff information shown before inspecting or rerunning a turn. */
export interface AnalysisTurnReplayStatus {
	turn_id: string;
	workspace?: string;
	original_revision?: string;
	current_revision?: string;
	same_workspace: boolean;
	revision_changed: boolean;
	snapshot_available: boolean;
	can_rerun: boolean;
	source_changes: WorkspaceSourceChange[];
}

/** The two user-facing ways to work with a mounted workspace. Ask is the
 * default conversational surface; Inspect is the stricter source-first path
 * with a read-only tool registry. */
export type AskMode = 'ask' | 'inspect';

/** A small, local reference attached to the next conversation turn. It is a
 * UI affordance for choosing context; the engine still decides which files to
 * read and the backend remains the source of truth for access. */
export type ContextReference =
	| { kind: 'source'; key: string; label: string; detail?: string }
	| { kind: 'column'; key: string; label: string; detail?: string };

/** One observable step in a local question run. Kept in the conversation so
 * switching tabs never loses the small amount of run history shown in the UI. */
export interface RunStep {
	id: string;
	label: string;
	state: 'running' | 'complete' | 'error';
	started_at_ms: number;
	finished_at_ms?: number;
	tool?: string;
	note?: string;
	evidence?: EvidenceItem;
}

export interface Message {
	id: string;
	role: Role;
	text: string;
	/** The one-line plan the model streamed before its first tool call, kept
	 *  visible (dimmed) while the tools run. Cleared when the answer lands. */
	plan?: string;
	/** Present on assistant messages once the answer is complete. */
	answer?: Answer;
	/** True while the assistant message is still streaming. */
	pending?: boolean;
	ts: number;
}

export type SourceKind =
	| 'csv'
	| 'tsv'
	| 'parquet'
	| 'json'
	| 'ndjson'
	| 'xlsx'
	| 'pdf'
	| 'text';

export interface SourceInfo {
	name: string;
	path: string;
	kind: SourceKind;
	/** DuckDB view name, for tabular sources. */
	view?: string;
	row_count?: number;
	columns?: ColumnInfo[];
	size_bytes: number;
	mtime: number;
	/** First line of a text document, for the catalog listing. */
	synopsis?: string;
	/** Ingest caveat about the whole source (preamble skipped, totals row dropped). */
	note?: string;
}

export interface ColumnInfo {
	name: string;
	type: string;
	null_fraction?: number;
	distinct?: number;
	min?: string;
	max?: string;
	example?: string;
	/** A few frequent values for low-cardinality label columns. */
	common_values?: string[];
	/** Ingest caveat: amounts coerced from text, or a mixed column left as text. */
	note?: string;
}

export interface SkippedFile {
	name: string;
	reason: string;
}

export interface Catalog {
	workspace: string | null;
	revision?: string;
	/** Unix milliseconds when this workspace snapshot was indexed. */
	indexed_at_ms?: number;
	sources: SourceInfo[];
	/** Files found but not loaded (unsupported type, unreadable, parse failure).
	 *  Absent when nothing was skipped. */
	skipped?: SkippedFile[];
}

export type FieldRole = 'date' | 'measure' | 'dimension' | 'identifier' | 'text';

export interface FieldProfile extends ColumnInfo {
	role: FieldRole;
}

export interface SourceModel {
	name: string;
	path: string;
	kind: SourceKind;
	view?: string;
	row_count?: number;
	fields: FieldProfile[];
	size_bytes: number;
	mtime: number;
	synopsis?: string;
	note?: string;
}

/** A naming-based join hypothesis; the backend still grounds and probes joins. */
export interface RelationshipCandidate {
	left_source: string;
	left_field: string;
	right_source: string;
	right_field: string;
	evidence: string;
}

/** Revision-bound semantic projection of a mounted workspace. */
export interface WorkspaceModel {
	workspace: string;
	revision: string;
	indexed_at_ms?: number;
	sources: SourceModel[];
	relationships?: RelationshipCandidate[];
	relationships_truncated?: boolean;
	skipped?: SkippedFile[];
}

export interface AnalysisCapabilities {
	table_analysis: boolean;
	document_analysis: boolean;
	python_analysis: boolean;
	visualizations: boolean;
}

export interface Settings {
	/** A provider id from the registry (`ollama-cloud`, `openai`, `vercel`, `xai`, `custom`, …). */
	provider: string;
	base_url: string;
	model: string;
	embed_model: string;
	/** A usable credential exists for `provider` (or it needs none). */
	has_credential: boolean;
	/** Local switches for the analysis paths exposed to the model. */
	capabilities: AnalysisCapabilities;
}

/** One built-in provider, as returned by `list_providers`. */
export interface ProviderInfo {
	id: string;
	display: string;
	/** `"none"` or `"key"`. */
	auth: 'none' | 'key';
	base_url: string;
	/** Page to get an API key from; empty when N/A. */
	get_key_url: string;
	/** Provider exposes an embeddings endpoint (needed for doc search). */
	embeddings: boolean;
	/** A credential is present, or none is needed. */
	authed: boolean;
	/** The currently-selected provider. */
	current: boolean;
}

export interface ProviderHealth {
	reachable: boolean;
	/** Endpoint answered with 401/403: it's up, but the key is wrong or
	 *  unauthorized. Always false when `reachable`. */
	rejected: boolean;
	models: string[];
}

export interface AppInfo {
	name: string;
	version: string;
	uptime_ms: number;
}

export interface QueryResult {
	columns: string[];
	rows: unknown[][];
	row_count: number;
	ms: number;
	truncated: boolean;
}

/** Result of `/update` checking (and possibly applying) a new release. */
export interface UpdateStatus {
	current: string;
	latest: string;
	/** True only if a newer release exists; false once an update has been
	 * kicked off (the app is about to exit) or when already up to date. */
	available: boolean;
}

/** One row of `/history`'s list — enough to recognize and pick a past
 * conversation without knowing its id. */
export interface ConversationSummary {
	id: string;
	saved_at_ms: number;
	workspace: string | null;
	preview: string;
	message_count: number;
	/** A user-given name, if this conversation was renamed. */
	title: string | null;
}

/** A user-created local wiki attached to one mounted repository. */
export interface Project {
	id: string;
	name: string;
	workspace: string;
	body: string;
	created_at_ms: number;
	updated_at_ms: number;
}

/** Streaming events emitted by the `ask` command over a Tauri Channel. */
export type AskEvent =
	| { kind: 'turn_state'; turn_id: string; state: RuntimeTurnState }
	| { kind: 'assistant_delta'; text: string }
	| { kind: 'tool_start'; tool: string; args: Record<string, unknown> }
	| { kind: 'tool_end'; item: EvidenceItem }
	| { kind: 'notice'; text: string }
	| { kind: 'answer_done'; answer: Answer };

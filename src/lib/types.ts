// Shared types between the UI and the Rust engine. Keep in sync with
// src-tauri/src/engine/*.rs (serde-serialized).

export type Role = 'user' | 'assistant' | 'system';

export interface ChartSeries {
	name: string;
	values: number[];
}

/** Typed visualization data from a chart tool (e.g. `make_chart`) -- labels
 *  and numbers only, never markup. Rendered by `$lib/components/Chart.svelte`. */
export interface VisualizationSpec {
	kind: 'bar' | 'line';
	title?: string;
	labels: string[];
	series: ChartSeries[];
	unit?: string;
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
	ms: number;
	error?: string;
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
	text: string;
	evidence: EvidenceItem[];
	verification: VerificationCheck[];
	/** Optional for archived answers written before typed verification status. */
	status?: VerificationStatus;
	workspace?: { path: string; revision: string };
	/** Token counts for the whole run, when the provider reported them. */
	usage?: { prompt_tokens: number; completion_tokens: number };
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
	| { kind: 'assistant_delta'; text: string }
	| { kind: 'tool_start'; tool: string; args: Record<string, unknown> }
	| { kind: 'tool_end'; item: EvidenceItem }
	| { kind: 'notice'; text: string }
	| { kind: 'answer_done'; answer: Answer };

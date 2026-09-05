// Shared types between the UI and the Rust engine. Keep in sync with
// src-tauri/src/engine/*.rs (serde-serialized).

export type Role = 'user' | 'assistant' | 'system';

export interface EvidenceItem {
	tool: string;
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
	ms: number;
	error?: string;
}

export interface VerificationCheck {
	label: string;
	ok: boolean;
	detail?: string;
}

export interface Answer {
	text: string;
	evidence: EvidenceItem[];
	verification: VerificationCheck[];
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
	/** Ingest caveat: amounts coerced from text, or a mixed column left as text. */
	note?: string;
}

export interface SkippedFile {
	name: string;
	reason: string;
}

export interface Catalog {
	workspace: string | null;
	sources: SourceInfo[];
	/** Files found but not loaded (unsupported type, unreadable, parse failure).
	 *  Absent when nothing was skipped. */
	skipped?: SkippedFile[];
}

export interface Settings {
	/** A provider id from the registry (`ollama`, `openai`, `vercel`, `xai`, `custom`, …). */
	provider: string;
	base_url: string;
	model: string;
	embed_model: string;
	/** A usable credential exists for `provider` (or it needs none). */
	has_credential: boolean;
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

export interface OllamaHealth {
	reachable: boolean;
	/** Endpoint answered with 401/403: it's up, but the key is wrong or
	 *  unauthorized. Always false when `reachable`. */
	rejected: boolean;
	models: string[];
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

/** A pack: a theme, a skill, or an mcp connector. See docs/EXTENSIBILITY.md. */
export type PackKind = 'theme' | 'skill' | 'mcp';

/** One installed pack, as returned by the `packs_*` commands. */
export interface InstalledPack {
	id: string;
	kind: PackKind;
	name: string;
	version: string;
	description: string;
	/** `"local"` (side-loaded) or `"marketplace"`. */
	source: string;
	/** Installed from the reviewed marketplace. */
	verified: boolean;
	enabled: boolean;
	/** `mcp` packs: enabled but still missing the token `/connect` needs. */
	needs_token?: boolean;
}

/** Streaming events emitted by the `ask` command over a Tauri Channel. */
export type AskEvent =
	| { kind: 'assistant_delta'; text: string }
	| { kind: 'tool_start'; tool: string; args: Record<string, unknown> }
	| { kind: 'tool_end'; item: EvidenceItem }
	| { kind: 'notice'; text: string }
	| { kind: 'answer_done'; answer: Answer };

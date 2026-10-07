// Typed wrappers around the desktop command surface.
//
// The renderer uses one narrow, typed-in-practice bridge into the Electron
// main process and Rust analytics sidecar.

import type {
	Answer,
	AnalysisTurn,
	AnalysisTurnReplayStatus,
	AppInfo,
	AskMode,
	AskEvent,
	Catalog,
	ClarificationReply,
	ContextReference,
	ConversationSummary,
	ProviderHealth,
	ProviderInfo,
	QueryResult,
	RunLogEntry,
	Settings,
	SourceInfo,
	WorkspaceModel,
	UpdateStatus,
	WorkspaceProgress
} from './types';

export function isElectron(): boolean {
	return typeof window !== 'undefined' && typeof window.fella?.invoke === 'function';
}

export function isDesktop(): boolean {
	return isElectron();
}

/** Native folder picker. Returns the chosen path, or null if cancelled. */
export async function pickFolder(): Promise<string | null> {
	return isElectron() ? window.fella?.pickFolder() ?? null : null;
}

/** Open an https URL in the user's default browser. No-op outside the app. */
export async function openExternal(url: string): Promise<void> {
	if (isElectron()) await window.fella?.openExternal(url);
}

/** Window controls for the custom titlebar. No-op outside the app. */
async function windowAction(fn: 'minimize' | 'toggleMaximize' | 'close'): Promise<void> {
	if (isElectron()) await window.fella?.windowAction(fn);
}
export const win = {
	minimize: () => windowAction('minimize'),
	toggleMaximize: () => windowAction('toggleMaximize'),
	close: () => windowAction('close')
};

/** Keep the native Electron surface in step with the document theme. */
async function setWindowAppearance(dark: boolean): Promise<void> {
	if (isElectron()) await window.fella?.setWindowAppearance(dark);
}

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
	if (!isElectron() || !window.fella) {
		throw new Error(`ipc: "${cmd}" is unavailable outside the Electron app`);
	}
	return window.fella.invoke<T>(cmd, args);
}

export const ipc = {
	/** Signals the app is interactive; returns cold-start ms. */
	appReady: () => invoke<number>('app_ready'),
	appInfo: () => invoke<AppInfo>('app_info'),
	async openWorkspace(path: string, onProgress: (progress: WorkspaceProgress) => void) {
		if (!window.fella) throw new Error('Electron preload bridge is unavailable');
		return window.fella.openWorkspace(path, onProgress);
	},
	getCatalog: () => invoke<Catalog>('get_catalog'),
	getWorkspaceModel: () => invoke<WorkspaceModel | null>('get_workspace_model'),
	lastWorkspacePath: () => invoke<string | null>('last_workspace_path'),
	describe: (name: string) => invoke<SourceInfo>('describe', { name }),
	sampleSource: (name: string, rows = 5) => invoke<QueryResult>('sample_source', { name, rows }),
	runSqlDirect: (sql: string) => invoke<QueryResult>('run_sql_direct', { sql }),
	getSettings: () => invoke<Settings>('get_settings'),
	setSettings: (settings: Partial<Settings>) => invoke<Settings>('set_settings', { settings }),
	listProviders: () => invoke<ProviderInfo[]>('list_providers'),
	setApiKey: (provider: string, key: string) =>
		invoke<Settings>('set_api_key', { provider, key }),
	logout: (provider: string, forget = false) =>
		invoke<Settings>('logout', { provider, forget }),

	providerHealth: () => invoke<ProviderHealth>('provider_health'),
	setWindowAppearance,
	cancel: (conversationId: string) => invoke<void>('cancel', { conversationId }),
	forgetConversation: (conversationId: string) =>
		invoke<void>('forget_conversation', { conversationId }),
	reindex: () => invoke<Catalog>('reindex'),
	memoryFile: () => invoke<[string, string | null] | null>('memory_file'),
	forgetMemory: () => invoke<boolean>('forget_memory'),

	/** Read and write the explicit user-authored workspace context file. */
	contextFile: () => invoke<[string, string | null] | null>('context_file'),
	saveContext: (contents: string) => invoke<void>('save_context', { contents }),

	/** Check for a newer release and, if one exists, download + verify +
	 * install it and exit. Only ever called by `/update`; never automatic. */
	update: () => invoke<UpdateStatus>('update'),

	/** Archive a finished transcript to a file; resolves with its path. */
	archiveConversation: (id: string, body: string) =>
		invoke<string>('archive_conversation', { id, body }),
	/** Where archived conversations live, and how many there are. */
	conversationsInfo: () => invoke<{ path: string; count: number }>('conversations_info'),
	/** Every archived conversation, newest first, for `/history` to list. */
	conversationsList: () => invoke<ConversationSummary[]>('conversations_list'),
	/** Raw JSON of one archived conversation `{id, workspace, messages}`,
	 * matching what `archiveConversation` originally wrote. */
	conversationLoad: (id: string) => invoke<string>('conversation_load', { id }),
	/** Recent local-only run metadata; never includes transcript or workspace contents. */
	runLogRecent: (limit = 50) => invoke<RunLogEntry[]>('run_log_recent', { limit }),
	/** Load the canonical backend record for one analytical turn. */
	analysisTurnLoad: (turnId: string) => invoke<AnalysisTurn>('analysis_turn_load', { turnId }),
	/** Compare a stored turn's source snapshot with the mounted workspace. */
	analysisTurnReplayStatus: (turnId: string) =>
		invoke<AnalysisTurnReplayStatus>('analysis_turn_replay_status', { turnId }),
	/** Rerun a canonical turn against the currently mounted workspace. */
	async analysisTurnRerun(
		turnId: string,
		onEvent: (event: AskEvent) => void,
		model?: string,
		mode?: AskMode
	): Promise<Answer> {
		if (!window.fella) throw new Error('Electron preload bridge is unavailable');
		return window.fella.rerunAnalysisTurn(
			{ turnId, model: model || null, mode: mode || null },
			onEvent
		);
	},
	/** Remove one archived conversation from the sidebar's history. */
	deleteConversation: (id: string) => invoke<void>('delete_conversation', { id }),
	/** Set (empty string clears) a custom title, for a conversation not
	 *  necessarily open in a live tab right now. */
	renameConversation: (id: string, title: string) =>
		invoke<void>('rename_conversation', { id, title }),

	/**
	 * Ask a question. Streams progress through `onEvent`; resolves with the
	 * final answer. `model` is the calling tab's choice; omit for the default.
	 */
	async ask(
		conversationId: string,
		question: string,
		onEvent: (e: AskEvent) => void,
		model?: string,
		mode?: AskMode,
		contextRefs?: ContextReference[],
		clarificationReply?: ClarificationReply
	): Promise<Answer> {
		if (!window.fella) throw new Error('Electron preload bridge is unavailable');
		// Svelte state proxies cannot cross Electron's structured-clone boundary.
		// Project references into plain data before sending them to the main process.
		const wireContextRefs = (contextRefs ?? []).map((reference) => ({
			kind: reference.kind,
			key: reference.key,
			label: reference.label,
			...(reference.detail === undefined ? {} : { detail: reference.detail })
		}));
		return window.fella.ask(
			{
				conversationId,
				question,
				model: model || null,
				mode: mode || null,
				contextRefs: wireContextRefs,
				clarificationReply: clarificationReply ?? null
			},
			onEvent
		);
	}
};

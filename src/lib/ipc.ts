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
	Investigation,
	InvestigationDefinition,
	InvestigationDefinitionInput,
	InvestigationQuestion,
	InvestigationQuestionInput,
	QuestionBreakdownProposal,
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

function workspaceArgs(workspaceId?: string | null): Record<string, unknown> {
	return workspaceId ? { workspaceId } : {};
}

export const ipc = {
	/** Signals the app is interactive; returns cold-start ms. */
	appReady: () => invoke<number>('app_ready'),
	appInfo: () => invoke<AppInfo>('app_info'),
	async openWorkspace(path: string, onProgress: (progress: WorkspaceProgress) => void) {
		if (!window.fella) throw new Error('Electron preload bridge is unavailable');
		return window.fella.openWorkspace(path, onProgress);
	},
	getCatalog: (workspaceId?: string) => invoke<Catalog>('get_catalog', workspaceArgs(workspaceId)),
	closeWorkspace: (workspaceId: string) => invoke<void>('close_workspace', { workspaceId }),
	getWorkspaceModel: (workspaceId?: string) =>
		invoke<WorkspaceModel | null>('get_workspace_model', workspaceArgs(workspaceId)),
	lastWorkspacePath: () => invoke<string | null>('last_workspace_path'),
	describe: (name: string, workspaceId?: string) =>
		invoke<SourceInfo>('describe', { name, ...workspaceArgs(workspaceId) }),
	sampleSource: (name: string, rows = 5, workspaceId?: string) =>
		invoke<QueryResult>('sample_source', { name, rows, ...workspaceArgs(workspaceId) }),
	runSqlDirect: (sql: string, workspaceId?: string) =>
		invoke<QueryResult>('run_sql_direct', { sql, ...workspaceArgs(workspaceId) }),
	getSettings: () => invoke<Settings>('get_settings'),
	setSettings: (settings: Partial<Settings>) => invoke<Settings>('set_settings', { settings }),
	listProviders: () => invoke<ProviderInfo[]>('list_providers'),
	setApiKey: (provider: string, key: string) =>
		invoke<Settings>('set_api_key', { provider, key }),
	logout: (provider: string, forget = false) =>
		invoke<Settings>('logout', { provider, forget }),

	providerHealth: () => invoke<ProviderHealth>('provider_health'),
	setWindowAppearance,
	cancel: (conversationId: string, workspaceId?: string | null) =>
		invoke<void>('cancel', { conversationId, ...workspaceArgs(workspaceId) }),
	forgetConversation: (conversationId: string) =>
		invoke<void>('forget_conversation', { conversationId }),
	reindex: (workspaceId?: string) => invoke<Catalog>('reindex', workspaceArgs(workspaceId)),
	memoryFile: (workspaceId?: string) =>
		invoke<[string, string | null] | null>('memory_file', workspaceArgs(workspaceId)),
	forgetMemory: (workspaceId?: string) =>
		invoke<boolean>('forget_memory', workspaceArgs(workspaceId)),

	/** Read and write the explicit user-authored workspace context file. */
	contextFile: (workspaceId?: string) =>
		invoke<[string, string | null] | null>('context_file', workspaceArgs(workspaceId)),
	saveContext: (contents: string, workspaceId?: string) =>
		invoke<void>('save_context', { contents, ...workspaceArgs(workspaceId) }),

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
	/** Raw JSON of one archived conversation, including its transcript and
	 * optional conversation-scoped presentation state. */
	conversationLoad: (id: string) => invoke<string>('conversation_load', { id }),
	/** Recent local-only run metadata; never includes transcript or workspace contents. */
	runLogRecent: (limit = 50) => invoke<RunLogEntry[]>('run_log_recent', { limit }),
	/** Load the canonical backend record for one analytical turn. */
	analysisTurnLoad: (turnId: string) => invoke<AnalysisTurn>('analysis_turn_load', { turnId }),
	/** Create a durable workspace-scoped Investigation, optionally drafting its
	 *  brief from the original request without changing that Conversation message. */
	investigationCreate: (
		workspaceId: string,
		title = 'New investigation',
		initialRequest?: string,
		sourceMessageId?: string,
		conversationId?: string
	) => invoke<Investigation>('investigation_create', { workspaceId, title, initialRequest, sourceMessageId, conversationId }),
	/** Resolve or create the Investigation associated with this stable Conversation. */
	investigationEnsure: (workspaceId: string, conversationId: string, initialRequest: string, sourceMessageId: string) =>
		invoke<Investigation>('investigation_ensure', { workspaceId, conversationId, initialRequest, sourceMessageId }),
	/** Save the analyst-edited current brief on an Investigation. */
	investigationUpdateBrief: (
		investigationId: string,
		brief: Investigation['brief'],
		reviewedMessageId?: string
	) => invoke<Investigation>('investigation_update_brief', { investigationId, brief, reviewedMessageId }),
	/** Replace the current ordered Question plan without rewriting past runs. */
	investigationUpdateQuestions: (investigationId: string, questions: InvestigationQuestionInput[]) =>
		invoke<Investigation>('investigation_update_questions', { investigationId, questions }),
	/** Save the active analyst-authored meanings and assumptions for one Investigation. */
	investigationUpdateDefinitions: (investigationId: string, definitions: InvestigationDefinitionInput[]) =>
		invoke<Investigation>('investigation_update_definitions', { investigationId, definitions }),
	investigationQuestionLoad: (questionId: string) =>
		invoke<InvestigationQuestion>('investigation_question_load', { questionId }),
	investigationDefinitionLoad: (definitionId: string) =>
		invoke<InvestigationDefinition>('investigation_definition_load', { definitionId }),
	investigationProposeQuestions: (investigationId: string, mainQuestion: string) =>
		invoke<QuestionBreakdownProposal>('investigation_propose_questions', { investigationId, mainQuestion }),
	/** Associate existing canonical AnalysisTurns by ID; never copy run payloads. */
	investigationLinkAnalysisTurns: (investigationId: string, analysisTurnIds: string[]) =>
		invoke<Investigation>('investigation_link_analysis_turns', { investigationId, analysisTurnIds }),
	/** Load one Investigation independently of its conversation surface. */
	investigationLoad: (investigationId: string) =>
		invoke<Investigation>('investigation_load', { investigationId }),
	/** List durable Investigations, most recently updated first. */
	investigationsList: () => invoke<Investigation[]>('investigations_list'),
	/** Compare a stored turn's source snapshot with the mounted workspace. */
	analysisTurnReplayStatus: (turnId: string, workspaceId?: string | null) =>
		invoke<AnalysisTurnReplayStatus>('analysis_turn_replay_status', { turnId, ...workspaceArgs(workspaceId) }),
	/** Rerun a canonical turn against the currently mounted workspace. */
	async analysisTurnRerun(
		turnId: string,
		onEvent: (event: AskEvent) => void,
		model?: string,
		mode?: AskMode,
		workspaceId?: string | null
	): Promise<Answer> {
		if (!window.fella) throw new Error('Electron preload bridge is unavailable');
		return window.fella.rerunAnalysisTurn(
			{ turnId, model: model || null, mode: mode || null, workspaceId: workspaceId ?? null },
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
		clarificationReply?: ClarificationReply,
		workspaceId?: string | null,
		investigationId?: string | null
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
				clarificationReply: clarificationReply ?? null,
			workspaceId: workspaceId ?? null,
			investigationId: investigationId ?? null
			},
			onEvent
		);
	}
};

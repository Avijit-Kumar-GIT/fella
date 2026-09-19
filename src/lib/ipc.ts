// Typed wrappers around the Tauri command surface.
//
// Every call degrades gracefully when the app is opened in a plain browser
// (e.g. `pnpm dev` without Tauri, or `pnpm build` prerender): `isTauri()` is
// false and the callers fall back to local-only behaviour.

import type {
	Answer,
	AskMode,
	AskEvent,
	Catalog,
	ConversationSummary,
	ProviderHealth,
	ProviderInfo,
	QueryResult,
	Settings,
	SourceInfo,
	UpdateStatus
} from './types';

export function isTauri(): boolean {
	return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

/** Native folder picker. Returns the chosen path, or null if cancelled. */
export async function pickFolder(): Promise<string | null> {
	if (!isTauri()) return null;
	const { open } = await import('@tauri-apps/plugin-dialog');
	const picked = await open({ directory: true, multiple: false, title: 'Choose a folder' });
	return typeof picked === 'string' ? picked : null;
}

/** Open an https URL in the user's default browser. No-op outside the app. */
export async function openExternal(url: string): Promise<void> {
	if (!isTauri()) return;
	try {
		const { openUrl } = await import('@tauri-apps/plugin-opener');
		await openUrl(url);
	} catch {
		/* opener unavailable the URL is still shown as text to copy */
	}
}

/** Window controls for the custom titlebar. No-op outside the app. */
async function windowAction(fn: 'minimize' | 'toggleMaximize' | 'close'): Promise<void> {
	if (!isTauri()) return;
	try {
		const { getCurrentWindow } = await import('@tauri-apps/api/window');
		await getCurrentWindow()[fn]();
	} catch {
		/* window API unavailable ignore */
	}
}
export const win = {
	minimize: () => windowAction('minimize'),
	toggleMaximize: () => windowAction('toggleMaximize'),
	close: () => windowAction('close')
};

type InvokeFn = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;

let _invoke: InvokeFn | null = null;

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
	if (!isTauri()) {
		throw new Error(`ipc: "${cmd}" is unavailable outside the desktop app`);
	}
	if (!_invoke) {
		const core = await import('@tauri-apps/api/core');
		_invoke = core.invoke as InvokeFn;
	}
	return _invoke<T>(cmd, args);
}

export const ipc = {
	/** Signals the app is interactive; returns cold-start ms. */
	appReady: () => invoke<number>('app_ready'),
	openWorkspace: (path: string) => invoke<Catalog>('open_workspace', { path }),
	getCatalog: () => invoke<Catalog>('get_catalog'),
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
	setWindowAppearance: (dark: boolean) => invoke<void>('set_window_appearance', { dark }),
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

	/** Windows: pull the OS cursor-visibility counter back to >= 0 before a
	 * modal native dialog, so "hide pointer while typing" can't leave the
	 * pointer invisible inside the folder picker. No-op on other platforms. */
	unhideCursor: () => invoke<void>('unhide_cursor'),

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
		mode?: AskMode
	): Promise<Answer> {
		const { Channel } = await import('@tauri-apps/api/core');
		const channel = new Channel<AskEvent>();
		channel.onmessage = onEvent;
		return invoke<Answer>('ask', {
			conversationId,
			question,
			model: model || null,
			mode: mode || null,
			channel
		});
	}
};

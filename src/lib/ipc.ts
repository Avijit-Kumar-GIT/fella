// Typed wrappers around the Tauri command surface.
//
// Every call degrades gracefully when the app is opened in a plain browser
// (e.g. `pnpm dev` without Tauri, or `pnpm build` prerender): `isTauri()` is
// false and the callers fall back to local-only behaviour.

import type {
	Answer,
	AskEvent,
	Catalog,
	ConversationSummary,
	InstalledPack,
	OllamaHealth,
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
	describe: (name: string) => invoke<SourceInfo>('describe', { name }),
	runSqlDirect: (sql: string) => invoke<QueryResult>('run_sql_direct', { sql }),
	getSettings: () => invoke<Settings>('get_settings'),
	setSettings: (settings: Partial<Settings>) => invoke<Settings>('set_settings', { settings }),
	listProviders: () => invoke<ProviderInfo[]>('list_providers'),
	setApiKey: (provider: string, key: string) =>
		invoke<Settings>('set_api_key', { provider, key }),
	logout: (provider: string) => invoke<Settings>('logout', { provider }),

	ollamaHealth: () => invoke<OllamaHealth>('ollama_health'),
	/** Is a local Ollama running, regardless of the configured provider? */
	probeOllama: () => invoke<OllamaHealth>('probe_ollama'),
	cancel: (conversationId: string) => invoke<void>('cancel', { conversationId }),
	forgetConversation: (conversationId: string) =>
		invoke<void>('forget_conversation', { conversationId }),
	reindex: () => invoke<Catalog>('reindex'),

	/** Installed packs (themes, skills, mcp connectors). */
	packsList: () => invoke<InstalledPack[]>('packs_list'),
	/** Add a pack from a local directory; returns the updated list. */
	packsAdd: (path: string) => invoke<InstalledPack[]>('packs_add', { path }),
	packsRemove: (id: string) => invoke<InstalledPack[]>('packs_remove', { id }),
	packsSetEnabled: (id: string, enabled: boolean) =>
		invoke<InstalledPack[]>('packs_set_enabled', { id, enabled }),
	/** Install a pack from the marketplace by id (files are hash-checked). */
	packsInstall: (id: string) => invoke<InstalledPack[]>('packs_install', { id }),
	/** Store the token an `mcp` connector pack needs. */
	mcpSetToken: (id: string, token: string) => invoke<void>('mcp_set_token', { id, token }),
	/** Forget an `mcp` connector pack's token. */
	mcpClearToken: (id: string) => invoke<boolean>('mcp_clear_token', { id }),
	/** CSS token map of the active theme pack, or null. */
	packsTheme: () => invoke<Record<string, string> | null>('packs_theme'),

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

	/**
	 * Ask a question. Streams progress through `onEvent`; resolves with the
	 * final answer. `model` is the calling tab's choice; omit for the default.
	 */
	async ask(
		conversationId: string,
		question: string,
		onEvent: (e: AskEvent) => void,
		model?: string
	): Promise<Answer> {
		const { Channel } = await import('@tauri-apps/api/core');
		const channel = new Channel<AskEvent>();
		channel.onmessage = onEvent;
		return invoke<Answer>('ask', {
			conversationId,
			question,
			model: model || null,
			channel
		});
	}
};

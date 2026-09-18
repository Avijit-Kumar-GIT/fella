// Shared reactive session state (Svelte 5 runes in a .svelte.ts module).
//
// `Session` holds workspace-level state (one open folder, one model) plus an
// array of independent `Conversation` tabs. The per-conversation fields
// (`messages`, `busy`, `activity`, …) are exposed as getters/setters that
// proxy the active tab, so the many `session.addSystem(...)` call sites keep
// acting on whichever tab is focused. Long-lived work (`ask`) is handed the
// specific `Conversation` so it keeps streaming into its own tab after a
// switch.

import { ipc, isTauri } from './ipc';
import type {
	AskMode,
	Catalog,
	ContextReference,
	EvidenceItem,
	InspectorSelection,
	Message,
	ProviderHealth,
	ProviderInfo,
	RunStep,
	Settings
} from './types';


function uid(): string {
	return Math.random().toString(36).slice(2, 10);
}

function humanToolName(tool: string): string {
	const labels: Record<string, string> = {
		list_files: 'Looking through the workspace',
		inspect_file: 'Inspecting a source',
		read_file: 'Reading a source',
		search_files: 'Searching the workspace',
		query: 'Calculating from the data',
		make_chart: 'Preparing a chart',
		memory: 'Checking workspace notes'
	};
	return labels[tool] ?? tool.replace(/[_-]+/g, ' ').replace(/^./, (c) => c.toUpperCase());
}

const PREFIX = 'fella:conversation:'; // one key per tab: fella:conversation:<id>
const LEGACY_KEY = 'fella:conversation'; // the single pre-tabs blob
const INDEX_KEY = 'fella:tabs'; // JSON array of open tab ids
const SIDEBAR_KEY = 'fella:sidebar-collapsed';

function readSidebarCollapsed(): boolean {
	try {
		const v = localStorage.getItem(SIDEBAR_KEY);
		return v === null ? false : v === '1';
	} catch {
		return false;
	}
}


/** One conversation tab: its transcript, its in-flight run, its input history. */
export class Conversation {
	readonly kind = 'chat' as const;
	readonly id: string;
	messages = $state<Message[]>([]);
	busy = $state<boolean>(false);
	/** Transient one-line status shown while this tab's agent is working. */
	activity = $state<string>('');
	/** Set by `/login <provider>`: the next composer line is taken as the API
	 *  key for this provider not echoed to the transcript, not persisted. */
	pendingKey = $state<{ provider: string; display: string } | null>(null);
	/** ↑-recall history for the composer while this tab is focused. */
	history: string[] = [];
	/** The model this tab answers with. Empty = use the saved default. All tabs
	 *  share one provider / login; only the model is per-tab. */
	model = $state<string>('');
	/** A user-given name. null = derive one from the folder + first message,
	 *  the same as an un-renamed conversation always has. */
	title = $state<string | null>(null);
	/** The current question's intent. Inspect selects the stricter read-only
	 *  tool registry and makes the source-first workflow explicit to the model. */
	mode = $state<AskMode>('ask');
	/** Sources and fields chosen from the workspace for this tab. */
	contextRefs = $state<ContextReference[]>([]);
	/** The compact, local run trace shown beneath the transcript. */
	runSteps = $state<RunStep[]>([]);
	runStartedAt = $state<number | null>(null);
	runDurationMs = $state<number | null>(null);

	/** `id` defaults to a fresh one (a genuinely new conversation). Reopening
	 *  an archived conversation passes its real id back in, so re-archiving
	 *  it (on the next settle, or on close) overwrites the same file instead
	 *  of forking a duplicate under a new one. */
	constructor(id?: string) {
		this.id = id ?? uid();
	}

	#persistTimer: ReturnType<typeof setTimeout> | undefined;

	// NB: return the element *after* pushing `messages` is a `$state` proxy, so
	// the pushed plain object is only reactive when reached through the array.
	addUser(text: string): Message {
		this.messages.push({ id: uid(), role: 'user', text, ts: Date.now() });
		return this.messages[this.messages.length - 1];
	}
	addAssistant(text = ''): Message {
		this.messages.push({ id: uid(), role: 'assistant', text, pending: true, ts: Date.now() });
		return this.messages[this.messages.length - 1];
	}
	addSystem(text: string): Message {
		this.messages.push({ id: uid(), role: 'system', text, ts: Date.now() });
		return this.messages[this.messages.length - 1];
	}

	setMode(mode: AskMode): void {
		this.mode = mode;
	}

	addContext(ref: ContextReference): void {
		if (this.contextRefs.some((item) => item.kind === ref.kind && item.key === ref.key)) return;
		this.contextRefs = [...this.contextRefs, ref];
	}

	removeContext(kind: ContextReference['kind'], key: string): void {
		this.contextRefs = this.contextRefs.filter((item) => item.kind !== kind || item.key !== key);
	}

	clearContext(): void {
		this.contextRefs = [];
	}

	startRun(): void {
		const now = Date.now();
		this.runStartedAt = now;
		this.runDurationMs = null;
		this.runSteps = [
			{
				id: uid(),
				label: 'Planning the question',
				state: 'running',
				started_at_ms: now
			}
		];
	}

	beginRunStep(tool: string, note?: string): void {
		const now = Date.now();
		const steps = this.runSteps.map((step) =>
			step.state === 'running' && !step.tool
				? { ...step, state: 'complete' as const, finished_at_ms: now }
				: step
		);
		this.runSteps = [
			...steps,
			{
				id: uid(),
				label: note || humanToolName(tool),
				state: 'running',
				started_at_ms: now,
				tool,
				note
			}
		];
	}

	completeRunStep(item: EvidenceItem): void {
		const now = Date.now();
		const index = this.runSteps.findLastIndex(
			(step) => step.state === 'running' && (!step.tool || step.tool === item.tool)
		);
		if (index < 0) return;
		this.runSteps = this.runSteps.map((step, i) =>
			i === index
				? { ...step, state: item.error ? ('error' as const) : ('complete' as const), finished_at_ms: now, evidence: item }
				: step
		);
	}

	finishRun(error = false): void {
		const now = Date.now();
		this.runSteps = this.runSteps.map((step) =>
			step.state === 'running'
				? { ...step, state: error ? ('error' as const) : ('complete' as const), finished_at_ms: now }
				: step
		);
		if (this.runStartedAt !== null) this.runDurationMs = Math.max(0, now - this.runStartedAt);
		this.runStartedAt = null;
	}

	/** Coalesce writes while a run streams; flush immediately once it settles. */
	persist(workspace: string | null): void {
		if (this.messages.length === 0) return;
		clearTimeout(this.#persistTimer);
		if (this.busy) {
			this.#persistTimer = setTimeout(() => this.#writeSnapshot(workspace), 250);
		} else {
			this.#writeSnapshot(workspace);
		}
	}
	#writeSnapshot(workspace: string | null): void {
		try {
			localStorage.setItem(
				PREFIX + this.id,
				JSON.stringify({ id: this.id, workspace, messages: this.messages.slice(-200) })
			);
		} catch {
			/* ignore */
		}
	}
	dropSnapshot(): void {
		clearTimeout(this.#persistTimer);
		try {
			localStorage.removeItem(PREFIX + this.id);
		} catch {
			/* ignore */
		}
	}
}


export type Tab = Conversation;
export type WorkspaceView = 'ask' | 'workspace' | 'settings';
export type WorkspacePane = 'sources' | 'context';

class Session {
	catalog = $state<Catalog>({ workspace: null, sources: [] });
	/** The lightweight workspace surface currently shown beside the tab state. */
	workspaceView = $state<WorkspaceView>('ask');
	/** The active pane inside Workspace. */
	workspacePane = $state<WorkspacePane>('sources');
	/** Folder from the last session, if it still exists shown on the welcome
	 *  screen as a one-click "reopen". Fella no longer opens it automatically. */
	lastFolder = $state<string | null>(null);
	settings = $state<Settings | null>(null);
	health = $state<ProviderHealth | null>(null);
	/** Built-in providers from the engine, cached so the composer can hint
	 *  valid `/login` / `/logout` names without an await. */
	providers = $state<ProviderInfo[]>([]);
	/** The open conversation tabs and the focused index. */
	tabs = $state<Tab[]>([new Conversation()]);
	active = $state<number>(0);
	/** Focus mode: hide the tab strip and the folder header for a plain,
	 *  single-conversation view. Toggled by `/focus` or Ctrl+Shift+F. */
	focus = $state<boolean>(false);
	/** History sidebar visibility. Unlike `focus`, this is remembered across
	 *  launches (open by default) it's a layout preference, not a
	 *  per-session display mode. Toggled by the titlebar button or Ctrl+B. */
	sidebarCollapsed = $state<boolean>(readSidebarCollapsed());
	/** Bumped whenever a conversation is archived, so the sidebar's list
	 *  knows to refetch without polling. */
	historyVersion = $state<number>(0);
	/** Right-hand contextual inspector, shared by Ask and Workspace. */
	inspectorOpen = $state<boolean>(false);
	inspectorSelection = $state<InspectorSelection>(null);

	toggleSidebar(): void {
		this.sidebarCollapsed = !this.sidebarCollapsed;
		try {
			localStorage.setItem(SIDEBAR_KEY, this.sidebarCollapsed ? '1' : '0');
		} catch {
			/* ignore */
		}
	}

	setWorkspaceView(view: WorkspaceView): void {
		if (view === 'workspace' && !this.catalog.workspace) {
			this.workspaceView = 'ask';
			return;
		}
		this.workspaceView = view;
	}

	setWorkspacePane(pane: WorkspacePane): void {
		if (!this.catalog.workspace) {
			this.workspaceView = 'ask';
			return;
		}
		this.workspacePane = pane;
		this.workspaceView = 'workspace';
	}

	/** Focus a conversation tab. */
	activateTab(index: number): void {
		if (!this.tabs[index]) return;
		this.active = index;
		this.workspaceView = 'ask';
	}


	setAskMode(mode: AskMode): void {
		this.ensureChat().setMode(mode);
	}

	addContextReference(ref: ContextReference): void {
		this.ensureChat().addContext(ref);
	}

	removeContextReference(kind: ContextReference['kind'], key: string): void {
		this.activeChat?.removeContext(kind, key);
	}

	clearContextReferences(): void {
		this.activeChat?.clearContext();
	}

	openInspector(selection: InspectorSelection): void {
		this.inspectorSelection = selection;
		this.inspectorOpen = selection !== null;
	}

	closeInspector(): void {
		this.inspectorOpen = false;
		this.inspectorSelection = null;
	}


	get activeTab(): Tab {
		return this.tabs[this.active] ?? this.tabs[0];
	}

	/** The focused conversation used by the command and ask paths. */
	get activeChat(): Conversation {
		return this.activeTab;
	}

	/** The active tab's model, or the saved default when it hasn't picked one. */
	get model(): string {
		const m = this.activeTab.model;
		return m || this.settings?.model || '';
	}

	// --- per-conversation facade -> the active (or first) conversation ------
	get messages(): Message[] {
		return this.activeChat?.messages ?? [];
	}
	set messages(v: Message[]) {
		if (this.activeChat) this.activeChat.messages = v;
	}
	get conversationId(): string {
		return this.activeChat?.id ?? this.activeTab.id;
	}
	get busy(): boolean {
		return this.activeChat?.busy ?? false;
	}
	set busy(v: boolean) {
		if (this.activeChat) this.activeChat.busy = v;
	}
	get activity(): string {
		return this.activeChat?.activity ?? '';
	}
	set activity(v: string) {
		if (this.activeChat) this.activeChat.activity = v;
	}
	get pendingKey(): { provider: string; display: string } | null {
		return this.activeChat?.pendingKey ?? null;
	}
	set pendingKey(v: { provider: string; display: string } | null) {
		if (this.activeChat) this.activeChat.pendingKey = v;
	}
	/** The conversation to act on for a slash command. */
	ensureChat(): Conversation {
		return this.activeChat;
	}

	addUser(text: string): Message {
		return this.ensureChat().addUser(text);
	}
	addAssistant(text = ''): Message {
		return this.ensureChat().addAssistant(text);
	}
	addSystem(text: string): Message {
		return this.ensureChat().addSystem(text);
	}

	// --- tab management ---------------------------------------------------
	newTab(): void {
		const inherit = this.model; // provider + login are shared; carry the model
		const c = new Conversation();
		c.model = inherit;
		this.tabs.push(c);
		this.activateTab(this.tabs.length - 1);
		this.#writeIndex();
	}

	/** Open an archived conversation (`/history <n>`) in a new tab, its
	 *  transcript exactly as saved. If `workspace` doesn't match the folder
	 *  open right now, the caller is responsible for warning that new
	 *  questions here will run against the current folder, not the
	 *  original one there's only ever one open folder for every tab. */
	loadArchivedTab(id: string, messages: Message[], title: string | null = null): void {
		// Already open (e.g. the very conversation you're re-clicking in the
		// sidebar) -- focus it instead of forking a second live copy under
		// the same id, which would collide as a duplicate tab key.
		const existing = this.tabs.findIndex((t) => t.kind === 'chat' && t.id === id);
		if (existing >= 0) {
			this.active = existing;
			return;
		}
		const inherit = this.model; // same convention as newTab()
		const c = new Conversation(id);
		c.model = inherit;
		c.title = title;
		// A reloaded transcript never has a run in flight.
		c.messages = messages.map((m) => (m.pending ? { ...m, pending: false } : m));
		this.tabs.push(c);
		this.activateTab(this.tabs.length - 1);
		this.#writeIndex();
	}

	/** Rename a conversation, live or archived-only. `title` empty/whitespace
	 *  clears a custom name back to the derived folder + first-message one. */
	async renameConversation(id: string, title: string): Promise<void> {
		const trimmed = title.trim();
		const tab = this.tabs.find(
			(t): t is Conversation => t.kind === 'chat' && t.id === id
		);
		if (tab) {
			tab.title = trimmed || null;
			await this.#archive(tab);
			return;
		}
		if (!isTauri()) return;
		await ipc.renameConversation(id, trimmed);
		this.historyVersion++;
	}


	async closeTab(i: number): Promise<void> {
		const tab = this.tabs[i];
		if (!tab) return;
		await this.#archive(tab);
		tab.dropSnapshot();
		if (isTauri()) void ipc.forgetConversation(tab.id).catch(() => {});
		this.tabs.splice(i, 1);
		if (this.tabs.length === 0) this.tabs.push(new Conversation());
		if (this.active > i) this.active -= 1;
		this.active = Math.min(this.active, this.tabs.length - 1);
		this.#writeIndex();
	}

	/** Remove a chat tab WITHOUT archiving it -- for when its history entry
	 *  was just deleted from the sidebar. Deleting only ever removed the
	 *  archived file; if that conversation was still open as a live tab, the
	 *  very next settle re-archived it via persist()'s "archive on content"
	 *  behaviour, silently undoing the delete (and, since persist() sweeps
	 *  every open tab on any single tab's activity, resurrecting every other
	 *  deleted-but-still-open conversation right along with it). No-op if
	 *  the conversation isn't currently open. */
	removeTabWithoutArchiving(id: string): void {
		const i = this.tabs.findIndex((t) => t.kind === 'chat' && t.id === id);
		if (i < 0) return;
		const tab = this.tabs[i] as Conversation;
		tab.dropSnapshot();
		if (isTauri()) void ipc.forgetConversation(tab.id).catch(() => {});
		this.tabs.splice(i, 1);
		if (this.tabs.length === 0) this.tabs.push(new Conversation());
		if (this.active > i) this.active -= 1;
		this.active = Math.min(this.active, this.tabs.length - 1);
		this.#writeIndex();
	}

	/** End the active conversation and start the slot blank. */
	async clear(): Promise<void> {
		const tab = this.activeChat;
		await this.#archive(tab);
		tab.dropSnapshot();
		this.tabs[this.active] = new Conversation();
		this.#writeIndex();
	}

	/** Called once on launch. Archives every transcript a previous run left
	 *  behind (one blob per tab, plus the legacy single blob) and starts fresh
	 *  with one empty tab transcripts are never restored. */
	async rollOver(): Promise<void> {
		let keys: string[];
		try {
			keys = Object.keys(localStorage).filter((k) => k === LEGACY_KEY || k.startsWith(PREFIX));
		} catch {
			this.tabs = [new Conversation()];
			this.active = 0;
			return;
		}
		for (const k of keys) {
			let raw: string | null = null;
			try {
				raw = localStorage.getItem(k);
			} catch {
				continue;
			}
			// Remove the key unless the archive call itself failed (then keep it
			// for the next launch to retry).
			if (await this.#archiveRaw(raw)) {
				try {
					localStorage.removeItem(k);
				} catch {
					/* ignore */
				}
			}
		}
		try {
			localStorage.removeItem(INDEX_KEY);
		} catch {
			/* ignore */
		}
		this.tabs = [new Conversation()];
		this.active = 0;
		this.historyVersion++;
	}

	/** Persist every conversation tab's transcript (each debounces its own
	 *  write).
	 *
	 *  Also archives a tab into history as soon as its first exchange
	 *  settles, not just on close/clear -- otherwise a conversation you're
	 *  still actively having doesn't show up in the sidebar until you're
	 *  done with it, which reads as "it didn't save" rather than "it hasn't
	 *  been archived yet". `#archive` re-runs (and just overwrites the same
	 *  file) on every later settle too, so the sidebar's title/preview
	 *  reflects the real conversation even if you never close the tab. */
	persist(): void {
		const ws = this.catalog.workspace ?? null;
		for (const t of this.tabs) {
			t.persist(ws);
			if (!t.busy && t.messages.length > 0) void this.#archive(t);
		}
		this.#writeIndex();
	}

	#writeIndex(): void {
		try {
			localStorage.setItem(INDEX_KEY, JSON.stringify(this.tabs.map((t) => t.id)));
		} catch {
			/* ignore */
		}
	}

	async #archive(tab: Conversation): Promise<void> {
		if (tab.messages.length === 0 || !isTauri()) return;
		const body = JSON.stringify({
			id: tab.id,
			saved_at_ms: Date.now(),
			workspace: this.catalog.workspace ?? null,
			messages: tab.messages,
			title: tab.title ?? undefined
		});
		try {
			await ipc.archiveConversation(tab.id, body);
			this.historyVersion++;
		} catch (e) {
			console.warn('archive failed', e);
		}
	}

	/** Returns true when the blob has been dealt with (archived, or not worth
	 *  keeping); false only when the archive IPC threw. */
	async #archiveRaw(raw: string | null): Promise<boolean> {
		if (!raw) return true;
		let saved: { id?: string; workspace?: string | null; messages?: unknown };
		try {
			saved = JSON.parse(raw);
		} catch {
			return true;
		}
		if (!Array.isArray(saved.messages) || saved.messages.length === 0 || !isTauri()) return true;
		// This is a previous run's leftover conversation, recovered on launch --
		// date it by its own last message, not by "now" (the relaunch moment),
		// or it files under today's date no matter how long ago it happened.
		const last = saved.messages[saved.messages.length - 1] as { ts?: number } | undefined;
		const body = JSON.stringify({
			id: saved.id ?? '',
			saved_at_ms: last?.ts ?? Date.now(),
			workspace: saved.workspace ?? this.catalog.workspace ?? null,
			messages: saved.messages
		});
		try {
			await ipc.archiveConversation(String(saved.id ?? ''), body);
			return true;
		} catch (e) {
			console.warn('archive failed keeping the transcript for the next launch', e);
			return false;
		}
	}
}

export const session = new Session();

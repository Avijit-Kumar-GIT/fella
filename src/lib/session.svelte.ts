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
	AugmentConfig,
	AnalysisArtifact,
	Answer,
	Catalog,
	InstalledPack,
	Message,
	OllamaHealth,
	ProviderInfo,
	Settings
} from './types';

export type { AugmentConfig };

function uid(): string {
	return Math.random().toString(36).slice(2, 10);
}

const PREFIX = 'fella:conversation:'; // one key per tab: fella:conversation:<id>
const LEGACY_KEY = 'fella:conversation'; // the single pre-tabs blob
const INDEX_KEY = 'fella:tabs'; // JSON array of open tab ids
const SIDEBAR_KEY = 'fella:sidebar-collapsed';
const ANALYSES_KEY = 'fella:analyses';

function readSidebarCollapsed(): boolean {
	try {
		const v = localStorage.getItem(SIDEBAR_KEY);
		return v === null ? false : v === '1';
	} catch {
		return false;
	}
}

function readAnalyses(): AnalysisArtifact[] {
	if (typeof localStorage === 'undefined') return [];
	try {
		const raw = localStorage.getItem(ANALYSES_KEY);
		if (!raw) return [];
		const parsed: unknown = JSON.parse(raw);
		return Array.isArray(parsed) ? (parsed as AnalysisArtifact[]) : [];
	} catch {
		return [];
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
	/** Set by `/connect <id>`: the next composer line is the connector's key
	 *  same masked-input treatment as `pendingKey`. */
	pendingConnect = $state<{ id: string } | null>(null);
	/** ↑-recall history for the composer while this tab is focused. */
	history: string[] = [];
	/** The model this tab answers with. Empty = use the saved default. All tabs
	 *  share one provider / login; only the model is per-tab. */
	model = $state<string>('');
	/** A user-given name. null = derive one from the folder + first message,
	 *  the same as an un-renamed conversation always has. */
	title = $state<string | null>(null);

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

/** A non-conversation tab: an augment view (a notes buffer, later a grid)
 *  editing one file in the open folder. Not persisted to localStorage the
 *  file on disk is the source of truth and it isn't restored on relaunch. */
export class AugmentTab {
	readonly kind = 'augment' as const;
	readonly id = uid();
	readonly capability: string;
	readonly command: string;
	readonly file: string;
	readonly syntax: string;
	/** Current editor contents. */
	text = $state<string>('');
	/** Last value confirmed written to disk. */
	saved = $state<string>('');
	savedAt = $state<number | null>(null);
	saving = $state<boolean>(false);
	loadError = $state<string | null>(null);

	constructor(cfg: AugmentConfig) {
		this.capability = cfg.capability;
		this.command = cfg.command;
		this.file = cfg.file;
		this.syntax = cfg.syntax;
	}

	get dirty(): boolean {
		return this.text !== this.saved;
	}
}

export type Tab = Conversation | AugmentTab;
export type WorkspaceView = 'home' | 'ask' | 'sources' | 'analyses' | 'context';

class Session {
	catalog = $state<Catalog>({ workspace: null, sources: [] });
	/** The lightweight workspace surface currently shown beside the tab state. */
	workspaceView = $state<WorkspaceView>('ask');
	/** Personal saved analyses. These stay local until a durable artifact store
	 *  exists; the answer itself remains the source of truth for its details. */
	analyses = $state<AnalysisArtifact[]>(readAnalyses());
	selectedAnalysisId = $state<string | null>(null);
	/** Folder from the last session, if it still exists shown on the welcome
	 *  screen as a one-click "reopen". Fella no longer opens it automatically. */
	lastFolder = $state<string | null>(null);
	settings = $state<Settings | null>(null);
	health = $state<OllamaHealth | null>(null);
	/** A local Ollama probed regardless of the configured provider so the empty
	 *  screen can offer "use Ollama" when it's installed after signing in
	 *  elsewhere. */
	ollamaLocal = $state<OllamaHealth | null>(null);
	/** Built-in providers from the engine, cached so the composer can hint
	 *  valid `/login` / `/logout` names without an await. */
	providers = $state<ProviderInfo[]>([]);
	/** Installed packs, cached so `/packs` completion can offer ids without an
	 *  await. */
	packs = $state<InstalledPack[]>([]);
	/** Augment capabilities this build ships (from the engine, not a hardcoded
	 *  list) an augment pack is only reachable if its capability is here. */
	augmentCapabilities = $state<string[]>([]);

	/** The open tabs (conversations, and augment views), and the focused index. */
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

	toggleSidebar(): void {
		this.sidebarCollapsed = !this.sidebarCollapsed;
		try {
			localStorage.setItem(SIDEBAR_KEY, this.sidebarCollapsed ? '1' : '0');
		} catch {
			/* ignore */
		}
	}

	setWorkspaceView(view: WorkspaceView): void {
		// Sources and Context describe a mounted workspace. If the catalog is
		// empty, keep the user on the useful no-folder onboarding surface instead
		// of showing a dead-end pane.
		if (!this.catalog.workspace && (view === 'sources' || view === 'context')) {
			this.workspaceView = 'ask';
			return;
		}
		this.workspaceView = view;
		// An augment is an editor surface, but Ask should always return to a
		// conversation rather than leaving the user on a hidden file tab.
		if (view === 'ask' && this.activeTab.kind === 'augment') {
			const chat = this.tabs.findIndex((t) => t.kind === 'chat');
			if (chat >= 0) this.active = chat;
		}
	}

	/** Focus a tab and show the surface that belongs to it. Chat and ordinary
	 *  augment tabs live in Ask; the context augment has its own workspace pane. */
	activateTab(index: number): void {
		const tab = this.tabs[index];
		if (!tab) return;
		this.active = index;
		this.workspaceView = tab.kind === 'augment' && tab.command === 'context' ? 'context' : 'ask';
	}

	selectAnalysis(id: string | null): void {
		this.selectedAnalysisId = id;
	}

	isAnalysisSaved(messageId: string): boolean {
		return this.analyses.some((analysis) => analysis.message_id === messageId);
	}

	saveAnalysis(messageId: string, question: string, answer: Answer): AnalysisArtifact {
		const existing = this.analyses.find((analysis) => analysis.message_id === messageId);
		const cleanQuestion = question.replace(/\s+/g, ' ').trim();
		const title = cleanQuestion
			? cleanQuestion.length > 72
				? cleanQuestion.slice(0, 69) + '…'
				: cleanQuestion
			: 'Saved analysis';
		// Answers are reactive objects while the transcript is alive. Take a
		// serializable snapshot so later streaming updates cannot mutate an
		// artifact that the user already saved.
		const answerSnapshot = JSON.parse(JSON.stringify(answer)) as Answer;
		const artifact: AnalysisArtifact = {
			id: existing?.id ?? uid(),
			message_id: messageId,
			title,
			question: cleanQuestion || 'Saved analysis',
			answer: answerSnapshot,
			created_at_ms: existing?.created_at_ms ?? Date.now()
		};
		this.analyses = [artifact, ...this.analyses.filter((item) => item.message_id !== messageId)];
		this.selectedAnalysisId = artifact.id;
		this.#writeAnalyses();
		return artifact;
	}

	deleteAnalysis(id: string): void {
		this.analyses = this.analyses.filter((analysis) => analysis.id !== id);
		if (this.selectedAnalysisId === id) this.selectedAnalysisId = this.analyses[0]?.id ?? null;
		this.#writeAnalyses();
	}

	get activeTab(): Tab {
		return this.tabs[this.active] ?? this.tabs[0];
	}

	/** The focused tab if it's a conversation, else the first conversation tab,
	 *  else null. The `session.addSystem(...)` / `ask` paths route here so a
	 *  slash command run while an augment tab is focused still lands somewhere
	 *  sensible. */
	get activeChat(): Conversation | null {
		const t = this.activeTab;
		if (t.kind === 'chat') return t;
		return (this.tabs.find((x) => x.kind === 'chat') as Conversation | undefined) ?? null;
	}

	/** The active tab's model, or the saved default when it hasn't picked one. */
	get model(): string {
		const m = this.activeTab.kind === 'chat' ? this.activeTab.model : '';
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
	get pendingConnect(): { id: string } | null {
		return this.activeChat?.pendingConnect ?? null;
	}
	set pendingConnect(v: { id: string } | null) {
		if (this.activeChat) this.activeChat.pendingConnect = v;
	}

	/** The conversation to act on for a slash command: the focused tab if it's a
	 *  conversation, otherwise the first conversation tab (focusing it), or a
	 *  fresh one. Slash commands are dispatched from the Composer, which is
	 *  hidden on an augment tab, so in practice this is just the active tab. */
	ensureChat(): Conversation {
		const existing = this.activeChat;
		if (existing) {
			if (this.activeTab.kind !== 'chat') this.active = this.tabs.indexOf(existing);
			return existing;
		}
		this.newTab();
		return this.tabs[this.active] as Conversation;
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

	/** Open (or focus) an augment view for `cfg`, loading the file's current
	 *  contents from the open folder. */
	async openAugment(cfg: AugmentConfig): Promise<void> {
		const existing = this.tabs.findIndex((t) => t.kind === 'augment' && t.file === cfg.file);
		if (existing >= 0) {
			this.activateTab(existing);
			return;
		}
		const tab = new AugmentTab(cfg);
		this.tabs.push(tab);
		this.activateTab(this.tabs.length - 1);
		this.#writeIndex();
		if (!isTauri()) return;
		try {
			const cur = await ipc.augmentLoad(cfg.file);
			tab.text = cur ?? '';
			tab.saved = tab.text;
		} catch (e) {
			tab.loadError = e instanceof Error ? e.message : String(e);
		}
	}

	async closeTab(i: number): Promise<void> {
		const tab = this.tabs[i];
		if (!tab) return;
		const closingContext = tab.kind === 'augment' && tab.command === 'context';
		if (tab.kind === 'chat') {
			await this.#archive(tab);
			tab.dropSnapshot();
			if (isTauri()) void ipc.forgetConversation(tab.id).catch(() => {});
		} else if (tab.dirty && isTauri()) {
			// Flush a final save so nothing typed is lost on close.
			try {
				await ipc.augmentSave(tab.capability, tab.file, tab.text);
			} catch (e) {
				console.warn('augment final save failed', e);
			}
		}
		this.tabs.splice(i, 1);
		if (this.tabs.length === 0) this.tabs.push(new Conversation());
		if (closingContext && this.workspaceView === 'context') this.workspaceView = 'ask';
		// Keep the focus on the same tab where possible: shift left if we closed
		// one before it, then clamp.
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

	/** End the active tab: archive a conversation / flush an augment, then start
	 *  the slot blank. */
	async clear(): Promise<void> {
		const tab = this.activeTab;
		if (tab.kind === 'chat') {
			await this.#archive(tab);
			tab.dropSnapshot();
		} else if (tab.dirty && isTauri()) {
			try {
				await ipc.augmentSave(tab.capability, tab.file, tab.text);
			} catch {
				/* ignore */
			}
		}
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
	 *  write). Augment tabs aren't persisted the file on disk is the truth.
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
			if (t.kind !== 'chat') continue;
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

	#writeAnalyses(): void {
		try {
			localStorage.setItem(ANALYSES_KEY, JSON.stringify(this.analyses));
		} catch {
			/* a full or unavailable local store should not interrupt a conversation */
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

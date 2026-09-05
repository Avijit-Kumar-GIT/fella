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
	Catalog,
	InstalledPack,
	Message,
	OllamaHealth,
	ProviderInfo,
	Settings
} from './types';

function uid(): string {
	return Math.random().toString(36).slice(2, 10);
}

const PREFIX = 'fella:conversation:'; // one key per tab: fella:conversation:<id>
const LEGACY_KEY = 'fella:conversation'; // the single pre-tabs blob
const INDEX_KEY = 'fella:tabs'; // JSON array of open tab ids

/** One conversation tab: its transcript, its in-flight run, its input history. */
export class Conversation {
	readonly id = uid();
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

class Session {
	catalog = $state<Catalog>({ workspace: null, sources: [] });
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

	/** The open conversation tabs, and the index of the focused one. */
	tabs = $state<Conversation[]>([new Conversation()]);
	active = $state<number>(0);
	/** Focus mode: hide the tab strip and the folder header for a plain,
	 *  single-conversation view. Toggled by `/focus` or Ctrl+Shift+F. */
	focus = $state<boolean>(false);

	get activeTab(): Conversation {
		return this.tabs[this.active] ?? this.tabs[0];
	}

	/** The active tab's model, or the saved default when it hasn't picked one. */
	get model(): string {
		return this.activeTab.model || this.settings?.model || '';
	}

	// --- per-conversation facade -> the active tab -------------------------
	get messages(): Message[] {
		return this.activeTab.messages;
	}
	set messages(v: Message[]) {
		this.activeTab.messages = v;
	}
	get conversationId(): string {
		return this.activeTab.id;
	}
	get busy(): boolean {
		return this.activeTab.busy;
	}
	set busy(v: boolean) {
		this.activeTab.busy = v;
	}
	get activity(): string {
		return this.activeTab.activity;
	}
	set activity(v: string) {
		this.activeTab.activity = v;
	}
	get pendingKey(): { provider: string; display: string } | null {
		return this.activeTab.pendingKey;
	}
	set pendingKey(v: { provider: string; display: string } | null) {
		this.activeTab.pendingKey = v;
	}
	get pendingConnect(): { id: string } | null {
		return this.activeTab.pendingConnect;
	}
	set pendingConnect(v: { id: string } | null) {
		this.activeTab.pendingConnect = v;
	}

	addUser(text: string): Message {
		return this.activeTab.addUser(text);
	}
	addAssistant(text = ''): Message {
		return this.activeTab.addAssistant(text);
	}
	addSystem(text: string): Message {
		return this.activeTab.addSystem(text);
	}

	// --- tab management ---------------------------------------------------
	newTab(): void {
		const inherit = this.model; // provider + login are shared; carry the model
		const c = new Conversation();
		c.model = inherit;
		this.tabs.push(c);
		this.active = this.tabs.length - 1;
		this.#writeIndex();
	}

	/** Open an archived conversation (`/history <n>`) in a new tab, its
	 *  transcript exactly as saved. If `workspace` doesn't match the folder
	 *  open right now, the caller is responsible for warning that new
	 *  questions here will run against the current folder, not the
	 *  original one there's only ever one open folder for every tab. */
	loadArchivedTab(messages: Message[]): void {
		const inherit = this.model; // same convention as newTab()
		const c = new Conversation();
		c.model = inherit;
		// A reloaded transcript never has a run in flight.
		c.messages = messages.map((m) => (m.pending ? { ...m, pending: false } : m));
		this.tabs.push(c);
		this.active = this.tabs.length - 1;
		this.#writeIndex();
	}

	async closeTab(i: number): Promise<void> {
		const tab = this.tabs[i];
		if (!tab) return;
		await this.#archive(tab);
		tab.dropSnapshot();
		if (isTauri()) void ipc.forgetConversation(tab.id).catch(() => {});
		this.tabs.splice(i, 1);
		if (this.tabs.length === 0) this.tabs.push(new Conversation());
		// Keep the focus on the same tab where possible: shift left if we closed
		// one before it, then clamp.
		if (this.active > i) this.active -= 1;
		this.active = Math.min(this.active, this.tabs.length - 1);
		this.#writeIndex();
	}

	/** End the active tab's conversation: archive it, then start it blank. */
	async clear(): Promise<void> {
		await this.#archive(this.activeTab);
		this.activeTab.dropSnapshot();
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
	}

	/** Persist every tab's transcript (each debounces its own write). */
	persist(): void {
		const ws = this.catalog.workspace ?? null;
		for (const t of this.tabs) t.persist(ws);
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
			messages: tab.messages
		});
		try {
			await ipc.archiveConversation(tab.id, body);
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
		const body = JSON.stringify({
			id: saved.id ?? '',
			saved_at_ms: Date.now(),
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

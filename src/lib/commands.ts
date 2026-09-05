// Slash-command parsing and input dispatch for the REPL.

import { ipc, isTauri, openExternal, pickFolder } from './ipc';
import { prefs } from './prefs.svelte';
import { Conversation, session } from './session.svelte';
import type { AskEvent, InstalledPack, Message, OllamaHealth, ProviderInfo } from './types';

const HELP = `Ask a question in plain language and Fella answers from your files,
showing the exact steps it took. You never need these commands, but here they are:

  /open <path>     choose the folder Fella looks at
  /files           see what Fella found in your folder
  /schema <name>   see the columns in a table
  /sql <query>     run a query yourself, without the AI
  /login           connect Fella to a model (lists the options)
  /login <name>    connect to one, then paste an API key (or /login <name> key <key>)
  /logout <name>   disconnect from a model service
  /auth            see which model services you're connected to
  /model           see or change which model answers
  /reindex         check the folder again for new or changed files
  /update          check for a newer version of Fella and install it
  /packs           themes and skills you've added (/packs browse to find more)
  /connect         connect a data source you've added
  /tab             open another conversation in a new tab
  /focus           hide the tabs and header for a plain view (again to undo)
  /clear           start this conversation over (the old one is saved)
  /history         list your saved conversations, /history <n> to reopen one
  /retry           ask the last question again
  /help            this list

keys  Enter send · Shift+Enter new line · ↑ last input · Ctrl+K commands
      Ctrl+T new tab · Ctrl+W close tab · Ctrl+1…9 switch tab
      Ctrl+L clear · Esc stop a run / hide details`;

export const SLASH_COMMANDS = [
	'/open',
	'/files',
	'/schema',
	'/sql',
	'/login',
	'/logout',
	'/auth',
	'/model',
	'/reindex',
	'/update',
	'/packs',
	'/connect',
	'/tab',
	'/focus',
	'/clear',
	'/history',
	'/retry',
	'/help'
] as const;

const MODEL_FIELDS = ['provider', 'base_url', 'model', 'embed_model'];

/** Where `/packs browse` sends you to find packs and their ids. Points at the
 *  `fella-extensions` repo until the marketplace site (`packs.fella.dev`) is
 *  deployed from `fella-web`. */
const MARKETPLACE_URL = 'https://github.com/Avijit-Kumar-GIT/fella-extensions';

/** One-line summary per command, for the composer menu and the ⌘K palette. */
export const COMMAND_DESCRIPTIONS: Record<string, string> = {
	'/open': 'choose the folder Fella looks at',
	'/files': 'see what Fella found in your folder',
	'/schema': 'see the columns in a table',
	'/sql': 'run a query yourself, without the AI',
	'/login': 'connect Fella to a model (lists the options)',
	'/logout': 'disconnect from a model service',
	'/auth': "see which model services you're connected to",
	'/model': 'see or change which model answers',
	'/reindex': 'check the folder again for new or changed files',
	'/update': 'check for a newer version of Fella and install it',
	'/packs': "themes and skills you've added",
	'/connect': "connect a data source you've added",
	'/tab': 'open another conversation in a new tab',
	'/focus': 'hide the tabs and header for a plain view',
	'/clear': 'start this conversation over (the old one is saved)',
	'/history': 'list your saved conversations, /history <n> to reopen one',
	'/retry': 'ask the last question again',
	'/help': 'show all commands'
};

/** Every valid next value for the current input matching command names before
 *  the first space, then the fixed argument set for `/login`, `/logout`,
 *  `/model` and `/schema`, each narrowed by what's already typed. Drives both
 *  the composer completion menu and Tab completion; returns `[]` when there's
 *  nothing useful to offer. */
export function completionsFor(input: string): string[] {
	if (!input.startsWith('/')) return [];
	const parts = input.split(/\s+/);
	const cmd = parts[0];
	const typed = parts[parts.length - 1].toLowerCase();
	const pick = (opts: string[]) => opts.filter((o) => o.toLowerCase().startsWith(typed));

	// Still on the command word itself.
	if (parts.length === 1) {
		const m = SLASH_COMMANDS.filter((c) => c.startsWith(cmd));
		return m.length === 1 && m[0] === cmd ? [] : [...m];
	}

	const models = () => session.health?.models ?? [];
	const tables = () => session.catalog.sources.filter((s) => s.view).map((s) => s.name);
	const providerIds = (only: (p: ProviderInfo) => boolean) =>
		session.providers.filter(only).map((p) => p.id);

	if (parts.length === 2) {
		switch (cmd) {
			case '/login':
				return pick(providerIds((p) => p.auth !== 'none'));
			case '/logout':
				// Signed-in services, plus the one you're currently on if it's a
				// hosted provider (so you can undo a half-configured switch).
				return pick(providerIds((p) => p.auth !== 'none' && (p.authed || p.current)));
			case '/model':
				// A bare word is taken as a model name, so offer both the fields
				// and the models the signed-in provider actually exposes.
				return pick([...MODEL_FIELDS, ...models()]);
			case '/schema':
				return pick(tables());
			case '/packs':
				return pick(['browse', 'install', 'add', 'enable', 'disable', 'remove']);
			case '/connect':
				return pick(session.packs.filter((p) => p.kind === 'mcp').map((p) => p.id));
		}
	}
	if (parts.length === 3 && cmd === '/packs') {
		const sub = parts[1].toLowerCase();
		if (sub === 'enable') return pick(session.packs.filter((p) => !p.enabled).map((p) => p.id));
		if (sub === 'disable') return pick(session.packs.filter((p) => p.enabled).map((p) => p.id));
		if (sub === 'remove') return pick(session.packs.map((p) => p.id));
	}
	if (parts.length === 3 && cmd === '/connect') {
		return pick(['off', 'forget']);
	}
	if (parts.length === 3 && cmd === '/login') {
		// `/login <provider> …` the only meaningful trailing word is `key`.
		const p = session.providers.find((x) => x.id === parts[1].toLowerCase());
		return p && p.auth !== 'none' ? pick(['key']) : [];
	}
	if (parts.length === 3 && cmd === '/model') {
		const field = parts[1].toLowerCase();
		if (field === 'provider') return pick(session.providers.map((p) => p.id));
		if (field === 'model' || field === 'embed_model') return pick(models());
	}
	return [];
}

/** Open a folder as the workspace. With no path, shows the native picker. */
export async function openFolder(path?: string): Promise<void> {
	if (!isTauri()) {
		session.addSystem('Fella needs the desktop app to do that.');
		return;
	}
	const chosen = path ?? (await pickFolder());
	if (!chosen) return;
	try {
		session.busy = true;
		session.activity = 'reading the folder…';
		session.catalog = await ipc.openWorkspace(chosen);
		session.addSystem(summarizeCatalog());
	} catch (e) {
		session.addSystem(`Couldn't open that folder: ${errMsg(e)}`);
	} finally {
		session.busy = false;
		session.activity = '';
	}
}

/** Ask the engine to stop one tab's in-progress run (the active tab by
 *  default). The `ask` promise then resolves normally (a "Stopped." answer) and
 *  clears that tab's `busy`. */
export async function stop(conv: Conversation = session.activeTab): Promise<void> {
	if (!conv.busy || !isTauri()) return;
	conv.activity = 'stopping…';
	try {
		await ipc.cancel(conv.id);
	} catch {
		/* the run may have already finished nothing to stop */
	}
}

/** Entry point: called with the raw composer text. */
export async function dispatch(raw: string): Promise<void> {
	const text = raw.trim();
	if (!text) return;

	// Capturing an API key for `/login`: the line is the key. Never echo it to
	// the transcript and never persist it.
	if (session.pendingKey) {
		const pending = session.pendingKey;
		session.pendingKey = null;
		if (text.startsWith('/')) {
			// user changed their mind fall through and run the command
		} else {
			try {
				session.settings = await ipc.setApiKey(pending.provider, text);
				await announceSignedIn(pending.display);
			} catch (e) {
				session.addSystem(`Couldn't save that key: ${errMsg(e)}`);
			}
			return;
		}
	}

	// Capturing an MCP connector token for `/connect <id>`.
	if (session.pendingConnect) {
		const { id } = session.pendingConnect;
		session.pendingConnect = null;
		if (!text.startsWith('/')) {
			try {
				await ipc.mcpSetToken(id, text);
				session.packs = await ipc.packsSetEnabled(id, true);
				session.addSystem(`Connected ${id}.`);
			} catch (e) {
				session.addSystem(`Couldn't save that key: ${errMsg(e)}`);
			}
			return;
		}
	}

	if (text.startsWith('/')) {
		await runCommand(text);
		return;
	}

	const conv = session.activeTab;
	conv.addUser(text);
	await ask(text, conv);
}

/** Fetch the provider list and cache it on the session so the composer hint
 *  stays current. */
async function loadProviders(): Promise<ProviderInfo[]> {
	const list = await ipc.listProviders();
	session.providers = list;
	return list;
}

/** Nudge the health indicator to re-probe after an auth change. Returns the
 *  probe result so the caller can react to a key the provider won't take. */
async function refreshHealthSoon(): Promise<OllamaHealth | null> {
	try {
		session.health = await ipc.ollamaHealth();
		await reconcileModel();
		return session.health;
	} catch {
		/* ignore the status bar will re-probe on its own timer */
		return null;
	}
}

/** When connected to Ollama, make sure the configured model is one that's
 *  actually pulled Fella's default `llama3.1` often isn't (people have
 *  `llama3.1:8b`). Silently switches to an available chat model; the status
 *  bar shows the result. Ollama only gateways expose hundreds of models and
 *  must be chosen deliberately. Never posts to the transcript, so it doesn't
 *  push the welcome screen away before the user has asked anything. */
export async function reconcileModel(): Promise<void> {
	const s = session.settings;
	const h = session.health;
	if (!s || s.provider !== 'ollama' || !h?.reachable) return;

	const models = h.models ?? [];
	if (!models.length) return;
	const chat = models.filter((m) => !/embed/i.test(m));
	if (!chat.length) return; // only embedding models pulled UI nudges a pull

	// Keep the saved default valid (what a fresh tab inherits).
	if (!s.model || !models.includes(s.model)) {
		try {
			session.settings = await ipc.setSettings({ model: chat[0] });
		} catch {
			/* leave it the empty-screen prompt still guides a manual pick */
		}
	}
	// And each tab that has explicitly picked a now-unavailable model. A tab
	// with no pick uses the (just-reconciled) default, so leave those alone.
	for (const t of session.tabs) {
		if (t.model && !models.includes(t.model)) t.model = chat[0];
	}
}

/** After a key is saved, say so if the provider wouldn't take it. The key stays
 *  saved either way a probe can fail for offline or transient reasons, and we
 *  don't want to block someone who knows their key is fine. */
function warnIfKeyUnverified(display: string, health: OllamaHealth | null): void {
	if (!health || health.reachable) return;
	session.addSystem(
		health.rejected
			? `That key didn't work with ${display}. Check it and run /login again.`
			: `Saved your key. Fella couldn't reach ${display} to test it just now; it should work once ${display} is reachable.`
	);
}

/** Confirm a completed sign-in and re-probe health. Call after `session.settings` is set. */
async function announceSignedIn(display: string): Promise<void> {
	const m = session.settings?.model;
	session.addSystem(
		`Connected to ${display}.` +
			(m
				? ` It'll answer with ${m}; change that with /model.`
				: ' Pick a model with /model.')
	);
	warnIfKeyUnverified(display, await refreshHealthSoon());
}

/** `/login <p> key <KEY>` and `/model key <KEY>` carry a secret on the line.
 *  True when `text` is one of those with a value after the keyword. */
export function carriesSecret(text: string): boolean {
	const t = text.trim();
	if (/^\/(login\s+\S+\s+key|model\s+key)\s+\S/i.test(t)) return true;
	// `/connect <id> <token>` but not `/connect <id> off|forget`
	return /^\/connect\s+\S+\s+(?!off\s*$|forget\s*$)\S/i.test(t);
}

/** The same line with the secret blanked, for the transcript. */
function redactSecret(text: string): string {
	return text
		.replace(/^(\/login\s+\S+\s+key)\s+.+/i, '$1 ••••••')
		.replace(/^(\/model\s+key)\s+.+/i, '$1 ••••••')
		.replace(/^(\/connect\s+\S+)\s+(?!off$|forget$).+/i, '$1 ••••••');
}

async function runCommand(text: string): Promise<void> {
	const [cmd, ...rest] = text.split(/\s+/);
	const arg = rest.join(' ').trim();
	session.addUser(redactSecret(text));

	switch (cmd) {
		case '/help':
			session.addSystem(HELP);
			return;

		case '/clear':
			await session.clear();
			return;

		case '/tab':
			session.newTab();
			return;

		case '/focus':
			session.focus = !session.focus;
			session.addSystem(
				session.focus
					? 'Focus mode on. The tabs and header are hidden. /focus again to bring them back.'
					: 'Focus mode off.'
			);
			return;

		case '/retry': {
			const q = lastQuestion();
			if (!q) {
				session.addSystem('Nothing to retry yet. Ask a question first.');
				return;
			}
			await ask(q, session.activeTab);
			return;
		}

		case '/history': {
			if (!isTauri()) {
				session.addSystem('Saved conversations need the desktop app.');
				return;
			}
			try {
				const list = await ipc.conversationsList();
				if (list.length === 0) {
					session.addSystem("No past conversations yet — they're saved here once you /clear or close a tab.");
					return;
				}
				const n = arg ? Number.parseInt(arg, 10) : NaN;
				if (Number.isInteger(n)) {
					const chosen = list[n - 1];
					if (!chosen) {
						session.addSystem(`No conversation #${n}. Type /history to see the list again.`);
						return;
					}
					const raw = await ipc.conversationLoad(chosen.id);
					const saved: { workspace?: string | null; messages?: unknown } = JSON.parse(raw);
					const messages = Array.isArray(saved.messages) ? (saved.messages as Message[]) : [];
					session.loadArchivedTab(messages);
					session.addSystem(`Reopened: "${chosen.preview}" (${dateLabel(chosen.saved_at_ms)}).`);
					const current = session.catalog.workspace;
					if (saved.workspace && current && saved.workspace !== current) {
						session.addSystem(
							`This conversation was about a different folder (${baseName(saved.workspace)}). ` +
								`Fella is pointed at ${baseName(current)} right now, so a new question here answers ` +
								`from that folder, not the original one. /open ${saved.workspace} first if you want the original.`
						);
					}
					return;
				}
				const lines = list.map((c, i) => {
					const where = c.workspace ? ` · ${baseName(c.workspace)}` : '';
					return `  ${i + 1}. "${c.preview}" · ${c.message_count} message${c.message_count === 1 ? '' : 's'} · ${dateLabel(c.saved_at_ms)}${where}`;
				});
				session.addSystem(
					`Your past conversations, newest first — /history <n> to reopen one:\n${lines.join('\n')}`
				);
			} catch (e) {
				session.addSystem(`error: ${errMsg(e)}`);
			}
			return;
		}

		case '/open':
			if (!requireEngine()) return;
			await openFolder(arg || undefined);
			return;

		case '/files':
			if (!requireEngine()) return;
			try {
				session.catalog = await ipc.getCatalog();
				session.addSystem(
					session.catalog.workspace
						? summarizeCatalog()
						: 'No folder open yet. Choose one with /open, or the button on the welcome screen.'
				);
			} catch (e) {
				session.addSystem(`error: ${errMsg(e)}`);
			}
			return;

		case '/schema':
			if (!requireEngine()) return;
			if (!arg) {
				const tables = session.catalog.sources.filter((s) => s.view).map((s) => s.name);
				session.addSystem(
					tables.length
						? `Which table? Try one of: ${tables.join(', ')}`
						: "Open a folder first, then /schema shows a table's columns."
				);
				return;
			}
			try {
				const s = await ipc.describe(arg);
				const lines = (s.columns ?? []).map(
					(c) =>
						`  ${c.name.padEnd(24)} ${c.type.padEnd(12)} ` +
						`${c.null_fraction != null ? `${Math.round(c.null_fraction * 100)}% null` : ''}`
				);
				session.addSystem(
					`${s.name}  (${s.row_count ?? '?'} rows)\n${lines.join('\n')}`
				);
			} catch (e) {
				const tables = session.catalog.sources.filter((s) => s.view).map((s) => s.name);
				const known = tables.some((t) => t.toLowerCase() === arg.toLowerCase());
				session.addSystem(
					!known && tables.length
						? `No table called "${arg}". Try one of: ${tables.join(', ')}`
						: `error: ${errMsg(e)}`
				);
			}
			return;

		case '/sql':
			if (!requireEngine()) return;
			if (!arg) {
				session.addSystem('Type a query after /sql, e.g. /sql select * from transactions limit 5');
				return;
			}
			{
				const conv = session.activeTab;
				try {
					conv.busy = true;
					conv.activity = 'running your query…';
					const r = await ipc.runSqlDirect(arg);
					conv.addSystem(renderTable(r.columns, r.rows, r.row_count, r.ms, r.truncated));
				} catch (e) {
					conv.addSystem(`error: ${errMsg(e)}`);
				} finally {
					conv.busy = false;
					conv.activity = '';
				}
			}
			return;

		case '/login': {
			if (!requireEngine()) return;
			let list: ProviderInfo[];
			try {
				list = await loadProviders();
			} catch (e) {
				session.addSystem(`error: ${errMsg(e)}`);
				return;
			}
			const words = arg.split(/\s+/).filter(Boolean);
			const name = words[0]?.toLowerCase();
			if (!name) {
				session.addSystem(renderProviders(list));
				return;
			}
			const p = list.find((x) => x.id === name);
			if (!p) {
				session.addSystem(`unknown provider: ${name}\n\n${renderProviders(list)}`);
				return;
			}
			if (p.auth === 'none') {
				session.addSystem(
					`${p.display} runs on your computer and needs no sign-in. Start it, then pick it with /model.`
				);
				return;
			}

			const saidKey = words[1]?.toLowerCase() === 'key';
			const inlineKey = saidKey ? words.slice(2).join(' ').trim() : '';
			if (inlineKey) {
				try {
					session.settings = await ipc.setApiKey(p.id, inlineKey);
					await announceSignedIn(p.display);
				} catch (e) {
					session.addSystem(`Couldn't save that key: ${errMsg(e)}`);
				}
				return;
			}
			session.pendingKey = { provider: p.id, display: p.display };
			session.addSystem(
				`Paste your ${p.display} API key and press Enter.` +
					(p.get_key_url ? `\nGet one at ${p.get_key_url}` : '') +
					`\nThe key is not shown or written to the transcript. Esc to cancel.`
			);
			return;
		}

		case '/logout': {
			if (!requireEngine()) return;
			let list: ProviderInfo[];
			try {
				list = await loadProviders();
			} catch (e) {
				session.addSystem(`error: ${errMsg(e)}`);
				return;
			}

			const named = arg.split(/\s+/)[0]?.toLowerCase();
			const signedIn = list.filter((p) => p.authed && p.auth !== 'none');
			const active = list.find((p) => p.current);

			// Work out which provider to disconnect.
			let target: ProviderInfo | undefined;
			if (named) {
				target = list.find((x) => x.id === named);
				if (!target) {
					// Not a registered provider but if it's the one settings point
					// at (a stray id from an older build), let the engine clear it.
					if (named === session.settings?.provider) {
						try {
							session.settings = await ipc.logout(named);
							session.providers = await ipc.listProviders();
							session.addSystem(`Disconnected from ${named}. Fella is back on the local default.`);
							await refreshHealthSoon();
						} catch (e) {
							session.addSystem(`error: ${errMsg(e)}`);
						}
						return;
					}
					session.addSystem(`unknown provider: ${named}\n\n${renderProviders(list)}`);
					return;
				}
				if (target.auth === 'none') {
					session.addSystem(`${target.display} runs on your computer, so there's no sign-in to undo.`);
					return;
				}
			} else if (active && active.auth !== 'none') {
				target = active; // bare /logout disconnects the service you're on
			} else if (signedIn.length === 1) {
				target = signedIn[0];
			} else if (signedIn.length > 1) {
				session.addSystem(
					`You're signed in to ${signedIn.map((p) => p.display).join(', ')}.\n` +
						`Which one? Use /logout <name>.`
				);
				return;
			} else {
				const ollama = list.find((x) => x.id === 'ollama')?.display ?? 'Ollama';
				session.addSystem(
					`You're not connected to any model service. Fella is on ${ollama}, which needs no sign-in.`
				);
				return;
			}

			try {
				const hadKey = target.authed;
				session.settings = await ipc.logout(target.id);
				session.providers = await ipc.listProviders();
				const resetToOllama = session.settings?.provider === 'ollama' && target.id !== 'ollama';
				const ollama = session.providers.find((x) => x.id === 'ollama')?.display ?? 'Ollama';
				session.addSystem(
					(hadKey
						? `Disconnected from ${target.display}.`
						: `${target.display} had no saved key.`) +
						(resetToOllama
							? ` Fella is back on ${ollama}; start it, or /login to another service.`
							: '')
				);
				await refreshHealthSoon();
			} catch (e) {
				session.addSystem(`error: ${errMsg(e)}`);
			}
			return;
		}

		case '/auth':
			if (!requireEngine()) return;
			try {
				session.addSystem(renderProviders(await loadProviders()));
			} catch (e) {
				session.addSystem(`error: ${errMsg(e)}`);
			}
			return;

		case '/model':
			if (!requireEngine()) return;
			try {
				if (arg) {
					const patch = parseModelArg(arg);
					if (!patch) {
						session.addSystem(
							'To switch this tab’s model, type /model followed by a name from the list ' +
								'(names have no spaces). Each tab can use a different model; they all share ' +
								'one provider.\n' +
								'To change a setting: /model <field> <value> where field is provider, base_url, or embed_model.\n' +
								'To connect to a new model service, use /login.'
						);
						return;
					}
					const { api_key, model: newModel, ...rest } = patch;
					if (api_key !== undefined) {
						const prov = (session.settings ?? (await ipc.getSettings())).provider;
						session.settings = await ipc.setApiKey(prov, api_key);
						const label = session.providers.find((p) => p.id === prov)?.display ?? prov;
						warnIfKeyUnverified(label, await refreshHealthSoon());
					}
					if (Object.keys(rest).length > 0) {
						session.settings = await ipc.setSettings(rest);
						// A provider / endpoint change invalidates every tab's model.
						if (rest.provider || rest.base_url) for (const t of session.tabs) t.model = '';
					}
					if (newModel !== undefined) {
						// Per-tab: only the focused conversation switches. Also remember
						// it as the default a fresh tab / next launch starts from.
						session.activeTab.model = newModel;
						session.settings = await ipc.setSettings({ model: newModel });
					}
				}
				const s = session.settings ?? (await ipc.getSettings());
				session.settings = s;
				// Re-probe so the model list reflects the provider you're signed in to.
				await refreshHealthSoon();
				const prov = session.providers.find((p) => p.id === s.provider);
				const tabModel = session.model;
				const perTab =
					session.tabs.length > 1
						? `\n\n${session.tabs.length} tabs open each can /model its own; the provider is shared.`
						: '';
				session.addSystem(
					`model service:   ${prov?.display ?? s.provider}\n` +
						`address:         ${s.base_url}\n` +
						`model:           ${tabModel}   (this tab)\n` +
						`connected:       ${s.has_credential ? 'yes' : 'no'}` +
						renderModelChoices(session.health?.models ?? [], tabModel) +
						perTab +
						(s.provider === 'ollama'
							? '\n\nOnly downloaded models show above. Browse more at ollama.com/library, ' +
								'then run  ollama pull <name>  (or add Ollama Cloud with /login ollama-cloud).'
							: '')
				);
			} catch (e) {
				session.addSystem(`error: ${errMsg(e)}`);
			}
			return;

		case '/reindex':
			if (!requireEngine()) return;
			{
				const conv = session.activeTab;
				try {
					conv.busy = true;
					conv.activity = 'checking the folder…';
					session.catalog = await ipc.reindex();
					conv.addSystem(`Checked the folder again.\n${summarizeCatalog()}`);
				} catch (e) {
					conv.addSystem(`error: ${errMsg(e)}`);
				} finally {
					conv.busy = false;
					conv.activity = '';
				}
			}
			return;

		case '/update':
			if (!requireEngine()) return;
			{
				const conv = session.activeTab;
				try {
					conv.busy = true;
					conv.activity = 'checking for an update…';
					const status = await ipc.update();
					// A found update is applied immediately (no separate confirm
					// step) the app exits as part of that, so this message may
					// never actually be seen before the window closes.
					conv.addSystem(
						status.available
							? `Updating to ${status.latest}… Fella will close; reopen it once the installer finishes.`
							: `You're up to date (${status.current}).`
					);
				} catch (e) {
					conv.addSystem(`error: ${errMsg(e)}`);
				} finally {
					conv.busy = false;
					conv.activity = '';
				}
			}
			return;

		case '/packs': {
			if (!requireEngine()) return;
			const words = arg.split(/\s+/).filter(Boolean);
			const sub = words[0]?.toLowerCase();
			try {
				if (!sub) {
					session.packs = await ipc.packsList();
					session.addSystem(renderPacks(session.packs));
					return;
				}
				if (sub === 'browse') {
					session.addSystem(
						"The browse site isn't live yet — opening the packs repo, " +
							`where the packs and their ids are listed:\n${MARKETPLACE_URL}`
					);
					void openExternal(MARKETPLACE_URL);
					return;
				}
				if (sub === 'add') {
					const path = words.slice(1).join(' ').trim();
					if (!path) {
						session.addSystem(
							'Point /packs add at a folder on this computer that holds a pack, ' +
								'for example: /packs add ~/Downloads/ocean-theme'
						);
						return;
					}
					session.packs = await ipc.packsAdd(path);
					await prefs.load();
					session.addSystem(
						`Added from a local folder, so it's marked unverified (nobody reviewed it but you).\n${renderPacks(session.packs)}`
					);
					return;
				}
				if (sub === 'install') {
					const id = words[1];
					if (!id) {
						session.addSystem(
							'Which pack? Run /packs browse to see what’s available, ' +
								'then /packs install <id> with an id from that list.'
						);
						return;
					}
					session.addSystem(`Installing ${id}…`);
					try {
						session.packs = await ipc.packsInstall(id);
					} catch (e) {
						// The hosted catalog isn't fully live yet — a missing or
						// unreachable catalog shouldn't read as a raw HTTP error.
						// Real failures (checksum mismatch, unknown id, disk) fall
						// through to the outer catch unchanged.
						if (/could not reach|catalog is not valid|: HTTP [45]\d\d/i.test(errMsg(e))) {
							session.addSystem(
								"Couldn't reach the pack catalog (the marketplace isn't fully live yet). " +
									'You can still add a pack from a local folder: /packs add <path>.'
							);
							return;
						}
						throw e;
					}
					await prefs.load();
					session.addSystem(`Installed ${id}. Enable it with /packs enable ${id}.\n${renderPacks(session.packs)}`);
					return;
				}
				if (sub === 'enable' || sub === 'disable' || sub === 'remove') {
					const id = words[1];
					if (!id) {
						const pool =
							sub === 'enable'
								? session.packs.filter((p) => !p.enabled)
								: sub === 'disable'
									? session.packs.filter((p) => p.enabled)
									: session.packs;
						session.addSystem(
							pool.length
								? `Which one? /packs ${sub} <id>, where <id> is one of:\n  ${pool
										.map((p) => p.id)
										.join('\n  ')}`
								: `Nothing to ${sub}. Run /packs to see what you have.`
						);
						return;
					}
					session.packs =
						sub === 'remove'
							? await ipc.packsRemove(id)
							: await ipc.packsSetEnabled(id, sub === 'enable');
					await prefs.load();
					session.addSystem(renderPacks(session.packs));
					return;
				}
				session.addSystem(
					`unknown: /packs ${sub}\n\n` +
						'/packs · /packs browse · /packs install <id> · /packs add <path> · /packs enable <id> · /packs disable <id> · /packs remove <id>'
				);
			} catch (e) {
				session.addSystem(`error: ${errMsg(e)}`);
			}
			return;
		}

		case '/connect': {
			if (!requireEngine()) return;
			const words = arg.split(/\s+/).filter(Boolean);
			const id = words[0];
			const sub = words[1]?.toLowerCase();
			try {
				const connectors = session.packs.filter((p) => p.kind === 'mcp');
				if (!id) {
					session.addSystem(renderConnectors(connectors));
					return;
				}
				const c = connectors.find((p) => p.id === id);
				if (!c) {
					session.addSystem(`No data connection called "${id}".\n\n${renderConnectors(connectors)}`);
					return;
				}
				if (sub === 'off') {
					session.packs = await ipc.packsSetEnabled(id, false);
					session.addSystem(`Disconnected ${id}.`);
					return;
				}
				if (sub === 'forget') {
					await ipc.mcpClearToken(id);
					session.packs = await ipc.packsSetEnabled(id, false);
					session.addSystem(`Forgot the ${id} key and disconnected it.`);
					return;
				}
				if (words.length >= 2) {
					await ipc.mcpSetToken(id, words.slice(1).join(' '));
					session.packs = await ipc.packsSetEnabled(id, true);
					session.addSystem(`Connected ${id}.`);
					return;
				}
				if (!c.needs_token) {
					session.packs = await ipc.packsSetEnabled(id, true);
					session.addSystem(c.enabled ? `${id} is already connected.` : `Connected ${id}.`);
					return;
				}
				session.pendingConnect = { id };
				session.addSystem(
					`Paste the ${id} key and press Enter. It's stored on this computer, ` +
						'never shown or logged. Esc to cancel.'
				);
			} catch (e) {
				session.addSystem(`error: ${errMsg(e)}`);
			}
			return;
		}

		default:
			session.addSystem(`unknown command: ${cmd}\n\n${HELP}`);
	}
}

/** Run one question in `conv` (its own tab). Bound to the tab, not "the active
 *  tab", so it keeps streaming there after the user switches away. */
async function ask(question: string, conv: Conversation): Promise<void> {
	if (!requireEngine()) return;

	const msg = conv.addAssistant('');
	conv.busy = true;
	conv.activity = 'thinking…';

	// A local model's first reply can take 10-20s (cold model load). Show a
	// running count so the wait doesn't read as a hang only ever rewrites
	// our own "thinking…" text, never a tool note or a retry notice.
	const t0 = Date.now();
	const tick = setInterval(() => {
		if (conv.activity === 'thinking…' || /^thinking… \d+s$/.test(conv.activity)) {
			const s = Math.round((Date.now() - t0) / 1000);
			if (s >= 4) conv.activity = `thinking… ${s}s`;
		}
	}, 1000);

	// Transient engine notices (retry/backoff, connector problems) normally only
	// flash in the status bar. Keep them so that if the run ends badly the user
	// has the warning that explains why.
	const notices: string[] = [];

	const onEvent = (e: AskEvent) => {
		switch (e.kind) {
			case 'assistant_delta':
				msg.text += e.text;
				break;
			case 'tool_start': {
				// The sentence the model streamed before calling a tool is its
				// plan keep it visible (dimmed) instead of the raw answer body,
				// so the wait shows what it's doing. Set once, from the first
				// tool_start; later rounds just clear any stray preamble.
				if (!msg.plan && msg.text.trim()) msg.plan = msg.text.trim();
				msg.text = '';
				const note = typeof e.args?.note === 'string' ? e.args.note.trim() : '';
				conv.activity = note ? `${note}…` : 'working…';
				break;
			}
			case 'tool_end':
				// Back to the model for the next step keep a heartbeat showing.
				conv.activity = 'thinking…';
				break;
			case 'notice':
				// Transient status from the engine, e.g. "rate limited retrying in 3s…".
				conv.activity = e.text;
				if (notices[notices.length - 1] !== e.text) notices.push(e.text);
				break;
			case 'answer_done':
				msg.answer = e.answer;
				msg.text = e.answer.text;
				msg.plan = undefined;
				break;
		}
	};

	try {
		const answer = await ipc.ask(conv.id, question, onEvent, conv.model || undefined);
		msg.answer = answer;
		msg.text = answer.text;
	} catch (e) {
		const kind = errKind(e);
		// If the model already streamed part of an answer, keep it rather than
		// replacing what the user watched appear with a bare "error:".
		const streamed = msg.text.trim();
		if (streamed && !msg.answer) {
			msg.text = `${streamed}\n\n(the connection dropped, so this answer may be incomplete)`;
		} else {
			msg.text = `error: ${errMsg(e)}`;
		}
		if (kind === 'transient') {
			msg.text += `\n\nType /retry to try again, or /model to switch model.`;
		}
		if (notices.length) {
			msg.text += `\n\nwhile working:\n` + notices.map((n) => `  ${n}`).join('\n');
		}
	} finally {
		clearInterval(tick);
		msg.pending = false;
		msg.plan = undefined;
		conv.busy = false;
		conv.activity = '';
	}
}

// --- helpers ---------------------------------------------------------------

type SettingsPatch = Partial<import('./types').Settings> & { api_key?: string };

function parseModelArg(arg: string): SettingsPatch | null {
	const parts = arg.split(/\s+/);
	const fields = ['provider', 'base_url', 'model', 'embed_model', 'key'];
	if (fields.includes(parts[0])) {
		if (parts.length < 2) return null; // a field name with no value
		const v = parts.slice(1).join(' ');
		return (parts[0] === 'key' ? { api_key: v } : { [parts[0]]: v }) as SettingsPatch;
	}
	// Anything else is a model id. Send it as-is the provider rejects a bad
	// one with a real error, which beats silently keeping the old model.
	return { model: arg };
}

function renderPacks(list: InstalledPack[]): string {
	if (list.length === 0) {
		return 'No packs installed.\n\n/packs browse  to find some · /packs install <id>  ·  /packs add <path>';
	}
	const rows = list.map((p) => {
		const mark = p.enabled ? '●' : '○';
		const unver = p.verified ? '' : '  (unverified)';
		return `${mark} ${p.id.padEnd(20)} ${p.kind.padEnd(6)} ${(p.enabled ? 'on' : 'off').padEnd(3)}  ${p.name}${unver}`;
	});
	const out = ['  id                   kind   state', ...rows, ''];
	if (list.some((p) => !p.verified)) {
		out.push('(unverified) = added from a local folder, not the reviewed marketplace');
	}
	out.push('/packs enable <id> · /packs disable <id> · /packs remove <id> · /packs browse');
	return out.join('\n');
}

function renderConnectors(list: InstalledPack[]): string {
	if (list.length === 0) {
		return 'No data connections yet.\n\nAdd one with /packs browse, then /connect it.';
	}
	const rows = list.map((c) => {
		const status = c.needs_token ? 'needs a key' : c.enabled ? 'connected' : 'off';
		const mark = c.enabled && !c.needs_token ? '●' : '○';
		return `${mark} ${c.id.padEnd(20)} ${status}`;
	});
	return [
		'  data connection',
		...rows,
		'',
		'/connect <id> to connect (paste a key) · /connect <id> off · /connect <id> forget'
	].join('\n');
}

function renderProviders(list: ProviderInfo[]): string {
	const rows = list.map((p) => {
		const bullet = p.current ? '●' : ' ';
		const status =
			p.auth === 'none' ? 'on your computer' : p.authed ? 'connected' : 'not connected';
		return `${bullet} ${p.id.padEnd(11)} ${p.display.padEnd(26)} ${status}${p.current ? '   (current)' : ''}`;
	});
	return [
		'  name        service                    status',
		...rows,
		'',
		'/login <name> to connect · /logout <name> to disconnect'
	].join('\n');
}

/** The trailing "available models" block for `/model` with no argument. Lists
 *  them all (sorted, current marked); a very long gateway catalogue is capped
 *  with a "+N more" line since the composer filters as you type. Empty when
 *  the probe returned nothing. */
function renderModelChoices(models: string[], current: string): string {
	if (models.length === 0) return '';
	const CAP = 60;
	const sorted = [...models].sort((a, b) => a.localeCompare(b));
	const shown = sorted.slice(0, CAP);
	const rows = shown.map((m) => `  ${m === current ? '●' : ' '} ${m}`);
	const extra = sorted.length - shown.length;
	return [
		'',
		'',
		'models you can pick:',
		...rows,
		...(extra > 0 ? [`  … +${extra} more start typing after /model to filter`] : []),
		'',
		'/model <name> to switch'
	].join('\n');
}

function requireEngine(): boolean {
	if (isTauri()) return true;
	session.addSystem('Fella needs the desktop app to do that.');
	return false;
}

function skippedLines(): string[] {
	const skipped = session.catalog.skipped ?? [];
	if (skipped.length === 0) return [];
	const out = [
		'',
		`${skipped.length} file${skipped.length === 1 ? '' : 's'} couldn't be used:`
	];
	for (const f of skipped) out.push(`  ${f.name}  ·  ${f.reason}`);
	return out;
}

function summarizeCatalog(): string {
	const s = session.catalog.sources;
	const folder = session.catalog.workspace?.replace(/^.*[/\\]/, '') ?? 'folder';
	if (s.length === 0)
		return `${folder}: nothing Fella can read here yet. It works with spreadsheets, CSVs, PDFs, and text files.`;
	const tables = s.filter((f) => f.view);
	const docs = s.filter((f) => !f.view);
	const lines = [`${folder}: ${s.length} file${s.length === 1 ? '' : 's'}`];
	for (const f of tables) {
		lines.push(`  ${f.name}  ·  ${f.row_count ?? '?'} rows${f.note ? `  (${f.note})` : ''}`);
	}
	for (const f of docs) {
		lines.push(`  ${f.name}  ·  ${f.kind === 'pdf' ? 'PDF' : 'text'}`);
	}
	lines.push(...skippedLines());
	lines.push('', 'Ask a question, or /help.');
	return lines.join('\n');
}

function renderTable(
	columns: string[],
	rows: unknown[][],
	rowCount: number,
	ms: number,
	truncated: boolean
): string {
	if (columns.length === 0) return `(0 rows, ${ms}ms)`;
	const widths = columns.map((c, i) =>
		Math.max(c.length, ...rows.map((r) => String(r[i] ?? '').length))
	);
	const fmt = (cells: unknown[]) =>
		cells.map((v, i) => String(v ?? '').padEnd(widths[i])).join('  ');
	const head = fmt(columns);
	const sep = widths.map((w) => '-'.repeat(w)).join('  ');
	const body = rows.map(fmt).join('\n');
	const foot = `${rowCount} row${rowCount === 1 ? '' : 's'}${truncated ? ' (truncated)' : ''}, ${ms}ms`;
	return `${head}\n${sep}\n${body}\n\n${foot}`;
}

/** Last path segment, either separator — workspace paths come from the Rust
 *  backend in the OS's own form, so a saved conversation from Windows can
 *  still show up readably here. */
function baseName(path: string): string {
	const parts = path.split(/[/\\]+/).filter(Boolean);
	return parts[parts.length - 1] ?? path;
}

/** A short, local-time label for `/history`'s list — just a date once it's
 *  not today, so the list stays scannable. */
function dateLabel(ms: number): string {
	const d = new Date(ms);
	const now = new Date();
	const sameDay =
		d.getFullYear() === now.getFullYear() &&
		d.getMonth() === now.getMonth() &&
		d.getDate() === now.getDate();
	return sameDay
		? d.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' })
		: d.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
}

/** The engine serialises errors as `{ kind, message }`; older / Tauri-internal
 *  errors are plain strings or `Error`s. Unwrap either. */
function errMsg(e: unknown): string {
	if (e && typeof e === 'object' && typeof (e as Record<string, unknown>).message === 'string') {
		return (e as { message: string }).message;
	}
	return e instanceof Error ? e.message : String(e);
}

/** Coarse category from the engine, for choosing the next step to offer. */
function errKind(e: unknown): string {
	if (e && typeof e === 'object' && typeof (e as Record<string, unknown>).kind === 'string') {
		return (e as { kind: string }).kind;
	}
	return 'internal';
}

/** Re-run the most recent question. Powers `/retry` and the hint after a
 *  transient failure. Skips slash-command lines (including the `/retry` that
 *  triggered this), so it finds the last real question. */
function lastQuestion(): string | null {
	for (let i = session.messages.length - 1; i >= 0; i--) {
		const m = session.messages[i];
		if (m.role === 'user' && !m.text.trimStart().startsWith('/')) return m.text;
	}
	return null;
}

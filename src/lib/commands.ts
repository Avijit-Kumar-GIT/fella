// Slash-command parsing and input dispatch for the REPL.

import { ipc, isTauri, pickFolder } from './ipc';
import { Conversation, session } from './session.svelte';
import type {
	AskEvent,
	ContextReference,
	Message,
	ProviderHealth,
	ProviderInfo
} from './types';

const HELP = `Ask a question in plain language and Fella answers from your files,
showing the exact steps it took. You never need these commands, but here they are:

  Ask / Inspect     choose the normal answer flow or a stricter source-first,
                   read-only inspection flow from the composer
  + Context / @     attach a source or field to your next question

  /open <path>     choose the folder Fella looks at
  /files           see what Fella found in your folder
  /schema <name>   see the columns in a table
  /sql <query>     run a query yourself, without the AI
  /login           connect Fella to a model (lists the options)
  /login <name>    switch to one; reuses a saved key, or asks for one the first
                   time (/login <name> key [<key>] to replace a saved key)
  /logout <name>   stop using a service; the key stays saved (add "forget" to delete it)
  /auth            see which model services you're connected to
  /model           see or change which model answers
  /reindex         check the folder again for new or changed files
  /memory          see what Fella has learned about this folder (/memory forget to clear)
  /context         open fella.md, where you tell Fella about your files
  /update          check for a newer version of Fella and install it
  /mcp             experimental and inert; no connectors are enabled
  /tab             open another conversation in a new tab
  /focus           hide the tabs and header for a plain view (again to undo)
  /clear           start this conversation over (the old one is saved)
  /history         list your saved conversations, /history <n> to reopen one
  /retry           ask the last question again
  /help            this list

keys  Enter send · Shift+Enter new line · Ctrl/Cmd+K or Ctrl/Cmd+Shift+P commands
      Ctrl/Cmd+N new conversation · Ctrl/Cmd+T new tab · Ctrl/Cmd+W close tab
      Ctrl/Cmd+[ / ] previous or next tab · Ctrl/Cmd+1…9 switch tab
      Ctrl/Cmd+Shift+A Ask · Ctrl/Cmd+Shift+S Sources · Ctrl/Cmd+Shift+C Context
      Ctrl/Cmd+, settings · Ctrl/Cmd+O open folder · Ctrl/Cmd+B sidebar · Ctrl/Cmd+L clear
      Ctrl/Cmd+Shift+F focus mode · Esc stop a run / hide details`;

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
	'/memory',
	'/context',
	'/update',
	'/mcp',
	'/tab',
	'/focus',
	'/clear',
	'/history',
	'/retry',
	'/help'
] as const;

const MODEL_FIELDS = ['provider', 'base_url', 'model', 'embed_model'];

/** One-line summary per command, for the composer menu and the ⌘K palette. */
export const COMMAND_DESCRIPTIONS: Record<string, string> = {
	'/open': 'choose the folder Fella looks at',
	'/files': 'see what Fella found in your folder',
	'/schema': 'see the columns in a table',
	'/sql': 'run a query yourself, without the AI',
	'/login': 'connect Fella to a model (lists the options)',
	'/logout': 'stop using a service (keeps the key; "forget" deletes it)',
	'/auth': "see which model services you're connected to",
	'/model': 'see or change which model answers',
	'/reindex': 'check the folder again for new or changed files',
	'/memory': 'see what Fella has learned about this folder',
	'/context': 'open fella.md, where you tell Fella about your files',
	'/update': 'check for a newer version of Fella and install it',
	'/mcp': 'experimental and inert; no connectors are enabled',
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
		const all = [...SLASH_COMMANDS];
		const m = all.filter((c) => c.startsWith(cmd));
		return m.length === 1 && m[0] === cmd ? [] : [...new Set(m)];
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
			case '/memory':
				return pick(['forget']);
		}
	}
	if (parts.length === 3 && cmd === '/login') {
		// `/login <provider> …` the only meaningful trailing word is `key`.
		const p = session.providers.find((x) => x.id === parts[1].toLowerCase());
		return p && p.auth !== 'none' ? pick(['key']) : [];
	}
	if (parts.length === 3 && cmd === '/logout') {
		// `/logout <provider> forget` also deletes the saved key.
		const p = session.providers.find((x) => x.id === parts[1].toLowerCase());
		return p && p.auth !== 'none' ? pick(['forget']) : [];
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
		session.setWorkspaceView('ask');
		session.addSystem('Fella needs the desktop app to do that.');
		return;
	}

	let chosen = path;
	if (chosen == null) {
		// The native picker inherits the OS cursor state from the instant it
		// opens. Reached by a mouse click (the welcome-screen button) that's
		// fine. Reached by `Enter` in the composer, Windows has just hidden the
		// pointer ("hide pointer while typing") and the modal dialog seizes the
		// message loop before a mouse-move restores it, so the cursor stays
		// invisible for the life of the picker. Drop the text caret and let the
		// webview settle its own cursor first, then have the OS re-show the
		// pointer (no-op off Windows) matching what the click path gets for free.
		(document.activeElement as HTMLElement | null)?.blur();
		await new Promise((r) => requestAnimationFrame(r));
		await ipc.unhideCursor().catch(() => {}); // best-effort; never block the picker
		chosen = (await pickFolder()) ?? undefined;
	}
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

/** On launch, load the (empty) catalog and note the last session's folder so
 *  the welcome screen can offer a one-click reopen. Fella no longer opens that
 *  folder automatically the user picks. Called once from the page's onMount. */
export async function loadStartupCatalog(): Promise<void> {
	if (!isTauri() || session.catalog.workspace) return;
	try {
		session.catalog = await ipc.getCatalog();
	} catch {
		/* no engine yet the welcome screen handles it */
	}
	try {
		session.lastFolder = await ipc.lastWorkspacePath();
	} catch {
		session.lastFolder = null;
	}
}

/** Reopen the folder from the last session (the welcome screen's "Reopen"
 *  button, and Enter on an empty composer with no folder open). */
export async function resumeLastFolder(): Promise<void> {
	if (session.lastFolder) await openFolder(session.lastFolder);
}

/** Open the workspace's fella.md editor from a navigation surface. Unlike the
 *  slash-command path this does not add a command message to the transcript. */
export async function openContext(): Promise<void> {
	if (!requireEngine()) return;
	if (!session.catalog.workspace) {
		session.setWorkspaceView('ask');
		session.addSystem('Open a folder first with /open — fella.md saves into it.');
		return;
	}
	session.setWorkspacePane('context');
}

/** Ask the engine to stop one tab's in-progress run (the active tab by
 *  default). The `ask` promise then resolves normally (a "Stopped." answer) and
 *  clears that tab's `busy`. */
export async function stop(conv: Conversation | null = session.activeChat): Promise<void> {
	if (!conv || !conv.busy || !isTauri()) return;
	conv.activity = 'stopping…';
	try {
		await ipc.cancel(conv.id);
	} catch {
		/* the run may have already finished nothing to stop */
	}
}

/** Correct a running answer: cancel it, then re-ask the same question with the
 *  new line appended. Only reached from a deliberate Enter in the composer while
 *  a run is live (see `Composer.submit`); the transcript shows what happened. */
export async function steerRun(conv: Conversation, extra: string): Promise<void> {
	const prior = [...conv.messages].reverse().find((m) => m.role === 'user');
	if (!prior?.text) return;
	conv.addSystem('↻ Cancelled the current answer and re-asking with your addition.');
	await stop(conv);
	// Let the cancelled run unwind (its `ask` resolves "Stopped." and clears busy).
	for (let i = 0; i < 60 && conv.busy; i++) await new Promise((r) => setTimeout(r, 50));
	conv.addUser(extra);
	await ask(buildQuestion(`${prior.text}\n\nAlso: ${extra}`, conv), conv);
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


	if (text.startsWith('/')) {
		await runCommand(text);
		return;
	}

	const conv = session.ensureChat();
	conv.addUser(text);
	await ask(buildQuestion(text, conv), conv);
}

/** Turn the small UI context selection into explicit model guidance. The
 * catalog and engine still decide what can be read; this only makes the
 * user's chosen starting points visible in the prompt. */
function buildQuestion(question: string, conv: Conversation): string {
	const instructions =
		conv.mode === 'inspect'
			? 'Start by inspecting the relevant workspace sources and schema. Briefly explain what you used and any caveats before giving the answer.'
			: '';
	const refs = conv.contextRefs;
	if (!instructions && refs.length === 0) return question;
	const context = refs.length
		? `Use these references as the starting point for this question. Treat saved results as hypotheses and verify them against the current workspace:\n${refs
				.map((ref) => `- ${contextReferenceText(ref)}`)
				.join('\n')}`
		: '';
	return [instructions, context, question].filter(Boolean).join('\n\n');
}

function contextReferenceText(ref: ContextReference): string {
	return `${ref.label}${ref.detail ? ` (${ref.detail})` : ''}`;
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
async function refreshHealthSoon(): Promise<ProviderHealth | null> {
	try {
		session.health = await ipc.providerHealth();
		return session.health;
	} catch {
		/* ignore the status bar will re-probe on its own timer */
		return null;
	}
}

/** After a key is saved, say so if the provider wouldn't take it. The key stays
 *  saved either way a probe can fail for offline or transient reasons, and we
 *  don't want to block someone who knows their key is fine. */
function warnIfKeyUnverified(display: string, health: ProviderHealth | null): void {
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
	return false;
}

/** The same line with the secret blanked, for the transcript. */
function redactSecret(text: string): string {
	return text
		.replace(/^(\/login\s+\S+\s+key)\s+.+/i, '$1 ••••••')
		.replace(/^(\/model\s+key)\s+.+/i, '$1 ••••••')
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
			const retryChat = session.ensureChat();
			await ask(buildQuestion(q, retryChat), retryChat);
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
					const saved: { workspace?: string | null; messages?: unknown; title?: string | null } =
						JSON.parse(raw);
					const messages = Array.isArray(saved.messages) ? (saved.messages as Message[]) : [];
					session.loadArchivedTab(chosen.id, messages, saved.title ?? null);
					session.addSystem(
						`Reopened: "${chosen.title ?? chosen.preview}" (${dateLabel(chosen.saved_at_ms)}).`
					);
					// Auto-mount the folder this conversation was about (openFolder
					// reports a failure -- moved/deleted folder -- as a system
					// message on its own, nothing extra needed here for that).
					if (saved.workspace && saved.workspace !== session.catalog.workspace) {
						await openFolder(saved.workspace);
					}
					return;
				}
				const lines = list.map((c, i) => {
					const where = c.workspace ? ` · ${baseName(c.workspace)}` : '';
					return `  ${i + 1}. "${c.title ?? c.preview}" · ${c.message_count} message${c.message_count === 1 ? '' : 's'} · ${dateLabel(c.saved_at_ms)}${where}`;
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
						? summarizeCatalog(true)
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
				const conv = session.ensureChat();
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

			// Bare `/login <provider>` and a key is already on file: switch to
			// it, no re-paste. `/login <provider> key` (no value) forces a
			// replacement prompt; so does the first sign-in.
			if (p.authed && !saidKey) {
				if (p.current) {
					session.addSystem(`Already signed in to ${p.display}.`);
					return;
				}
				try {
					session.settings = await ipc.setSettings({ provider: p.id });
					await announceSignedIn(p.display);
				} catch (e) {
					session.addSystem(`error: ${errMsg(e)}`);
				}
				return;
			}

			session.pendingKey = { provider: p.id, display: p.display };
			session.addSystem(
				`${p.authed ? `Replacing your saved ${p.display} key. ` : ''}` +
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

			// `forget` (anywhere) also deletes the saved key; without it,
			// /logout just stops using the service and keeps the key for /login.
			const words = arg.split(/\s+/).filter(Boolean).map((w) => w.toLowerCase());
			const forget = words.includes('forget');
			const named = words.find((w) => w !== 'forget');
			const signedIn = list.filter((p) => p.authed && p.auth !== 'none');
			const active = list.find((p) => p.current);
			const kept = (id: string) =>
				forget ? '' : ` Its key is still saved  /login ${id} to use it again.`;

			// Work out which provider to disconnect.
			let target: ProviderInfo | undefined;
			if (named) {
				target = list.find((x) => x.id === named);
				if (!target) {
					// Not a registered provider but if it's the one settings point
					// at (a stray id from an older build), let the engine clear it.
					if (named === session.settings?.provider) {
						try {
							session.settings = await ipc.logout(named, forget);
							session.providers = await ipc.listProviders();
							session.addSystem(
								`Stopped using ${named}. Run /login to choose another model service.` + kept(named)
							);
							await refreshHealthSoon();
						} catch (e) {
							session.addSystem(`error: ${errMsg(e)}`);
						}
						return;
					}
					session.addSystem(`unknown provider: ${named}\n\n${renderProviders(list)}`);
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
				session.addSystem("You're not connected to a model service. Use /login to connect one.");
				return;
			}

			try {
				const hadKey = target.authed;
				session.settings = await ipc.logout(target.id, forget);
				session.providers = await ipc.listProviders();
				const head = !hadKey
					? `${target.display} had no saved key.`
					: forget
						? `Disconnected from ${target.display} and deleted its saved key.`
						: `Stopped using ${target.display}.${kept(target.id)}`;
				session.addSystem(head + ' Use /login to choose another connected service.');
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
						if (rest.provider || rest.base_url)
							for (const t of session.tabs) if (t.kind === 'chat') t.model = '';
					}
					if (newModel !== undefined) {
						// Per-tab: only the focused conversation switches. Also remember
						// it as the default a fresh tab / next launch starts from.
						session.ensureChat().model = newModel;
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
					perTab
				);
			} catch (e) {
				session.addSystem(`error: ${errMsg(e)}`);
			}
			return;

		case '/reindex':
			if (!requireEngine()) return;
			{
				const conv = session.ensureChat();
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

		case '/memory': {
			if (!requireEngine()) return;
			try {
				if (arg.trim().toLowerCase() === 'forget') {
					const had = await ipc.forgetMemory();
					session.addSystem(
						had
							? 'Cleared what Fella had learned about this folder.'
							: 'Nothing learned about this folder yet.'
					);
					return;
				}
				const res = await ipc.memoryFile();
				if (!res) {
					session.addSystem('Open a folder first, then /memory shows what Fella has learned about it.');
					return;
				}
				const [path, contents] = res;
				session.addSystem(
					contents?.trim()
						? `What Fella has learned about this folder (edit this file directly; /memory forget clears it):\n${path}\n\n${contents.trim()}`
						: `Nothing learned about this folder yet — Fella fills this in as you ask and correct it.\nFile (once it exists): ${path}`
				);
			} catch (e) {
				session.addSystem(`error: ${errMsg(e)}`);
			}
			return;
		}

		case '/context': {
			// fella.md is the one explicit workspace context file the engine reads.
			await openContext();
			return;
		}

		case '/update':
			if (!requireEngine()) return;
			{
				const conv = session.ensureChat();
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

		case '/mcp':
			session.addSystem(
				'MCP is experimental and closed in this release.\n' +
					'No official connectors are enabled.\n' +
					'Custom implementations require a fork or experimental build.'
			);
			return;

		default:
			session.addSystem(`unknown command: ${cmd}\n\n${HELP}`);
			return;
		}
	}

/** Run one question in `conv` (its own tab). Bound to the tab, not "the active
 *  tab", so it keeps streaming there after the user switches away. */
async function ask(question: string, conv: Conversation): Promise<void> {
	if (!requireEngine()) return;

	const msg = conv.addAssistant('');
	conv.startRun();
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

	// Transient engine notices (retry/backoff, provider problems) normally only
	// flash in the status bar. Keep them so that if the run ends badly the user
	// has the warning that explains why.
	const notices: string[] = [];
	let failed = false;

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
				conv.beginRunStep(e.tool, note || undefined);
				conv.activity = note ? `${note}…` : 'working…';
				break;
			}
			case 'tool_end':
				conv.completeRunStep(e.item);
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
		const answer = await ipc.ask(conv.id, question, onEvent, conv.model || undefined, conv.mode);
		msg.answer = answer;
		msg.text = answer.text;
	} catch (e) {
		failed = true;
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
		conv.finishRun(failed);
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

function renderProviders(list: ProviderInfo[]): string {
	const rows = list.map((p) => {
		const bullet = p.current ? '●' : ' ';
		const status = p.authed ? 'connected' : 'not connected';
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

/** Short by default ("<folder>: N files ready.") the user knows what's in
 *  their folder. `full` lists every file and is only used by `/files`. */
function summarizeCatalog(full = false): string {
	const s = session.catalog.sources;
	const folder = session.catalog.workspace?.replace(/^.*[/\\]/, '') ?? 'folder';
	if (s.length === 0)
		return `${folder}: nothing Fella can read here yet. It works with spreadsheets, CSVs, PDFs, and text files.`;
	const skipped = session.catalog.skipped ?? [];
	if (!full) {
		const tail = skipped.length ? `  ${skipped.length} skipped — /files for details.` : '';
		return `${folder}: ${s.length} file${s.length === 1 ? '' : 's'} ready.${tail}  Ask a question, or /help.`;
	}
	const tables = s.filter((f) => f.view);
	const docs = s.filter((f) => !f.view);
	const lines = [`${folder}: ${s.length} file${s.length === 1 ? '' : 's'}`];
	for (const f of tables) {
		lines.push(`  ${f.name}  ·  ${f.row_count ?? '?'} rows${f.note ? `  (${f.note})` : ''}`);
	}
	for (const f of docs) {
		lines.push(`  ${f.name}  ·  ${f.kind.toUpperCase()}`);
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
export function baseName(path: string): string {
	const parts = path.split(/[/\\]+/).filter(Boolean);
	return parts[parts.length - 1] ?? path;
}

/** A short, local-time label for `/history`'s list — just a date once it's
 *  not today, so the list stays scannable. */
export function dateLabel(ms: number): string {
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

/** "27m" / "3h" / "5d" style relative age, for the sidebar's recency rows. */
export function relativeAge(ms: number): string {
	const mins = Math.round((Date.now() - ms) / 60_000);
	if (mins < 1) return 'now';
	if (mins < 60) return `${mins}m`;
	const hrs = Math.round(mins / 60);
	if (hrs < 24) return `${hrs}h`;
	const days = Math.round(hrs / 24);
	return `${days}d`;
}

/** The engine serialises errors as `{ kind, message }`; older / Tauri-internal
 *  errors are plain strings or `Error`s. Unwrap either. */
export function errMsg(e: unknown): string {
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

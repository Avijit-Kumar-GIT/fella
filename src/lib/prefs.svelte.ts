// Per-install UI preferences. Appearance is a local UI choice; theme packs
// remain durable engine state and can still provide their own CSS tokens.

import { ipc, isTauri } from './ipc';

export type Appearance = 'system' | 'light' | 'dark';

const APPEARANCE_KEY = 'fella:appearance';

function readAppearance(): Appearance {
	if (typeof localStorage === 'undefined') return 'system';
	try {
		const saved = localStorage.getItem(APPEARANCE_KEY);
		return saved === 'light' || saved === 'dark' || saved === 'system' ? saved : 'system';
	} catch {
		return 'system';
	}
}

function readSystemDark(): boolean {
	return (
		typeof window !== 'undefined' &&
		typeof window.matchMedia === 'function' &&
		window.matchMedia('(prefers-color-scheme: dark)').matches
	);
}

/** The CSS custom properties a theme pack may set (matches THEME_TOKEN_KEYS in
 *  src-tauri/src/engine/extensions.rs). Anything else is ignored. */
const THEME_TOKEN_KEYS = new Set([
	'--bg',
	'--bg-raised',
	'--bg-inset',
	'--border',
	'--border-strong',
	'--text',
	'--text-dim',
	'--text-faint',
	'--accent',
	'--brand',
	'--link',
	'--ok',
	'--warn',
	'--err',
	'--radius',
	'--pad'
]);

class Prefs {
	/** The selected appearance. System follows the OS preference. */
	appearance = $state<Appearance>(readAppearance());
	/** The currently resolved light/dark mode, used by CSS and image assets. */
	systemDark = $state(readSystemDark());
	isDark = $state(false);

	/** Tokens of the active theme pack, or null for the built-in look. */
	themeTokens = $state<Record<string, string> | null>(null);

	constructor() {
		this.syncColorMode();
		if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') return;
		const query = window.matchMedia('(prefers-color-scheme: dark)');
		this.systemDark = query.matches;
		query.addEventListener('change', (event) => {
			this.systemDark = event.matches;
			if (this.appearance === 'system') {
				this.syncColorMode();
				this.apply();
			}
		});
	}

	private syncColorMode(): void {
		this.isDark = this.appearance === 'dark' || (this.appearance === 'system' && this.systemDark);
	}

	setAppearance(appearance: Appearance): void {
		this.appearance = appearance;
		this.syncColorMode();
		try {
			localStorage.setItem(APPEARANCE_KEY, appearance);
		} catch {
			/* A restricted webview can still use the setting for this session. */
		}
		this.apply();
	}

	/** Pull the active theme from the engine. Safe to call outside Tauri. */
	async load(): Promise<void> {
		if (!isTauri()) return;
		try {
			this.themeTokens = await ipc.packsTheme();
		} catch {
			this.themeTokens = null;
		}
	}

	/** Apply appearance and `themeTokens` to <html>. */
	apply(): void {
		if (typeof document === 'undefined') return;
		const root = document.documentElement;
		root.dataset.appearance = this.appearance;
		root.dataset.colorMode = this.isDark ? 'dark' : 'light';
		root.style.colorScheme = this.isDark ? 'dark' : 'light';
		for (const k of THEME_TOKEN_KEYS) root.style.removeProperty(k);
		const t = this.themeTokens;
		if (!t) return;
		for (const [k, v] of Object.entries(t)) {
			if (THEME_TOKEN_KEYS.has(k) && typeof v === 'string') {
				root.style.setProperty(k, v);
			}
		}
	}
}

export const prefs = new Prefs();

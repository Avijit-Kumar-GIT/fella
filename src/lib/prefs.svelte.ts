// Per-install UI preferences. Today this is just the active theme, whose
// durable truth lives in Rust (the enabled `theme` pack); this singleton
// mirrors it and applies the CSS tokens to the document.

import { ipc, isTauri } from './ipc';

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
	'--link',
	'--ok',
	'--warn',
	'--err',
	'--radius',
	'--pad'
]);

class Prefs {
	/** Tokens of the active theme pack, or null for the built-in look. */
	themeTokens = $state<Record<string, string> | null>(null);

	/** Pull the active theme from the engine. Safe to call outside Tauri. */
	async load(): Promise<void> {
		if (!isTauri()) return;
		try {
			this.themeTokens = await ipc.packsTheme();
		} catch {
			this.themeTokens = null;
		}
	}

	/** Apply `themeTokens` to <html>, clearing any previously-set overrides. */
	apply(): void {
		if (typeof document === 'undefined') return;
		const root = document.documentElement;
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

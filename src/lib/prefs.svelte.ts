// Per-install UI preferences. Appearance is a local UI choice.

import { ipc, isDesktop } from './ipc';

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

class Prefs {
	/** The selected appearance. System follows the OS preference. */
	appearance = $state<Appearance>(readAppearance());
	/** The currently resolved light/dark mode, used by CSS and image assets. */
	systemDark = $state(readSystemDark());
	isDark = $state(false);


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

	/** Apply the selected appearance to the document and native window. */
	apply(): void {
		if (typeof document === 'undefined') return;
		const root = document.documentElement;
		root.dataset.appearance = this.appearance;
		root.dataset.colorMode = this.isDark ? 'dark' : 'light';
		root.style.colorScheme = this.isDark ? 'dark' : 'light';
		if (isDesktop()) void ipc.setWindowAppearance(this.isDark).catch(() => {});
	}
}

export const prefs = new Prefs();

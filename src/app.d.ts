// See https://svelte.dev/docs/kit/types#app.d.ts
// for information about these interfaces
import type { AskEvent } from './lib/types';

declare global {
	interface Window {
		/** Secure Electron preload bridge. It is absent in a normal browser. */
		fella?: {
			invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
			ask(
				params: {
					conversationId: string;
					question: string;
					model: string | null;
					mode: string | null;
				},
				onEvent: (event: AskEvent) => void
			): Promise<import('./lib/types').Answer>;
			pickFolder(): Promise<string | null>;
			openExternal(url: string): Promise<void>;
			setWindowAppearance(dark: boolean): Promise<void>;
			pathForFile(file: File): string;
			windowAction(action: 'minimize' | 'toggleMaximize' | 'close'): Promise<void>;
		};
	}

	namespace App {
		// interface Error {}
		// interface Locals {}
		// interface PageData {}
		// interface PageState {}
		// interface Platform {}
	}
}

export {};

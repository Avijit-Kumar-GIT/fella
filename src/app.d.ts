// See https://svelte.dev/docs/kit/types#app.d.ts
// for information about these interfaces
import type { AskEvent, Catalog, WorkspaceProgress } from './lib/types';

declare global {
	interface Window {
		/** Secure Electron preload bridge. It is absent in a normal browser. */
		fella?: {
			invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
			openWorkspace(
				path: string,
				onProgress: (progress: WorkspaceProgress) => void
			): Promise<Catalog>;
			/** Installed shells detected on the host; Fella does not install shells. */
			listTerminalShells(): Promise<{
				shells: Array<{ path: string; name: string }>;
				defaultShellPath: string | null;
			}>;
			/** Starts a user-only PTY session. Its stream is isolated from model and analytics IPC. */
			startTerminal(
				workspacePath: string,
				shellPath: string,
				cols: number,
				rows: number,
				onData: (data: string) => void,
				onExit: (event: { exitCode: number; signal: string | null }) => void
			): Promise<{ terminalId: string; cwd: string; shell: string; shellPath: string }>;
			writeTerminal(terminalId: string, data: string): void;
			resizeTerminal(terminalId: string, cols: number, rows: number): void;
			closeTerminal(terminalId: string): void;
			ask(
				params: {
					conversationId: string;
					question: string;
					model: string | null;
					mode: string | null;
					contextRefs: import('./lib/types').ContextReference[];
					clarificationReply?: import('./lib/types').ClarificationReply | null;
					workspaceId?: string | null;
					investigationId?: string | null;
					questionId?: string | null;
				},
				onEvent: (event: AskEvent) => void
			): Promise<import('./lib/types').Answer>;
			rerunAnalysisTurn(
				params: {
					turnId: string;
					model: string | null;
					mode: string | null;
					workspaceId?: string | null;
				},
				onEvent: (event: AskEvent) => void
			): Promise<import('./lib/types').Answer>;
			pickFolder(): Promise<string | null>;
			sampleWorkspacePath(): Promise<string | null>;
			openExternal(url: string): Promise<void>;
			setWindowAppearance(dark: boolean): Promise<void>;
			pathForFile(file: File): string;
			windowAction(action: 'minimize' | 'toggleMaximize' | 'close'): Promise<void>;
			onAppCommand(callback: (command: string) => void): void;
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

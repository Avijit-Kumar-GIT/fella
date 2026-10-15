<script lang="ts">
	import { FitAddon } from '@xterm/addon-fit';
	import { Terminal } from '@xterm/xterm';
	import '@xterm/xterm/css/xterm.css';
	import { onDestroy, onMount } from 'svelte';
	import Icon from './Icon.svelte';
	import {
		closeWorkspaceTerminal,
		listWorkspaceTerminalShells,
		resizeWorkspaceTerminal,
		startWorkspaceTerminal,
		writeWorkspaceTerminal
	} from '$lib/ipc';
	import { session } from '$lib/session.svelte';

	const CONNECTION_LABELS = {
		starting: 'Starting',
		connected: 'Connected',
		exited: 'Exited',
		error: 'Unavailable',
		unavailable: 'No shell'
	} as const;

	let { active = true } = $props<{ active?: boolean }>();
	let workspacePath = $derived(session.catalog.workspace);
	let terminalHost: HTMLDivElement;
	let terminalState = $state<'starting' | 'connected' | 'exited' | 'error' | 'unavailable'>('starting');
	let error = $state('');
	let currentShellPath = $state('');
	let selectedShellPath = $state('');
	let shellOptions = $state<Array<{ path: string; name: string }>>([]);
	let connectionLabel = $derived(CONNECTION_LABELS[terminalState]);
	let terminalId = '';
	let generation = 0;
	let destroyed = false;
	let terminal: Terminal | null = null;
	let fitAddon: FitAddon | null = null;
	let resizeObserver: ResizeObserver | null = null;

	function fitTerminal(): void {
		if (!terminal || !fitAddon || !terminalHost?.clientWidth || !terminalHost.clientHeight) return;
		try {
			fitAddon.fit();
			if (terminalId) resizeWorkspaceTerminal(terminalId, terminal.cols, terminal.rows);
		} catch {
			// A hidden or newly resized tab can briefly have no measurable cell grid.
		}
	}

	function terminalTheme(): NonNullable<ConstructorParameters<typeof Terminal>[0]>['theme'] {
		const styles = getComputedStyle(terminalHost);
		const color = (name: string, fallback: string) => styles.getPropertyValue(name).trim() || fallback;
		return {
			background: color('--bg-inset', '#1e1e22'),
			foreground: color('--text', '#ededec'),
			cursor: color('--brand', '#008080'),
			cursorAccent: color('--bg-inset', '#1e1e22'),
			selectionBackground: 'rgba(0, 128, 128, 0.32)'
		};
	}

	function terminalFontFamily(): string {
		return (
			getComputedStyle(terminalHost).getPropertyValue('--mono').trim() ||
			'ui-monospace, "SFMono-Regular", "Cascadia Code", Consolas, monospace'
		);
	}

	function displayWorkspacePath(path: string | null): string {
		if (!path) return '';
		return path.replace(/^\\\\\?\\UNC\\/i, '\\\\').replace(/^\\\\\?\\/, '');
	}

	async function launchTerminal(): Promise<void> {
		const path = workspacePath;
		if (!path || !terminal || !selectedShellPath) {
			terminalState = 'unavailable';
			return;
		}
		const currentGeneration = ++generation;
		const shellTerminal = terminal;
		terminalState = 'starting';
		error = '';
		let exitedWhileStarting = false;
		try {
			const result = await startWorkspaceTerminal(
				path,
				selectedShellPath,
				shellTerminal.cols || 80,
				shellTerminal.rows || 24,
				(data) => {
					if (!destroyed && generation === currentGeneration) shellTerminal.write(data);
				},
				() => {
					if (destroyed || generation !== currentGeneration) return;
					exitedWhileStarting = !terminalId;
					terminalState = 'exited';
				}
			);
			if (destroyed || generation !== currentGeneration) {
				closeWorkspaceTerminal(result.terminalId);
				return;
			}
			terminalId = result.terminalId;
			currentShellPath = result.shellPath;
			terminalState = exitedWhileStarting ? 'exited' : 'connected';
			requestAnimationFrame(() => {
				fitTerminal();
				if (active) terminal?.focus();
			});
		} catch (cause) {
			if (destroyed || generation !== currentGeneration) return;
			terminalState = 'error';
			error = cause instanceof Error ? cause.message : String(cause);
		}
	}

	async function initializeShells(): Promise<void> {
		try {
			const available = await listWorkspaceTerminalShells();
			if (destroyed) return;
			shellOptions = available.shells;
			let savedShell = '';
			try {
				savedShell = localStorage.getItem('fella.shell.path') ?? '';
			} catch {
				// Storage may be unavailable; use the host's default shell.
			}
			selectedShellPath = shellOptions.some((item) => item.path === savedShell)
				? savedShell
				: available.defaultShellPath ?? shellOptions[0]?.path ?? '';
			if (!selectedShellPath) {
				terminalState = 'unavailable';
				return;
			}
			void launchTerminal();
		} catch (cause) {
			if (destroyed) return;
			terminalState = 'error';
			error = cause instanceof Error ? cause.message : String(cause);
		}
	}

	function selectShell(path: string): void {
		selectedShellPath = path;
		try {
			localStorage.setItem('fella.shell.path', path);
		} catch {
			// The selection remains active for this session when storage is unavailable.
		}
	}

	async function restartTerminal(): Promise<void> {
		const oldId = terminalId;
		terminalId = '';
		generation += 1;
		if (oldId) closeWorkspaceTerminal(oldId);
		terminal?.reset();
		await launchTerminal();
	}

	onMount(() => {
		terminal = new Terminal({
			allowProposedApi: false,
			convertEol: false,
			cursorBlink: true,
			fontFamily: terminalFontFamily(),
			fontSize: 13,
			letterSpacing: 0,
			lineHeight: 1.35,
			scrollback: 5000,
			theme: terminalTheme()
		});
		fitAddon = new FitAddon();
		terminal.loadAddon(fitAddon);
		terminal.open(terminalHost);
		terminal.onData((data) => {
			if (terminalId) writeWorkspaceTerminal(terminalId, data);
		});
		terminal.onResize(({ cols, rows }) => {
			if (terminalId) resizeWorkspaceTerminal(terminalId, cols, rows);
		});
		resizeObserver = new ResizeObserver(() => fitTerminal());
		resizeObserver.observe(terminalHost);
		requestAnimationFrame(() => {
			fitTerminal();
			void initializeShells();
		});
	});

	$effect(() => {
		if (!terminal) return;
		if (active) {
			requestAnimationFrame(() => {
				fitTerminal();
				terminal?.focus();
			});
		} else {
			terminal.blur();
		}
	});

	onDestroy(() => {
		destroyed = true;
		generation += 1;
		resizeObserver?.disconnect();
		if (terminalId) closeWorkspaceTerminal(terminalId);
		terminal?.dispose();
	});
</script>

<section class="terminal-view" aria-label="Shell">
	<header class="terminal-toolbar">
		<div class="terminal-location">
			<span class="terminal-connection" role="status">
				<span
					class="terminal-state-dot"
					class:connected={terminalState === 'connected'}
					class:starting={terminalState === 'starting'}
					aria-hidden="true"
				></span>
				{connectionLabel}
			</span>
			{#if workspacePath}
				<span class="terminal-location-divider" aria-hidden="true"></span>
				<Icon name="folder" size={14} />
				<code title={workspacePath}>{displayWorkspacePath(workspacePath)}</code>
			{/if}
		</div>
		<div class="shell-controls">
			<label class="shell-picker-label">
				<span>Shell</span>
				<span class="shell-picker-control">
					<select
						aria-label="Shell"
						data-testid="workspace-shell-picker"
						value={selectedShellPath}
						disabled={shellOptions.length === 0 || terminalState === 'starting'}
						onchange={(event) => selectShell(event.currentTarget.value)}
					>
						{#each shellOptions as option (option.path)}
							<option value={option.path}>{option.name}</option>
						{/each}
					</select>
					<Icon name="chevron-down" size={14} />
				</span>
			</label>
			{#if selectedShellPath && currentShellPath !== selectedShellPath}
				<span class="shell-pending" role="status">Restart to apply</span>
			{/if}
			<button
				class="terminal-restart"
				type="button"
				aria-label={currentShellPath && currentShellPath !== selectedShellPath ? 'Switch shell' : 'Restart shell'}
				disabled={terminalState === 'starting' || !workspacePath || !selectedShellPath}
				onclick={() => void restartTerminal()}
			>
				<Icon name="refresh" size={14} />
				{currentShellPath && currentShellPath !== selectedShellPath ? 'Switch shell' : 'Restart shell'}
			</button>
		</div>
	</header>
	{#if terminalState === 'error'}
		<p class="terminal-error" role="alert">{error}</p>
	{:else if terminalState === 'unavailable'}
		<p class="terminal-unavailable">
			{shellOptions.length === 0 ? 'No installed shells were found on this system.' : 'Open a workspace to start a shell there.'}
		</p>
	{/if}
	<div class="terminal-surface" bind:this={terminalHost} data-testid="terminal-surface" aria-label="Interactive shell"></div>
</section>

<style>
	.terminal-view {
		display: flex;
		flex: 1;
		flex-direction: column;
		min-width: 0;
		min-height: 0;
		background: var(--workspace-canvas);
		container-type: inline-size;
	}
	.terminal-toolbar {
		display: flex;
		min-height: 52px;
		flex: none;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-4);
		padding: 0 var(--space-3);
		border-bottom: 1px solid var(--border);
		background: var(--workspace-canvas);
	}
	.terminal-location {
		display: flex;
		min-width: 0;
		align-items: center;
		gap: var(--space-2);
		color: var(--text-dim);
		font-size: var(--fs-xs);
		white-space: nowrap;
	}
	.terminal-connection {
		display: inline-flex;
		flex: none;
		align-items: center;
		gap: var(--space-2);
		color: var(--text-dim);
		font-size: var(--fs-xs);
	}
	.terminal-state-dot {
		width: 7px;
		height: 7px;
		flex: none;
		border-radius: 50%;
		background: var(--text-faint);
	}
	.terminal-state-dot.connected {
		background: var(--ok);
		box-shadow: 0 0 0 2px color-mix(in srgb, var(--ok) 12%, transparent);
	}
	.terminal-state-dot.starting { background: var(--brand); animation: terminal-pulse 1.3s ease-in-out infinite; }
	.terminal-location-divider {
		width: 1px;
		height: 18px;
		flex: none;
		margin: 0 var(--space-1);
		background: var(--border);
	}
	.terminal-location :global(svg.icon) { color: var(--text-faint); }
	.terminal-location code {
		overflow: hidden;
		min-width: 0;
		color: var(--text-dim);
		font-family: var(--mono);
		font-size: var(--fs-xs);
		letter-spacing: 0;
		text-overflow: ellipsis;
		white-space: nowrap;
		unicode-bidi: plaintext;
	}
	.terminal-restart {
		display: inline-flex;
		min-height: 32px;
		flex: none;
		align-items: center;
		justify-content: center;
		gap: var(--space-1);
		padding: 0 11px;
		border: 1px solid var(--border);
		border-radius: var(--radius);
		background: transparent;
		color: var(--text);
		font: inherit;
		font-size: var(--fs-xs);
		cursor: pointer;
	}
	.terminal-restart:hover:not(:disabled) { border-color: var(--border-strong); background: var(--bg-inset); }
	.terminal-restart:focus-visible { outline: none; box-shadow: var(--focus-ring); }
	.terminal-restart:disabled { cursor: default; opacity: 0.5; }
	.shell-controls { display: flex; flex: none; align-items: center; gap: var(--space-2); }
	.shell-picker-label { display: flex; align-items: center; gap: var(--space-2); color: var(--text-faint); font-size: var(--fs-xs); }
	.shell-picker-control {
		position: relative;
		display: flex;
		min-width: 140px;
		max-width: 220px;
		min-height: 32px;
		align-items: center;
		border: 1px solid var(--border);
		border-radius: var(--radius);
		background: var(--bg-raised);
		transition: border-color var(--dur-fast) var(--ease), background var(--dur-fast) var(--ease);
	}
	.shell-picker-control:hover { border-color: var(--border-strong); }
	.shell-picker-control:focus-within { border-color: var(--link); box-shadow: var(--focus-ring); }
	.shell-picker-control :global(svg) {
		position: absolute;
		right: 9px;
		color: var(--text-faint);
		pointer-events: none;
	}
	.shell-picker-control select {
		width: 100%;
		min-width: 0;
		min-height: 30px;
		appearance: none;
		padding: 0 32px 0 11px;
		border: 0;
		border-radius: inherit;
		outline: 0;
		background: transparent;
		color: var(--text);
		font: inherit;
		font-size: var(--fs-xs);
		text-overflow: ellipsis;
		cursor: pointer;
	}
	.shell-picker-control select:disabled { opacity: 0.55; cursor: default; }
	.shell-pending { color: var(--text-faint); font-size: 11px; white-space: nowrap; }
	.terminal-surface {
		flex: 1;
		min-width: 0;
		min-height: 0;
		margin: var(--space-3);
		padding: var(--space-3) var(--space-4);
		overflow: hidden;
		border: 1px solid var(--pane-edge);
		border-radius: var(--radius-window);
		background: var(--bg-inset);
	}
	.terminal-surface :global(.xterm) { height: 100%; }
	.terminal-surface :global(.xterm-viewport) { scrollbar-width: thin; }
	.terminal-error, .terminal-unavailable {
		margin: 0;
		padding: 10px 16px;
		border-bottom: 1px solid var(--pane-edge);
		color: var(--danger, #b42318);
		font-size: var(--fs-sm);
		line-height: 1.5;
		overflow-wrap: anywhere;
	}
	.terminal-unavailable { color: var(--text-faint); }
	@keyframes terminal-pulse {
		50% { opacity: 0.45; }
	}
	@container (max-width: 720px) {
		.terminal-toolbar {
			align-items: stretch;
			flex-direction: column;
			gap: var(--space-2);
			padding: var(--space-2) var(--space-3);
		}
		.terminal-location { width: 100%; min-height: 28px; }
		.terminal-location code { flex: 1; }
		.shell-controls { width: 100%; flex-wrap: wrap; justify-content: flex-end; }
		.terminal-surface { margin: var(--space-2); padding: var(--space-2); border-radius: var(--radius-md); }
	}
	@media (prefers-reduced-motion: reduce) {
		.terminal-state-dot.starting { animation: none; }
	}
</style>

<script lang="ts">
	import { FitAddon } from '@xterm/addon-fit';
	import { Terminal } from '@xterm/xterm';
	import '@xterm/xterm/css/xterm.css';
	import { onDestroy, onMount } from 'svelte';
	import {
		closeWorkspaceTerminal,
		listWorkspaceTerminalShells,
		resizeWorkspaceTerminal,
		startWorkspaceTerminal,
		writeWorkspaceTerminal
	} from '$lib/ipc';
	import { session } from '$lib/session.svelte';

	let { active = true } = $props<{ active?: boolean }>();
	let workspacePath = $derived(session.catalog.workspace);
	let terminalHost: HTMLDivElement;
	let terminalState = $state<'starting' | 'connected' | 'exited' | 'error' | 'unavailable'>('starting');
	let error = $state('');
	let currentShell = $state('');
	let currentShellPath = $state('');
	let selectedShellPath = $state('');
	let shellOptions = $state<Array<{ path: string; name: string }>>([]);
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
			currentShell = result.shell;
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
			fontFamily: 'var(--mono)',
			fontSize: 13,
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
		<div class="terminal-location" title={workspacePath ?? ''}>
			<span class="terminal-state-dot" class:connected={terminalState === 'connected'} aria-hidden="true"></span>
			<span class="terminal-location-label">{terminalState === 'connected' ? currentShell : 'Shell'}</span>
			{#if workspacePath}
				<code>{workspacePath}</code>
			{/if}
		</div>
		<div class="shell-controls">
			<label class="shell-picker-label">
				<span>Shell</span>
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
			</label>
			{#if selectedShellPath && currentShellPath !== selectedShellPath}
				<span class="shell-pending" role="status">Restart to apply</span>
			{/if}
			<button
				class="terminal-restart"
				type="button"
				disabled={terminalState === 'starting' || !workspacePath || !selectedShellPath}
				onclick={() => void restartTerminal()}
			>
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
		background: var(--bg);
	}
	.terminal-toolbar {
		display: flex;
		min-height: 42px;
		flex: none;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-3);
		padding: 0 var(--space-4);
		border-bottom: 1px solid var(--pane-edge);
	}
	.terminal-location {
		display: flex;
		min-width: 0;
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
	.terminal-state-dot.connected { background: #43a879; }
	.terminal-location-label { flex: none; color: var(--text); font-weight: 560; }
	.terminal-location code {
		overflow: hidden;
		color: var(--text-faint);
		font-family: var(--mono);
		font-size: 11px;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.terminal-restart {
		min-height: 28px;
		flex: none;
		padding: 0 10px;
		border: 1px solid var(--pane-edge);
		border-radius: 999px;
		background: transparent;
		color: var(--text-dim);
		font: inherit;
		font-size: var(--fs-xs);
		cursor: pointer;
	}
	.terminal-restart:hover:not(:disabled) { background: var(--bg-inset); color: var(--text); }
	.terminal-restart:focus-visible { outline: none; box-shadow: var(--focus-ring); }
	.terminal-restart:disabled { cursor: default; opacity: 0.5; }
	.shell-controls { display: flex; flex: none; align-items: center; gap: var(--space-2); }
	.shell-picker-label { display: flex; align-items: center; gap: 7px; color: var(--text-faint); font-size: var(--fs-xs); }
	.shell-picker-label select {
		max-width: 180px;
		min-height: 28px;
		padding: 0 26px 0 9px;
		border: 1px solid var(--pane-edge);
		border-radius: 999px;
		background: var(--bg);
		color: var(--text);
		font: inherit;
		font-size: var(--fs-xs);
	}
	.shell-picker-label select:focus-visible { outline: none; box-shadow: var(--focus-ring); }
	.shell-picker-label select:disabled { opacity: 0.55; }
	.shell-pending { color: var(--text-faint); font-size: 11px; white-space: nowrap; }
	.terminal-surface {
		flex: 1;
		min-width: 0;
		min-height: 0;
		padding: 14px 16px;
		overflow: hidden;
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
</style>

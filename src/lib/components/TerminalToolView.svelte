<script lang="ts">
	import { FitAddon } from '@xterm/addon-fit';
	import { Terminal } from '@xterm/xterm';
	import '@xterm/xterm/css/xterm.css';
	import { onDestroy, onMount } from 'svelte';
	import {
		closeWorkspaceTerminal,
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
		if (!path || !terminal) {
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
			void launchTerminal();
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

<section class="terminal-view" aria-label="Terminal">
	<header class="terminal-toolbar">
		<div class="terminal-location" title={workspacePath ?? ''}>
			<span class="terminal-state-dot" class:connected={terminalState === 'connected'} aria-hidden="true"></span>
			<span class="terminal-location-label">{terminalState === 'connected' ? currentShell : 'Terminal'}</span>
			{#if workspacePath}
				<code>{workspacePath}</code>
			{/if}
		</div>
		<button class="terminal-restart" type="button" disabled={terminalState === 'starting' || !workspacePath} onclick={() => void restartTerminal()}>
			Restart
		</button>
	</header>
	{#if terminalState === 'error'}
		<p class="terminal-error" role="alert">{error}</p>
	{:else if terminalState === 'unavailable'}
		<p class="terminal-unavailable">Open a workspace to start a terminal there.</p>
	{/if}
	<div class="terminal-surface" bind:this={terminalHost} data-testid="terminal-surface" aria-label="Interactive terminal"></div>
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

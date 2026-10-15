<script lang="ts">
	import { FitAddon } from '@xterm/addon-fit';
	import { Terminal } from '@xterm/xterm';
	import '@xterm/xterm/css/xterm.css';
	import { onDestroy, onMount } from 'svelte';
	import { DropdownMenu } from '$lib/components/ui/dropdown-menu';
	import Icon from './Icon.svelte';
	import {
		closeWorkspaceTerminal,
		listWorkspaceTerminalShells,
		resizeWorkspaceTerminal,
		startWorkspaceTerminal,
		writeWorkspaceTerminal
	} from '$lib/ipc';
	import { session } from '$lib/session.svelte';

	let { active = true, tabId = 'terminal' } = $props<{ active?: boolean; tabId?: string }>();
	let workspacePath = $derived(session.catalog.workspace);
	let terminalHost: HTMLDivElement;
	let terminalState = $state<'starting' | 'connected' | 'exited' | 'error' | 'unavailable'>('starting');
	let error = $state('');
	let currentShellPath = $state('');
	let selectedShellPath = $state('');
	let shellOptions = $state<Array<{ path: string; name: string }>>([]);
	let shellMenuOpen = $state(false);
	let selectedShellName = $derived(shellOptions.find((item) => item.path === selectedShellPath)?.name ?? 'Choose shell');
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
		const dark = document.documentElement.getAttribute('data-color-mode') === 'dark';
		return {
			background: color('--bg-inset', '#1e1e22'),
			foreground: color('--text', '#ededec'),
			cursor: color('--brand', '#008080'),
			cursorAccent: color('--bg-inset', '#1e1e22'),
			selectionBackground: 'rgba(0, 128, 128, 0.32)',
			// GNU ls uses blue text on a green background for writable
			// directories. Keep that distinction, but bring the ANSI colors into
			// Fella's restrained palette instead of xterm's high-saturation defaults.
			green: dark ? '#315b4c' : '#d2e6d8',
			brightGreen: dark ? '#75b894' : '#397652',
			blue: dark ? '#bfd0ef' : '#315c8e',
			brightBlue: dark ? '#d2def4' : '#4a77ad'
		};
	}

	function terminalFontFamily(): string {
		return (
			getComputedStyle(terminalHost).getPropertyValue('--mono').trim() ||
			'ui-monospace, "SFMono-Regular", "Cascadia Code", Consolas, monospace'
		);
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
		<div class="shell-controls">
			<span class="shell-control-label">Shell</span>
			<DropdownMenu.Root open={shellMenuOpen} onOpenChange={(open) => (shellMenuOpen = open)}>
				<DropdownMenu.Trigger
					class="terminal-shell-trigger"
					aria-label={`Shell: ${selectedShellName}`}
					data-testid="workspace-shell-picker"
					disabled={shellOptions.length === 0 || terminalState === 'starting'}
				>
					<span>{selectedShellName}</span>
					<Icon name="chevron-down" size={14} />
				</DropdownMenu.Trigger>
				<DropdownMenu.Portal>
					<DropdownMenu.Content class="fella-ui-menu-content terminal-shell-menu" side="bottom" align="end" sideOffset={5}>
						<DropdownMenu.RadioGroup value={selectedShellPath} onValueChange={selectShell}>
							{#each shellOptions as option (option.path)}
								<DropdownMenu.RadioItem class="fella-ui-menu-item terminal-shell-option" value={option.path}>
									<span>{option.name}</span>
									{#if option.path === selectedShellPath}<Icon name="check" size={14} />{/if}
								</DropdownMenu.RadioItem>
							{/each}
						</DropdownMenu.RadioGroup>
					</DropdownMenu.Content>
				</DropdownMenu.Portal>
			</DropdownMenu.Root>
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
	<div class="terminal-surface" bind:this={terminalHost} data-testid="terminal-surface" data-terminal-tab={tabId} aria-label="Interactive shell"></div>
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
		min-height: 48px;
		flex: none;
		align-items: center;
		justify-content: flex-end;
		gap: var(--space-4);
		padding: 0 var(--space-3);
		border-bottom: 1px solid var(--border);
		background: var(--workspace-canvas);
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
	.shell-controls { display: flex; min-width: 0; align-items: center; gap: var(--space-2); }
	.shell-control-label { color: var(--text-faint); font-size: var(--fs-xs); }
	:global(.terminal-shell-trigger) {
		display: inline-flex;
		width: 180px;
		max-width: min(220px, 40vw);
		min-width: 120px;
		min-height: 34px;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-2);
		padding: 5px 9px 5px 10px;
		border: 1px solid var(--border);
		border-radius: var(--radius);
		background: var(--bg-raised);
		color: var(--text);
		font: inherit;
		font-size: var(--fs-xs);
		text-align: left;
		transition: background var(--dur-fast) var(--ease), border-color var(--dur-fast) var(--ease);
	}
	:global(.terminal-shell-trigger > span) { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	:global(.terminal-shell-trigger > svg) { flex: none; color: var(--text-faint); transform: rotate(90deg); transition: transform var(--dur-fast) var(--ease); }
	:global(.terminal-shell-trigger:hover:not(:disabled)),
	:global(.terminal-shell-trigger[data-state='open']) { border-color: var(--border-strong); background: var(--bg-inset); }
	:global(.terminal-shell-trigger[data-state='open'] > svg) { transform: rotate(-90deg); }
	:global(.terminal-shell-trigger:focus-visible) { outline: 2px solid color-mix(in srgb, var(--brand) 52%, transparent); outline-offset: 2px; }
	:global(.terminal-shell-trigger:disabled) { cursor: default; opacity: 0.55; }
	:global(.terminal-shell-menu) { min-width: 180px; max-width: min(280px, calc(100vw - 24px)); }
	:global(.terminal-shell-option) { justify-content: space-between; gap: var(--space-3); }
	:global(.terminal-shell-option > svg) { flex: none; color: var(--brand-icon); }
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
	@container (max-width: 720px) {
		.terminal-toolbar {
			min-height: 44px;
			gap: var(--space-2);
			padding: var(--space-2);
		}
		.shell-controls { flex: 1; flex-wrap: wrap; justify-content: flex-end; }
		:global(.terminal-shell-trigger) { width: min(180px, 40vw); }
		.shell-pending { display: none; }
		.terminal-surface { margin: var(--space-2); padding: var(--space-2); border-radius: var(--radius-md); }
	}
</style>

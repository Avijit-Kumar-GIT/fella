<script lang="ts">
	import { GENERAL_WORKSPACE_ID, session } from '$lib/session.svelte';
	import { Button } from '$lib/components/ui/button';
	import { isDesktop, win } from '$lib/ipc';
	import EnvironmentSwitcher from './EnvironmentSwitcher.svelte';
	import Icon from './Icon.svelte';
	import Logo from './Logo.svelte';

	let {
		onpalette,
		onenvironmentselect,
		onnewenvironment,
		oncloseenvironment
	}: {
		onpalette: () => void;
		onenvironmentselect: (id: string) => void | Promise<void>;
		onnewenvironment: () => void;
		oncloseenvironment: (id: string) => void | Promise<void>;
	} = $props();

	let folder = $derived.by(() => {
		const workspace = session.activeWorkspace;
		if (!workspace) return '';
		if (workspace.id === GENERAL_WORKSPACE_ID) return 'General';
		return workspace.path?.replace(/[/\\]+$/, '').replace(/^.*[/\\]/, '') ?? 'Workspace';
	});

	let displayTitle = $derived.by(() => {
		if (session.workspaceView === 'board') return folder || 'Workspaces';
		if (session.workspaceView === 'workspace') {
			return `${folder || 'Workspace'} · ${session.workspacePane === 'sources' ? 'Sources' : 'Guide'}`;
		}
		if (session.workspaceView === 'project') return session.activeProject?.name ?? 'Project';
		if (session.workspaceView === 'settings') return 'Fella';
		return folder || 'Ask';
	});


	// macOS keeps its native traffic lights (titleBarStyle: Overlay), so leave a
	// gutter for them. Windows/Linux draw nothing on the left.
	const isMac =
		typeof navigator !== 'undefined' && /Mac/i.test(navigator.platform || navigator.userAgent);
	// Windows is frameless (decorations:false) and needs our own controls; Linux
	// keeps its native frame, so it doesn't.
	const isWindows =
		typeof navigator !== 'undefined' && /Win/i.test(navigator.platform || navigator.userAgent);
	const shortcutModifier = isMac ? '⌘' : 'Ctrl';

</script>

<div class="titlebar" class:mac={isMac} class:focus={session.focus} class:collapsed={!session.focus && session.sidebarCollapsed}>
	{#if isMac}<span class="lights" aria-hidden="true"></span>{/if}

	{#if !session.focus && session.sidebarCollapsed}
		<span class="logo"><Logo size={18} active={session.busy} /></span>
		<Button
			variant="ghost"
			size="icon"
			class="navbtn"
			aria-expanded={!session.sidebarCollapsed}
			title={`Expand sidebar (${shortcutModifier}+B)`}
			onclick={() => session.toggleSidebar()}
		>
			<Icon name="panel" size={16} />
		</Button>
	{/if}

	{#if session.focus}
		<span class="spacer"></span>
		<span class="folder faint" title={displayTitle}>{displayTitle}</span>
		<span class="spacer"></span>
	{:else}
		<span class="id"><span class="folder" title={displayTitle}>{displayTitle}</span></span>
		<span class="spacer"></span>
		{#if session.sidebarCollapsed}
			<EnvironmentSwitcher
				compact
				onselect={onenvironmentselect}
				onnew={onnewenvironment}
				onclose={oncloseenvironment}
			/>
		{/if}

		<Button
			variant="ghost"
			size="sm"
			class="hint"
			onclick={onpalette}
			title={`Search Fella (${shortcutModifier}+K or ${shortcutModifier}+Shift+P)`}
		>
			<kbd>{shortcutModifier}</kbd><kbd>K</kbd>
		</Button>
	{/if}

	{#if isWindows && isDesktop()}
		<div class="winctl">
			<button aria-label="Minimize" onclick={() => void win.minimize()}>
				<Icon name="minus" size={16} />
			</button>
			<button aria-label="Maximize" onclick={() => void win.toggleMaximize()}>
				<Icon name="square" size={12} />
			</button>
			<button class="x" aria-label="Close" onclick={() => void win.close()}>
				<Icon name="x" size={16} />
			</button>
		</div>
	{/if}
</div>

<style>
	.titlebar {
		flex: none;
		display: flex;
		align-items: center;
		gap: var(--space-2);
		height: 42px;
		padding: 0 var(--space-2) 0 var(--pad);
		background: var(--app-chrome);
		color: var(--text-faint);
		font-size: var(--fs-sm);
		user-select: none;
		white-space: nowrap;
		-webkit-app-region: drag;
	}
	.titlebar :global(button) {
		-webkit-app-region: no-drag;
	}
	.titlebar.mac {
		padding-left: 0;
	}
	.titlebar.collapsed {
		padding-left: var(--space-2);
	}
	.lights {
		flex: none;
		width: 78px;
	}
	.logo {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 26px;
		height: 26px;
		flex: none;
	}
	:global(.titlebar .navbtn) {
		flex: none;
		display: grid;
		place-items: center;
		width: 24px;
		min-width: 24px;
		height: 24px;
		min-height: 24px;
		padding: 0;
		border-radius: var(--radius-chip);
		background: transparent;
		color: var(--text-faint);
		transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
	}
	:global(.titlebar .navbtn:hover:not(:disabled)) {
		background: var(--bg-inset);
		color: var(--text-dim);
	}
	:global(.titlebar .navbtn:disabled) {
		color: var(--border-strong);
		cursor: default;
	}
	.id {
		display: flex;
		align-items: baseline;
		gap: var(--space-2);
		min-width: 0;
		max-width: min(27vw, 280px);
	}
	.folder {
		font-family: var(--sans);
		font-size: var(--fs-sm);
		font-weight: 550;
		color: var(--text);
		overflow: hidden;
		text-overflow: ellipsis;
		min-width: 0;
	}
	.titlebar:not(.focus) .id { flex: none; }
	.folder.faint {
		color: var(--text-faint);
	}
	.spacer {
		flex: 1;
		align-self: stretch;
	}
	:global(.titlebar .hint) {
		display: inline-flex;
		align-items: center;
		gap: 2px;
		height: auto;
		min-height: 0;
		padding: 2px 6px;
		border-radius: var(--radius-chip);
		background: transparent;
		color: var(--text-faint);
		font-size: var(--fs-sm);
		transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
	}
	:global(.titlebar .hint:hover) {
		background: var(--bg-inset);
		color: var(--text-dim);
	}
	.winctl {
		display: flex;
		align-self: stretch;
	}
	.winctl button {
		width: 42px;
		display: grid;
		place-items: center;
		color: var(--text-faint);
		transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
	}
	.winctl button:hover {
		background: var(--bg-inset);
		color: var(--text);
	}
	.winctl button.x:hover {
		background: var(--err);
		color: var(--bg-raised);
	}
</style>

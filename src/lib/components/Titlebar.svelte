<script lang="ts">
	import { session } from '$lib/session.svelte';
	import { Button } from '$lib/components/ui/button';
	import { isDesktop, win } from '$lib/ipc';
	import WorkspaceSwitcher from './WorkspaceSwitcher.svelte';
	import WorkspaceContextBar from './WorkspaceContextBar.svelte';
	import Icon from './Icon.svelte';
	import Logo from './Logo.svelte';

	let {
		onpalette,
		onworkspaceselect,
		expanded = false,
		compact = false,
		ontoggleexpand
	}: {
		onpalette: () => void;
		onworkspaceselect: (id: string) => void | Promise<void>;
		expanded?: boolean;
		compact?: boolean;
		ontoggleexpand?: () => void;
	} = $props();

	let displayTitle = $derived.by(() => {
		if (session.workspaceView === 'project') return session.activeProject?.name ?? 'Project';
		if (session.workspaceView === 'settings') return 'Settings';
		return session.activeChat.title?.trim() || session.investigationNames[session.activeChat.id] || (session.sidebarCollapsed ? 'Fella' : '');
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
		<div class="titlebar-workspace">
			<WorkspaceSwitcher
				compact
				onselect={onworkspaceselect}
			/>
		</div>
		<span class="spacer"></span>
		<div class="titlebar-actions" role="group" aria-label="Workspace controls">
			<Button
				variant="ghost"
				size="icon"
				class="toolbar-icon"
				onclick={onpalette}
				aria-label="Search Fella"
				title={`Search Fella (${shortcutModifier}+K or ${shortcutModifier}+Shift+P)`}
			>
				<Icon name="search" size={16} />
			</Button>
			<WorkspaceContextBar {expanded} {compact} {ontoggleexpand} />
		</div>
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
	.titlebar-workspace { display:flex; flex:none; align-items:center; min-width:0; }
	.titlebar-workspace :global(.workspace-switcher.compact) { width:clamp(132px, 19vw, 240px); }
	.titlebar-actions { display:flex; flex:none; align-items:center; gap:4px; }
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
	.folder {
		font-family: var(--sans);
		font-size: var(--fs-sm);
		font-weight: 550;
		color: var(--text);
		overflow: hidden;
		text-overflow: ellipsis;
		min-width: 0;
	}
	.folder.faint {
		color: var(--text-faint);
	}
	.spacer {
		flex: 1;
		align-self: stretch;
	}
	:global(.titlebar .toolbar-icon) {
		display: grid;
		place-items: center;
		width: 30px;
		min-width: 30px;
		height: 30px;
		min-height: 30px;
		padding: 0;
		border-radius: var(--radius-chip);
		background: transparent;
		color: var(--text-faint);
		transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
	}
	:global(.titlebar .toolbar-icon:hover) {
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
	@media (max-width: 900px) {
		.titlebar-workspace :global(.workspace-switcher.compact) { width:clamp(120px, 24vw, 190px); }
	}
	@media (max-width: 700px) {
		.titlebar { gap:4px; }
		.titlebar-workspace :global(.workspace-switcher.compact) { width:clamp(112px, 32vw, 150px); }
		.titlebar-actions { gap:1px; }
	}
</style>

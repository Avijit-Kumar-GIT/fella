<script lang="ts">
	import { firstActualQuestion, session } from '$lib/session.svelte';
	import { isDesktop, isElectron, win } from '$lib/ipc';
	import Icon from './Icon.svelte';
	import Logo from './Logo.svelte';
	import TabBar from './TabBar.svelte';

	let { onpalette }: { onpalette: () => void } = $props();

	let multiTab = $derived(session.tabs.length > 1);
	let folder = $derived(
		session.catalog.workspace?.replace(/[/\\]+$/, '').replace(/^.*[/\\]/, '') ?? ''
	);

	// --- the active conversation's own title: a custom name if renamed,
	// otherwise folder + its first message, not just the raw message -- the
	// folder is what actually distinguishes two similarly-phrased
	// conversations from each other.
	let conversationTitle = $derived.by(() => {
		if (session.activeChat?.title) return session.activeChat.title;
		const msgs = session.activeChat?.messages ?? [];
		const first = firstActualQuestion(msgs);
		if (!first) return folder || 'New conversation';
		const t = first.text.trim();
		const clipped = t.length > 60 ? t.slice(0, 60) + '…' : t;
		return folder ? `${folder} — ${clipped}` : clipped;
	});
	let displayTitle = $derived.by(() => {
		const prefix = folder || 'Workspace';
		if (session.workspaceView === 'workspace') return `${prefix} — Workspace`;
		if (session.workspaceView === 'project') return `${session.activeProject?.name ?? 'Project'} — Project`;
		if (session.workspaceView === 'settings') return `${prefix} — Settings`;
		return conversationTitle;
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

<div class="titlebar" class:mac={isMac} class:focus={session.focus} class:collapsed={!session.focus && session.sidebarCollapsed} data-tauri-drag-region>
	{#if isMac}<span class="lights" aria-hidden="true"></span>{/if}

	{#if !session.focus && session.sidebarCollapsed}
		<span class="logo"><Logo size={18} active={session.busy} /></span>
		<button
			class="navbtn"
			data-tauri-drag-region="false"
			aria-expanded={!session.sidebarCollapsed}
			title={`Expand sidebar (${shortcutModifier}+B)`}
			onclick={() => session.toggleSidebar()}
		>
			<Icon name="panel" size={16} />
		</button>
	{/if}

	{#if session.focus}
		<span class="spacer" data-tauri-drag-region></span>
		{#if folder}<span class="folder faint" title={session.catalog.workspace}>{folder}</span>{/if}
		<span class="spacer" data-tauri-drag-region></span>
	{:else}
		{#if multiTab}
			<TabBar />
		{:else}
			<span class="id" data-tauri-drag-region>
				{#if folder}
					<span class="folder" title={displayTitle}>{displayTitle}</span>
				{:else}
					<span class="wordmark">Fella</span>
				{/if}
			</span>
		{/if}

		<span class="spacer" data-tauri-drag-region></span>

		<button
			class="hint"
			data-tauri-drag-region="false"
			onclick={onpalette}
			title={`Search Fella (${shortcutModifier}+K or ${shortcutModifier}+Shift+P)`}
		>
			<kbd>{shortcutModifier}</kbd><kbd>K</kbd>
		</button>
	{/if}

	{#if (isWindows || (isElectron() && !isMac)) && isDesktop()}
		<div class="winctl" data-tauri-drag-region="false">
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
		height: 38px;
		padding: 0 var(--space-2) 0 var(--pad);
		background: var(--bg);
		color: var(--text-faint);
		font-size: var(--fs-sm);
		user-select: none;
		white-space: nowrap;
		-webkit-app-region: drag;
	}
	.titlebar button,
	.titlebar [data-tauri-drag-region='false'] {
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
	.navbtn {
		flex: none;
		display: grid;
		place-items: center;
		width: 24px;
		height: 24px;
		border-radius: var(--radius-chip);
		color: var(--text-faint);
		transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
	}
	.navbtn:hover:not(:disabled) {
		background: var(--bg-inset);
		color: var(--text-dim);
	}
	.navbtn:disabled {
		color: var(--border-strong);
		cursor: default;
	}
	.id {
		display: flex;
		align-items: baseline;
		gap: var(--space-2);
		min-width: 0;
	}
	.wordmark {
		color: var(--text-dim);
		font-weight: 600;
		letter-spacing: -0.02em;
	}
	.folder {
		font-family: var(--mono);
		font-size: var(--fs-xs);
		color: var(--text-dim);
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
	.hint {
		display: inline-flex;
		gap: 2px;
		padding: 2px 6px;
		color: var(--text-faint);
		border-radius: var(--radius-chip);
		transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
	}
	.hint:hover {
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

<script lang="ts">
	import { session } from '$lib/session.svelte';
	import { isTauri, win } from '$lib/ipc';
	import { hardFail } from '$lib/verify';
	import Icon from './Icon.svelte';
	import TabBar from './TabBar.svelte';

	let { onpalette }: { onpalette: () => void } = $props();

	let multiTab = $derived(session.tabs.length > 1);

	// --- back/forward: real tab-history nav, not a fake affordance -------
	let canGoBack = $derived(session.active > 0);
	let canGoForward = $derived(session.active < session.tabs.length - 1);
	function goBack() {
		if (canGoBack) session.active -= 1;
	}
	function goForward() {
		if (canGoForward) session.active += 1;
	}

	// --- the active conversation's own title, not the workspace name -----
	let conversationTitle = $derived.by(() => {
		const msgs = session.activeChat?.messages ?? [];
		const first = msgs.find((m) => m.text?.trim());
		if (!first) return 'New conversation';
		const t = first.text.trim();
		return t.length > 60 ? t.slice(0, 60) + '…' : t;
	});

	// --- info popover: message count, model, last answer's verification --
	let infoOpen = $state(false);
	let infoWrapEl: HTMLDivElement | undefined = $state();
	function onWindowClick(e: MouseEvent) {
		if (infoOpen && infoWrapEl && !infoWrapEl.contains(e.target as Node)) {
			infoOpen = false;
		}
	}
	let messageCount = $derived(session.activeChat?.messages.length ?? 0);
	let providerId = $derived(session.settings?.provider ?? 'ollama');
	let providerName = $derived(
		session.providers.find((p) => p.id === providerId)?.display ?? providerId
	);
	let lastAnswer = $derived.by(() => {
		const msgs = session.activeChat?.messages ?? [];
		for (let i = msgs.length - 1; i >= 0; i--) {
			if (msgs[i].answer) return msgs[i].answer;
		}
		return null;
	});
	let verifySummary = $derived.by(() => {
		if (!lastAnswer) return null;
		const fail = hardFail(lastAnswer.verification);
		if (fail) return `unconfirmed — ${fail}`;
		const n = lastAnswer.verification.length;
		return n ? `${n} check${n === 1 ? '' : 's'} passed` : null;
	});

	// macOS keeps its native traffic lights (titleBarStyle: Overlay), so leave a
	// gutter for them. Windows/Linux draw nothing on the left.
	const isMac =
		typeof navigator !== 'undefined' && /Mac/i.test(navigator.platform || navigator.userAgent);
	// Windows is frameless (decorations:false) and needs our own controls; Linux
	// keeps its native frame, so it doesn't.
	const isWindows =
		typeof navigator !== 'undefined' && /Win/i.test(navigator.platform || navigator.userAgent);

	let folder = $derived(
		session.catalog.workspace?.replace(/[/\\]+$/, '').replace(/^.*[/\\]/, '') ?? ''
	);
</script>

<svelte:window onclick={onWindowClick} />

<div class="titlebar" class:mac={isMac} class:focus={session.focus} data-tauri-drag-region>
	{#if isMac}<span class="lights" aria-hidden="true"></span>{/if}

	{#if !session.focus}
		<button
			class="sidebar-toggle"
			data-tauri-drag-region="false"
			aria-expanded={!session.sidebarCollapsed}
			title="Toggle sidebar (Ctrl+B)"
			onclick={() => session.toggleSidebar()}
		>
			<span class="caret" class:open={!session.sidebarCollapsed} aria-hidden="true">
				<Icon name="chevron-right" size={13} />
			</span>
		</button>
		<button
			class="navbtn"
			data-tauri-drag-region="false"
			disabled={!canGoBack}
			title="Previous tab"
			onclick={goBack}
		>
			<Icon name="chevron-right" size={13} />
		</button>
		<button
			class="navbtn fwd"
			data-tauri-drag-region="false"
			disabled={!canGoForward}
			title="Next tab"
			onclick={goForward}
		>
			<Icon name="chevron-right" size={13} />
		</button>
		<span class="divider" aria-hidden="true"></span>
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
					<span class="folder" title={conversationTitle}>{conversationTitle}</span>
				{:else}
					<span class="wordmark">Fella</span>
				{/if}
			</span>
		{/if}

		<span class="spacer" data-tauri-drag-region></span>

		<span class="divider" aria-hidden="true"></span>
		<div class="info-wrap" bind:this={infoWrapEl}>
			<button
				class="navbtn info-btn"
				data-tauri-drag-region="false"
				onclick={() => (infoOpen = !infoOpen)}
				title="Session info"
				aria-expanded={infoOpen}
			>
				<Icon name="info" size={14} />
			</button>
			{#if infoOpen}
				<div class="info-pop" role="dialog" aria-label="Session info">
					<p>{messageCount} message{messageCount === 1 ? '' : 's'}</p>
					{#if session.model}<p>{providerName}/{session.model}</p>{/if}
					{#if verifySummary}<p>{verifySummary}</p>{/if}
				</div>
			{/if}
		</div>
		<span class="divider" aria-hidden="true"></span>
		<button class="hint" data-tauri-drag-region="false" onclick={onpalette} title="Command palette">
			<kbd>Ctrl</kbd><kbd>K</kbd>
		</button>
	{/if}

	{#if isWindows && isTauri()}
		<div class="winctl" data-tauri-drag-region="false">
			<button aria-label="Minimize" onclick={() => void win.minimize()}>
				<Icon name="minus" size={14} />
			</button>
			<button aria-label="Maximize" onclick={() => void win.toggleMaximize()}>
				<Icon name="square" size={11} />
			</button>
			<button class="x" aria-label="Close" onclick={() => void win.close()}>
				<Icon name="x" size={14} />
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
	}
	.titlebar.mac {
		padding-left: 0;
	}
	.lights {
		flex: none;
		width: 78px;
	}
	.sidebar-toggle {
		flex: none;
		display: grid;
		place-items: center;
		width: 28px;
		height: 28px;
		border-radius: var(--radius-chip);
		color: var(--text-faint);
		transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
	}
	.sidebar-toggle:hover {
		background: var(--bg-inset);
		color: var(--text-dim);
	}
	.caret {
		display: inline-flex;
		transition: transform var(--dur-fast) var(--ease);
	}
	.caret.open {
		transform: rotate(90deg);
	}
	.divider {
		flex: none;
		align-self: stretch;
		margin: var(--space-1) 0;
		width: 1px;
		background: var(--border);
	}
	.navbtn {
		flex: none;
		display: grid;
		place-items: center;
		width: 24px;
		height: 24px;
		border-radius: var(--radius-chip);
		color: var(--text-faint);
		transform: rotate(180deg);
		transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
	}
	.navbtn.fwd,
	.navbtn.info-btn {
		transform: none;
	}
	.info-wrap {
		position: relative;
		display: flex;
	}
	.info-pop {
		position: absolute;
		top: calc(100% + var(--space-2));
		right: 0;
		z-index: 20;
		min-width: 180px;
		padding: var(--space-2) var(--space-3);
		background: var(--bg-raised);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		box-shadow: var(--shadow-pop);
		color: var(--text-dim);
		font-size: var(--fs-sm);
		white-space: normal;
	}
	.info-pop p {
		margin: 0;
		padding: 3px 0;
	}
	.info-pop p + p {
		border-top: 1px solid var(--border);
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
		font-weight: 560;
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

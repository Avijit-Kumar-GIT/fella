<script lang="ts">
	import { session } from '$lib/session.svelte';
	import { isTauri, win } from '$lib/ipc';
	import { prefs, type Appearance } from '$lib/prefs.svelte';
	import { answerStatus, hardFail } from '$lib/verify';
	import Icon, { type IconName } from './Icon.svelte';
	import Logo from './Logo.svelte';
	import TabBar from './TabBar.svelte';

	let { onpalette }: { onpalette: () => void } = $props();

	type AppearanceOption = { id: Appearance; label: string; icon: IconName };
	const APPEARANCE_OPTIONS: AppearanceOption[] = [
		{ id: 'system', label: 'System', icon: 'monitor' },
		{ id: 'light', label: 'Light', icon: 'sun' },
		{ id: 'dark', label: 'Dark', icon: 'moon' }
	];

	function appearanceLabel(mode: Appearance): string {
		return APPEARANCE_OPTIONS.find((option) => option.id === mode)?.label ?? 'System';
	}

	function appearanceIcon(mode: Appearance): IconName {
		return APPEARANCE_OPTIONS.find((option) => option.id === mode)?.icon ?? 'monitor';
	}

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
		const first = msgs.find((m) => m.text?.trim());
		if (!first) return folder || 'New conversation';
		const t = first.text.trim();
		const clipped = t.length > 60 ? t.slice(0, 60) + '…' : t;
		return folder ? `${folder} — ${clipped}` : clipped;
	});
	let displayTitle = $derived.by(() => {
		const prefix = folder || 'Workspace';
		if (session.workspaceView === 'packs') return `${prefix} — Packs`;
		if (session.workspaceView === 'sources') return `${prefix} — Sources`;
		if (session.workspaceView === 'analyses') return `${prefix} — Analyses`;
		if (session.workspaceView === 'context') return `${prefix} — Context`;
		return conversationTitle;
	});

	// --- info popover: message count, model, last answer's verification --
	let infoOpen = $state(false);
	let infoWrapEl: HTMLDivElement | undefined = $state();
	let appearanceOpen = $state(false);
	let appearanceWrapEl: HTMLDivElement | undefined = $state();
	function onWindowClick(e: MouseEvent) {
		if (infoOpen && infoWrapEl && !infoWrapEl.contains(e.target as Node)) {
			infoOpen = false;
		}
		if (appearanceOpen && appearanceWrapEl && !appearanceWrapEl.contains(e.target as Node)) {
			appearanceOpen = false;
		}
	}
	let messageCount = $derived(session.activeChat?.messages.length ?? 0);
	let providerId = $derived(session.settings?.provider ?? 'ollama-cloud');
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
		const status = answerStatus(lastAnswer);
		if (status === 'failed') {
			const fail = hardFail(lastAnswer.verification);
			return fail ? `failed — ${fail}` : 'failed';
		}
		if (status === 'insufficient_data') return 'insufficient data';
		const n = lastAnswer.verification.length;
		if (status === 'verified') return `verified · ${n} check${n === 1 ? '' : 's'}`;
		return `needs review · ${n} check${n === 1 ? '' : 's'}`;
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

<svelte:window onclick={onWindowClick} />

<div class="titlebar" class:mac={isMac} class:focus={session.focus} data-tauri-drag-region>
	{#if isMac}<span class="lights" aria-hidden="true"></span>{/if}

	{#if !session.focus}
		{#if session.sidebarCollapsed}
			<span class="logo"><Logo size={16} /></span>
		{/if}
		<button
			class="navbtn"
			data-tauri-drag-region="false"
			aria-expanded={!session.sidebarCollapsed}
			title={`Toggle sidebar (${shortcutModifier}+B)`}
			onclick={() => session.toggleSidebar()}
		>
			<Icon name="panel" size={14} />
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

		<div class="appearance-wrap" bind:this={appearanceWrapEl}>
			<button
				class="navbtn appearance-btn"
				data-tauri-drag-region="false"
				onclick={() => {
					appearanceOpen = !appearanceOpen;
					infoOpen = false;
				}}
				title={`Appearance: ${appearanceLabel(prefs.appearance)}`}
				aria-label={`Appearance: ${appearanceLabel(prefs.appearance)}`}
				aria-expanded={appearanceOpen}
				aria-haspopup="menu"
			>
				<Icon name={appearanceIcon(prefs.appearance)} size={14} />
			</button>
			{#if appearanceOpen}
				<div
					class="appearance-pop"
					data-tauri-drag-region="false"
					role="menu"
					aria-label="Appearance"
				>
					<p class="appearance-heading">Appearance</p>
					{#each APPEARANCE_OPTIONS as option (option.id)}
						<button
							class="appearance-option"
							class:selected={prefs.appearance === option.id}
							type="button"
							role="menuitemradio"
							aria-checked={prefs.appearance === option.id}
							onclick={() => {
								prefs.setAppearance(option.id);
								appearanceOpen = false;
							}}
						>
							<Icon name={option.icon} size={14} />
							<span>{option.label}</span>
							{#if prefs.appearance === option.id}
								<Icon name="check" size={13} />
							{/if}
						</button>
					{/each}
				</div>
			{/if}
		</div>

		<div class="info-wrap" bind:this={infoWrapEl}>
			<button
				class="navbtn info-btn"
				data-tauri-drag-region="false"
				onclick={() => {
					infoOpen = !infoOpen;
					appearanceOpen = false;
				}}
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
		<button class="hint" data-tauri-drag-region="false" onclick={onpalette} title="Command palette">
			<kbd>{shortcutModifier}</kbd><kbd>K</kbd>
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
	.logo {
		display: flex;
		align-items: center;
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
	.info-wrap {
		position: relative;
		display: flex;
	}
	.appearance-wrap {
		position: relative;
		display: flex;
	}
	.appearance-btn[aria-expanded='true'] {
		background: var(--bg-inset);
		color: var(--text-dim);
	}
	.appearance-pop {
		position: absolute;
		top: calc(100% + var(--space-2));
		right: 0;
		z-index: 20;
		width: 156px;
		padding: var(--space-2);
		background: var(--bg-raised);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		box-shadow: var(--shadow-pop);
	}
	.appearance-heading {
		margin: 2px var(--space-2) var(--space-1);
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.appearance-option {
		width: 100%;
		display: flex;
		align-items: center;
		gap: var(--space-2);
		padding: 6px var(--space-2);
		border: 0;
		border-radius: var(--radius-chip);
		background: transparent;
		color: var(--text-dim);
		font: inherit;
		font-size: var(--fs-sm);
		text-align: left;
		white-space: nowrap;
		cursor: pointer;
	}
	.appearance-option:hover,
	.appearance-option.selected {
		background: var(--bg-inset);
		color: var(--text);
	}
	.appearance-option :global(.icon:last-child) {
		margin-left: auto;
		color: var(--brand);
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

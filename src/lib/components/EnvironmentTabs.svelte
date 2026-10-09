<script lang="ts">
	import { session } from '$lib/session.svelte';
	import Icon from './Icon.svelte';
	import Logo from './Logo.svelte';

	let {
		onselect,
		onnew,
		onclose
	}: {
		onselect: (id: string) => void | Promise<void>;
		onnew: () => void;
		onclose: (id: string) => void | Promise<void>;
	} = $props();
	const shortcutModifier =
		typeof navigator !== 'undefined' && /Mac/i.test(navigator.platform || navigator.userAgent) ? '⌘' : 'Ctrl';
	let tablist = $state<HTMLDivElement>();

	$effect(() => {
		const activeId = session.activeEnvironmentId;
		const container = tablist;
		if (!activeId || !container) return;
		const activeTab = [...container.querySelectorAll<HTMLButtonElement>('[data-environment-id]')]
			.find((tab) => tab.dataset.environmentId === activeId);
		if (!activeTab) return;
		const listBounds = container.getBoundingClientRect();
		const tabBounds = activeTab.getBoundingClientRect();
		if (tabBounds.left < listBounds.left) container.scrollLeft -= listBounds.left - tabBounds.left;
		else if (tabBounds.right > listBounds.right) container.scrollLeft += tabBounds.right - listBounds.right;
	});

	function onTabKeydown(event: KeyboardEvent, index: number): void {
		if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return;
		event.preventDefault();
		const count = session.environments.length;
		if (!count) return;
		const next = (index + (event.key === 'ArrowRight' ? 1 : -1) + count) % count;
		void onselect(session.environments[next].id);
		queueMicrotask(() => document.querySelectorAll<HTMLButtonElement>('[role="tab"]')[next]?.focus());
	}
</script>

<div class="environment-tabs-row">
	<div class="environment-tabs" role="tablist" aria-label="Environments" bind:this={tablist}>
		{#each session.environments as environment, index (environment.id)}
			<div class="tab-entry" role="presentation">
				<button
					class="tab"
					class:active={environment.id === session.activeEnvironmentId}
					type="button"
					role="tab"
					data-environment-id={environment.id}
					aria-selected={environment.id === session.activeEnvironmentId}
					tabindex={environment.id === session.activeEnvironmentId ? 0 : -1}
					aria-label={`${session.environmentLabel(environment)} environment, ${environment.panes.length} workspace${environment.panes.length === 1 ? '' : 's'}`}
					title={`${session.environmentLabel(environment)} · ${environment.panes.length} workspace${environment.panes.length === 1 ? '' : 's'}`}
					onkeydown={(event) => onTabKeydown(event, index)}
					onclick={() => void onselect(environment.id)}
				>
					{#if environment.panes[0]?.workspaceId}
						<Icon name="repository" size={14} solid />
					{:else if environment.panes.length === 1}
						<Logo size={14} active={environment.panes.some((pane) => session.conversations.find((item) => item.id === pane.conversationId)?.busy)} />
					{:else}
						<Icon name="panel" size={14} />
					{/if}
					<span class="tab-title">{session.environmentLabel(environment)}</span>
					{#if environment.panes.length > 1}<span class="pane-count">{environment.panes.length}</span>{/if}
				</button>
				{#if session.environments.length > 1}
					<button
						class="tab-close"
						type="button"
						aria-label={`Close environment: ${session.environmentLabel(environment)}`}
						title={environment.panes.some((pane) => session.conversations.find((item) => item.id === pane.conversationId)?.busy) ? 'An analysis is running in this environment' : 'Close environment'}
						disabled={environment.panes.some((pane) => session.conversations.find((item) => item.id === pane.conversationId)?.busy)}
						onclick={() => void onclose(environment.id)}
					>
						<Icon name="x" size={14} />
					</button>
				{/if}
			</div>
		{/each}
	</div>
	<button class="new-tab" type="button" aria-label="New environment" title={`New environment (${shortcutModifier}+T)`} onclick={onnew}>
		<Icon name="plus" size={16} />
	</button>
</div>

<style>
	.environment-tabs-row {
		position: relative;
		flex: 1 1 0;
		width: 0;
		min-width: 0;
		display: flex;
		align-items: center;
		padding-inline: 8px;
		-webkit-app-region: no-drag;
	}
	.environment-tabs {
		position: absolute;
		left: 50%;
		transform: translateX(-50%);
		flex: none;
		width: max-content;
		max-width: min(680px, calc(100% - 64px));
		min-width: 0;
		display: flex;
		align-items: center;
		justify-content: flex-start;
		gap: 2px;
		padding: 2px;
		border: 1px solid var(--pane-edge);
		border-radius: 999px;
		background: color-mix(in srgb, var(--workspace-surface) 42%, var(--app-chrome));
		overflow-x: auto;
		scrollbar-width: none;
	}
	.environment-tabs::-webkit-scrollbar { display: none; }
	.tab-entry {
		position: relative;
		flex: 0 1 142px;
		min-width: 70px;
		max-width: 180px;
		display: flex;
		align-items: center;
		border-radius: 999px;
		color: var(--text-faint);
	}
	.tab-entry:hover { background: color-mix(in srgb, var(--text) 5%, transparent); color: var(--text-dim); }
	.tab-entry:has(.tab.active) { background: var(--workspace-surface); color: var(--text); }
	.tab {
		flex: 1;
		min-width: 0;
		height: 27px;
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 0 5px 0 10px;
		text-align: left;
		color: inherit;
		white-space: nowrap;
	}
	.tab-title { min-width: 0; flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--fs-xs); font-weight: 540; }
	.tab :global(svg), .tab :global(.logo) { flex: none; }
	.pane-count { flex: none; color: var(--text-faint); font-size: 10px; }
	.tab-close {
		flex: none;
		width: 18px;
		height: 18px;
		margin-right: 5px;
		display: grid;
		place-items: center;
		border-radius: 50%;
		color: var(--text-faint);
		opacity: 0;
	}
	.tab-entry:hover .tab-close, .tab-close:focus-visible { opacity: 1; }
	.tab-close:hover:not(:disabled) { background: var(--bg-inset); color: var(--text); }
	.tab-close:disabled { cursor: not-allowed; }
	.new-tab {
		flex: none;
		width: 27px;
		height: 27px;
		margin-left: auto;
		display: grid;
		place-items: center;
		border: 1px solid transparent;
		border-radius: 50%;
		color: var(--text-faint);
	}
	.new-tab:hover { background: var(--workspace-surface); border-color: var(--pane-edge); color: var(--text); }
	@media (max-width: 620px) {
		.environment-tabs { left: 0; transform: none; width: calc(100% - 40px); justify-content: flex-start; }
		.tab-entry { flex-basis: 116px; }
	}
</style>

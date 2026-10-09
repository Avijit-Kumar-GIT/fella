<script lang="ts">
	import { session } from '$lib/session.svelte';
	import { Button } from '$lib/components/ui/button';
	import { Tabs } from '$lib/components/ui/tabs';
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
	let tablist = $state<HTMLDivElement | null>(null);
	let keyboardNavigation = false;

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

	function selectEnvironment(id: string): void {
		const restoreTabFocus = keyboardNavigation;
		keyboardNavigation = false;
		const selection = onselect(id);
		if (!restoreTabFocus) return;
		void Promise.resolve(selection).then(() => {
			if (session.activeEnvironmentId !== id || !tablist) return;
			const activeTab = [...tablist.querySelectorAll<HTMLButtonElement>('[data-environment-id]')]
				.find((tab) => tab.dataset.environmentId === id);
			activeTab?.focus();
		});
	}

	function noteKeyboardNavigation(event: KeyboardEvent): void {
		if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
		keyboardNavigation = true;
		queueMicrotask(() => (keyboardNavigation = false));
	}
</script>

<div class="environment-tabs-row">
	<Tabs.Root value={session.activeEnvironmentId} onValueChange={selectEnvironment} class="environment-tabs-root">
		<Tabs.List class="fella-ui-tabs-list environment-tabs" aria-label="Environments" bind:ref={tablist}>
		{#each session.environments as environment (environment.id)}
			<div class="tab-entry" role="presentation">
				<Tabs.Trigger
					class="fella-ui-tabs-trigger tab"
					value={environment.id}
					data-environment-id={environment.id}
					onkeydown={noteKeyboardNavigation}
					aria-label={`${session.environmentLabel(environment)} environment, ${environment.panes.length} workspace${environment.panes.length === 1 ? '' : 's'}`}
					title={`${session.environmentLabel(environment)} · ${environment.panes.length} workspace${environment.panes.length === 1 ? '' : 's'}`}
				>
					{#if environment.panes[0]?.workspaceId}
						<Icon name="repository" size={14} />
					{:else if environment.panes.length === 1}
						<Logo size={14} active={environment.panes.some((pane) => session.conversations.find((item) => item.id === pane.conversationId)?.busy)} />
					{:else}
						<Icon name="panel" size={14} />
					{/if}
					<span class="tab-title">{session.environmentLabel(environment)}</span>
					{#if environment.panes.length > 1}<span class="pane-count">{environment.panes.length}</span>{/if}
				</Tabs.Trigger>
				{#if session.environments.length > 1}
					<Button
						variant="ghost"
						size="icon"
						class="tab-close"
						aria-label={`Close environment: ${session.environmentLabel(environment)}`}
						title={environment.panes.some((pane) => session.conversations.find((item) => item.id === pane.conversationId)?.busy) ? 'An analysis is running in this environment' : 'Close environment'}
						disabled={environment.panes.some((pane) => session.conversations.find((item) => item.id === pane.conversationId)?.busy)}
						onclick={() => void onclose(environment.id)}
					>
						<Icon name="x" size={14} />
					</Button>
				{/if}
			</div>
		{/each}
		</Tabs.List>
	</Tabs.Root>
	<Button variant="ghost" size="icon" class="new-tab" aria-label="New environment" title={`New environment (${shortcutModifier}+T)`} onclick={onnew}>
		<Icon name="plus" size={16} />
	</Button>
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
	:global(.environment-tabs-root) { display: contents; }
	:global(.environment-tabs) {
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
	:global(.environment-tabs)::-webkit-scrollbar { display: none; }
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
	.tab-entry:has(:global(.tab[data-state='active'])) { background: var(--workspace-surface); color: var(--text); }
	:global(.tab) {
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
	:global(.tab svg), :global(.tab .logo) { flex: none; }
	.pane-count { flex: none; color: var(--text-faint); font-size: 10px; }
	:global(.tab-close) {
		flex: none;
		width: 18px;
		min-width: 18px;
		height: 18px;
		min-height: 18px;
		margin-right: 5px;
		padding: 0;
		display: grid;
		place-items: center;
		border-radius: 50%;
		background: transparent;
		color: var(--text-faint);
		opacity: 0;
	}
	.tab-entry:hover :global(.tab-close),
	:global(.tab-close:focus-visible) { opacity: 1; }
	:global(.tab-close:hover:not(:disabled)) { background: var(--bg-inset); color: var(--text); }
	:global(.tab-close:disabled) { cursor: not-allowed; }
	:global(.environment-tabs-row .new-tab) {
		flex: none;
		width: 27px;
		min-width: 27px;
		height: 27px;
		min-height: 27px;
		margin-left: auto;
		padding: 0;
		display: grid;
		place-items: center;
		border: 1px solid transparent;
		border-radius: 50%;
		background: transparent;
		color: var(--text-faint);
	}
	:global(.environment-tabs-row .new-tab:hover) { background: var(--workspace-surface); border-color: var(--pane-edge); color: var(--text); }
	@media (max-width: 620px) {
		:global(.environment-tabs) { left: 0; transform: none; width: calc(100% - 40px); justify-content: flex-start; }
		.tab-entry { flex-basis: 116px; }
	}
</style>

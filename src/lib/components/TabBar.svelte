<script lang="ts">
	import { firstActualQuestion, session } from '$lib/session.svelte';
	import type { Tab } from '$lib/session.svelte';
	import Icon from './Icon.svelte';

	/** A chip-sized label: a custom name if renamed, or the first actual question. */
	function label(tab: Tab): string {
		if (tab.title) return tab.title;
		const first = firstActualQuestion(tab.messages);
		const t = first?.text.replace(/\s+/g, ' ').trim();
		if (!t) return 'New conversation';
		return t.length > 24 ? t.slice(0, 23) + '…' : t;
	}

	function owner(tab: Tab): string {
		const scope = tab.workspaceScope;
		if (!scope) return 'General';
		return scope.replace(/[/\\]+$/, '').split(/[/\\]/).at(-1) || scope;
	}

	function accessibleLabel(tab: Tab): string {
		return `${owner(tab)}: ${label(tab)}`;
	}

	function onKey(e: KeyboardEvent, i: number) {
		if (e.key === 'ArrowRight' || e.key === 'ArrowLeft') {
			e.preventDefault();
			const n = session.tabs.length;
			const next = (i + (e.key === 'ArrowRight' ? 1 : n - 1)) % n;
			const tablist = e.currentTarget instanceof HTMLElement
				? e.currentTarget.closest('[role="tablist"]')
				: null;
			session.activateTab(next);
			queueMicrotask(() => {
				tablist?.querySelectorAll<HTMLButtonElement>('[role="tab"]')[next]?.focus();
			});
		}
	}
</script>

<div class="tabs" role="tablist" aria-label="Open conversations">
	{#each session.tabs as tab, i (tab.id)}
		<div
			class="tab"
			class:active={i === session.active}
			role="presentation"
		>
			<button
				class="select"
				role="tab"
				aria-label={accessibleLabel(tab)}
				aria-selected={i === session.active}
				tabindex={i === session.active ? 0 : -1}
				title={`${owner(tab)} · ${label(tab)}`}
				onclick={() => session.activateTab(i)}
				onkeydown={(e) => onKey(e, i)}
			>
				{#if tab.busy}<span class="thinking" aria-hidden="true"></span>{/if}
				<span class="owner">{owner(tab)}</span>
				<span class="label">{label(tab)}</span>
			</button>
			<button
				class="close"
				aria-label={`Close conversation: ${accessibleLabel(tab)}`}
				title={`Close ${label(tab)}`}
				onclick={(e) => {
					e.stopPropagation();
					void session.closeTab(i);
				}}
			>
				<Icon name="x" size={12} />
			</button>
		</div>
	{/each}
	<button
		class="add"
		aria-label="New conversation"
		onclick={() => session.newTab()}
	>
		<Icon name="plus" size={16} />
	</button>
</div>

<style>
	.tabs {
		display: flex;
		align-items: center;
		gap: 2px;
		min-width: 0;
		overflow-x: auto;
		scrollbar-width: none;
		/* TabBar is rendered inside Electron's draggable titlebar. Keep the
		   whole strip interactive; Titlebar.svelte's scoped no-drag rule cannot
		   style elements rendered by this child component. */
		-webkit-app-region: no-drag;
	}
	.tabs::-webkit-scrollbar {
		display: none;
	}
	.tab {
		display: flex;
		align-items: center;
		gap: 0;
		max-width: min(29ch, 24vw);
		border-radius: var(--radius-sm);
		color: var(--text-faint);
		font-size: var(--fs-sm);
		white-space: nowrap;
		-webkit-app-region: no-drag;
		transition:
			background var(--dur-fast) var(--ease),
			color var(--dur-fast) var(--ease);
	}
	.tab:hover {
		color: var(--text-dim);
		background: var(--bg-inset);
	}
	.tab.active {
		color: var(--text);
		background: var(--bg-inset);
		box-shadow: inset 0 0 0 1px var(--border);
	}
	.select {
		min-width: 0;
		flex: 1;
		display: flex;
		align-items: center;
		gap: 7px;
		padding: 4px 0 4px 9px;
		color: inherit;
		text-align: left;
		font: inherit;
		cursor: pointer;
		-webkit-app-region: no-drag;
	}
	.owner {
		flex: none;
		max-width: 10ch;
		overflow: hidden;
		color: var(--text-faint);
		font-size: var(--fs-xs);
		font-weight: 600;
		text-overflow: ellipsis;
	}
	.label {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.close,
	.add {
		display: grid;
		place-items: center;
		border-radius: var(--radius-chip);
		color: var(--text-faint);
		transition:
			background var(--dur-fast) var(--ease),
			color var(--dur-fast) var(--ease),
			opacity var(--dur-fast) var(--ease);
		-webkit-app-region: no-drag;
	}
	.close {
		flex: none;
		width: 20px;
		height: 20px;
		margin-right: 4px;
		opacity: 0;
	}
	.tab:hover .close,
	.tab.active .close,
	.close:focus-visible {
		opacity: 1;
	}
	.select:focus-visible,
	.close:focus-visible,
	.add:focus-visible {
		outline: 2px solid var(--brand);
		outline-offset: 1px;
	}
	.close:hover {
		color: var(--text);
		background: var(--border);
	}
	.add {
		width: 22px;
		height: 22px;
	}
	.add:hover {
		color: var(--text);
		background: var(--bg-inset);
	}
</style>

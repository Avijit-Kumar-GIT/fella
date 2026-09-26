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

	function onKey(e: KeyboardEvent, i: number) {
		if (e.key === 'ArrowRight' || e.key === 'ArrowLeft') {
			e.preventDefault();
			const n = session.tabs.length;
			session.activateTab((i + (e.key === 'ArrowRight' ? 1 : n - 1)) % n);
		} else if (e.key === 'Enter' || e.key === ' ') {
			e.preventDefault();
			session.activateTab(i);
		}
	}
</script>

<div class="tabs" role="tablist" aria-label="Tabs">
	{#each session.tabs as tab, i (tab.id)}
		<div
			class="tab"
			class:active={i === session.active}
			role="tab"
			aria-selected={i === session.active}
			tabindex={i === session.active ? 0 : -1}
			onclick={() => session.activateTab(i)}
			onkeydown={(e) => onKey(e, i)}
			data-tauri-drag-region="false"
		>
			{#if tab.busy}
				<span class="thinking" aria-hidden="true"></span>
			{/if}
			<span class="label">{label(tab)}</span>
			<button
				class="close"
				aria-label="Close this conversation"
				tabindex="-1"
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
		data-tauri-drag-region="false"
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
		gap: var(--space-1);
		max-width: 20ch;
		padding: 3px var(--space-1) 3px var(--space-2);
		border-radius: var(--radius-chip);
		color: var(--text-faint);
		font-size: var(--fs-sm);
		cursor: pointer;
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
	}
	.label {
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
		width: 16px;
		height: 16px;
		opacity: 0;
	}
	.tab:hover .close,
	.tab.active .close,
	.close:focus-visible {
		opacity: 1;
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

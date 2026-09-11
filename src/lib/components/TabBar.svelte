<script lang="ts">
	import { session } from '$lib/session.svelte';
	import type { Tab } from '$lib/session.svelte';
	import Icon from './Icon.svelte';

	/** A chip-sized label: the augment's file, or a conversation's first line. */
	function label(tab: Tab): string {
		if (tab.kind === 'augment') return tab.file;
		const first = tab.messages.find((m) => m.role === 'user');
		const t = first?.text.replace(/\s+/g, ' ').trim();
		if (!t) return 'New conversation';
		return t.length > 24 ? t.slice(0, 23) + '…' : t;
	}

	function onKey(e: KeyboardEvent, i: number) {
		if (e.key === 'ArrowRight' || e.key === 'ArrowLeft') {
			e.preventDefault();
			const n = session.tabs.length;
			session.active = (i + (e.key === 'ArrowRight' ? 1 : n - 1)) % n;
		} else if (e.key === 'Enter' || e.key === ' ') {
			e.preventDefault();
			session.active = i;
		}
	}
</script>

<div class="tabs" role="tablist" aria-label="Conversations">
	{#each session.tabs as tab, i (tab.id)}
		<div
			class="tab"
			class:active={i === session.active}
			role="tab"
			aria-selected={i === session.active}
			tabindex={i === session.active ? 0 : -1}
			onclick={() => (session.active = i)}
			onkeydown={(e) => onKey(e, i)}
			data-tauri-drag-region="false"
		>
			{#if tab.kind === 'chat' && tab.busy}
				<span class="thinking" aria-hidden="true"></span>
			{:else if tab.kind === 'augment' && tab.dirty}
				<span class="unsaved" aria-hidden="true" title="unsaved changes"></span>
			{/if}
			<span class="label">{label(tab)}</span>
			<button
				class="close"
				aria-label={tab.kind === 'augment' ? 'Close this tab' : 'Close this conversation'}
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
		<Icon name="plus" size={14} />
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
	.unsaved {
		width: 5px;
		height: 5px;
		border-radius: 50%;
		background: var(--warn);
		flex: none;
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

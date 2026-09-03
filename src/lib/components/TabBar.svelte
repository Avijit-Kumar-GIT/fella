<script lang="ts">
	import { session } from '$lib/session.svelte';
	import type { Conversation } from '$lib/session.svelte';
	import Icon from './Icon.svelte';

	/** First user line, trimmed to a chip-sized label. */
	function label(tab: Conversation): string {
		const first = tab.messages.find((m) => m.role === 'user');
		const t = first?.text.replace(/\s+/g, ' ').trim();
		if (!t) return 'New conversation';
		return t.length > 28 ? t.slice(0, 27) + '…' : t;
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

{#if session.tabs.length > 1}
	<div class="tabbar" role="tablist" aria-label="Conversations" data-tauri-drag-region>
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
					<Icon name="x" size={13} />
				</button>
			</div>
		{/each}
		<button
			class="add"
			aria-label="New conversation"
			data-tauri-drag-region="false"
			onclick={() => session.newTab()}
		>
			<Icon name="plus" size={15} />
		</button>
		<span class="filler" data-tauri-drag-region></span>
	</div>
{/if}

<style>
	.tabbar {
		flex: none;
		display: flex;
		align-items: flex-end;
		gap: var(--space-1);
		padding: var(--space-2) var(--pad) 0;
		background: var(--bg);
		overflow-x: auto;
		scrollbar-width: none;
	}
	.tabbar::-webkit-scrollbar {
		display: none;
	}
	.filler {
		flex: 1;
		align-self: stretch;
		min-width: var(--space-4);
	}
	.tab {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		max-width: 22ch;
		padding: var(--space-2) var(--space-1) var(--space-2) var(--space-3);
		border: 1px solid transparent;
		border-bottom: none;
		border-radius: var(--radius-sm) var(--radius-sm) 0 0;
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
		border-color: var(--border);
		background: var(--bg-raised);
		box-shadow: 0 -1px 3px rgba(20, 20, 26, 0.04);
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
	}
	.close {
		width: 18px;
		height: 18px;
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
		width: 24px;
		height: 24px;
		margin-bottom: var(--space-1);
	}
	.add:hover {
		color: var(--text);
		background: var(--bg-inset);
	}
</style>

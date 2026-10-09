<script lang="ts">
	import { firstActualQuestion, session } from '$lib/session.svelte';
	import Icon from './Icon.svelte';

	let {
		onselect,
		onnew,
		onclose
	}: {
		onselect: (index: number) => void | Promise<void>;
		onnew: () => void;
		onclose: (id: string) => void | Promise<void>;
	} = $props();
	const shortcutModifier =
		typeof navigator !== 'undefined' && /Mac/i.test(navigator.platform || navigator.userAgent) ? '⌘' : 'Ctrl';

	function title(conversation: (typeof session.conversations)[number]): string {
		return conversation.title?.trim() || firstActualQuestion(conversation.messages)?.text.trim() || 'New conversation';
	}

	function owner(conversation: (typeof session.conversations)[number]): string {
		const path = conversation.workspaceScope;
		return path ? path.replace(/[/\\]+$/, '').split(/[/\\]/).at(-1) || path : 'General';
	}

	function onTabKeydown(event: KeyboardEvent, index: number): void {
		if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return;
		event.preventDefault();
		const count = session.conversations.length;
		if (!count) return;
		const next = (index + (event.key === 'ArrowRight' ? 1 : -1) + count) % count;
		void onselect(next);
		queueMicrotask(() => document.querySelectorAll<HTMLButtonElement>('[role="tab"]')[next]?.focus());
	}
</script>

<div class="conversation-tabs-row">
	<div class="conversation-tabs" role="tablist" aria-label="Conversations">
		{#each session.conversations as conversation, index (conversation.id)}
			<div class="tab-entry" role="presentation">
				<button
					class="tab"
					class:active={index === session.activeConversationIndex}
					type="button"
					role="tab"
					aria-selected={index === session.activeConversationIndex}
					tabindex={index === session.activeConversationIndex ? 0 : -1}
					aria-label={`${title(conversation)} · ${owner(conversation)}`}
					title={`${title(conversation)} · ${owner(conversation)}`}
					onkeydown={(event) => onTabKeydown(event, index)}
					onclick={() => void onselect(index)}
				>
					<span class="tab-title">{title(conversation)}</span>
					<span class="tab-owner">{owner(conversation)}</span>
				</button>
				<button
					class="tab-close"
					type="button"
					aria-label={`Close tab: ${title(conversation)}`}
					title={conversation.busy ? 'Stop the run before closing this tab' : 'Close tab'}
					disabled={conversation.busy}
					onclick={() => void onclose(conversation.id)}
				>
					<Icon name="x" size={14} />
				</button>
			</div>
		{/each}
	</div>
	<button class="new-tab" type="button" aria-label="New conversation" title={`New conversation (${shortcutModifier}+T)`} onclick={onnew}>
		<Icon name="plus" size={16} />
	</button>
</div>

<style>
	.conversation-tabs-row {
		flex: none;
		display: flex;
		align-items: end;
		gap: 5px;
		min-width: 0;
		min-height: 39px;
		padding: 5px 10px 0;
		border-bottom: 1px solid var(--pane-edge);
		background: var(--workspace-surface);
	}
	.conversation-tabs {
		flex: 1;
		min-width: 0;
		display: flex;
		align-items: end;
		gap: 3px;
		overflow-x: auto;
		scrollbar-width: none;
	}
	.conversation-tabs::-webkit-scrollbar { display: none; }
	.tab-entry {
		position: relative;
		flex: 0 1 190px;
		min-width: 92px;
		max-width: 220px;
		display: flex;
		align-items: center;
		border-radius: 9px 9px 0 0;
		color: var(--text-faint);
	}
	.tab-entry:hover { background: var(--workspace-canvas); color: var(--text-dim); }
	.tab-entry:has(.tab.active) {
		background: var(--workspace-canvas);
		color: var(--text);
	}
	.tab {
		flex: 1;
		min-width: 0;
		min-height: 33px;
		display: flex;
		flex-direction: column;
		justify-content: center;
		gap: 1px;
		padding: 4px 6px 4px 10px;
		text-align: left;
		color: inherit;
	}
	.tab-title, .tab-owner {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.tab-title { font-size: var(--fs-xs); font-weight: 560; }
	.tab-owner { color: var(--text-faint); font-size: 10px; }
	.tab.active .tab-owner { color: var(--text-dim); }
	.tab-close {
		flex: none;
		width: 22px;
		height: 22px;
		margin-right: 4px;
		display: grid;
		place-items: center;
		border-radius: 6px;
		color: var(--text-faint);
		opacity: 0;
	}
	.tab-entry:hover .tab-close, .tab-close:focus-visible, .tab.active + .tab-close { opacity: 1; }
	.tab-close:hover:not(:disabled) { background: var(--pane-surface); color: var(--text); }
	.tab-close:disabled { cursor: not-allowed; }
	.new-tab {
		flex: none;
		width: 27px;
		height: 27px;
		margin: 0 0 4px;
		display: grid;
		place-items: center;
		border-radius: 7px;
		color: var(--text-faint);
	}
	.new-tab:hover { background: var(--workspace-canvas); color: var(--text); }
	@media (max-width: 620px) {
		.conversation-tabs-row { padding-inline: 5px; }
		.tab-entry { flex-basis: 150px; }
	}
</style>

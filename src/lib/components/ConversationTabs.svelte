<script lang="ts">
	import { firstActualQuestion, session } from '$lib/session.svelte';
	import Icon from './Icon.svelte';
	import Logo from './Logo.svelte';

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
					{#if conversation.workspaceScope}
						<Icon name="folder" size={14} solid />
					{:else}
						<Logo size={14} active={conversation.busy} />
					{/if}
					<span class="tab-title">{title(conversation)}</span>
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
		position: relative;
		flex: 1 1 0;
		width: 0;
		min-width: 0;
		display: flex;
		align-items: center;
		padding-inline: 8px;
		-webkit-app-region: no-drag;
	}
	.conversation-tabs {
		position: absolute;
		left: 50%;
		transform: translateX(-50%);
		flex: none;
		width: max-content;
		max-width: min(640px, calc(100% - 64px));
		min-width: 0;
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 2px;
		padding: 2px;
		border: 1px solid var(--pane-edge);
		border-radius: 999px;
		background: color-mix(in srgb, var(--workspace-surface) 42%, var(--app-chrome));
		overflow-x: auto;
		scrollbar-width: none;
	}
	.conversation-tabs::-webkit-scrollbar { display: none; }
	.tab-entry {
		position: relative;
		flex: 0 1 188px;
		min-width: 76px;
		max-width: 210px;
		display: flex;
		align-items: center;
		border-radius: 999px;
		color: var(--text-faint);
	}
	.tab-entry:hover { background: color-mix(in srgb, var(--text) 5%, transparent); color: var(--text-dim); }
	.tab-entry:has(.tab.active) {
		background: var(--workspace-surface);
		color: var(--text);
		box-shadow: var(--window-shadow);
	}
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
	.tab-title {
		min-width: 0;
		flex: 1;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-size: var(--fs-xs);
		font-weight: 540;
	}
	.tab :global(svg),
	.tab :global(.logo) { flex: none; }
	.tab-close {
		flex: none;
		width: 19px;
		height: 19px;
		margin-right: 5px;
		display: grid;
		place-items: center;
		border-radius: 50%;
		color: var(--text-faint);
		opacity: 0;
	}
	.tab-entry:hover .tab-close, .tab-close:focus-visible, .tab.active + .tab-close { opacity: 1; }
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
		.conversation-tabs { left: 0; transform: none; width: calc(100% - 40px); justify-content: flex-start; }
		.tab-entry { flex-basis: 150px; }
	}
</style>

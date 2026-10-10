<script lang="ts">
	import { openRepository } from '$lib/commands';
	import { Button } from '$lib/components/ui/button';
	import { firstActualQuestion, session } from '$lib/session.svelte';
	import { resolveWorkspaceTileLayout } from '$lib/workspace-layout';
	import type { WorkspaceTilePreference } from '$lib/workspace-layout';
	import type Transcript from './Transcript.svelte';
	import CompanionPane from './CompanionPane.svelte';
	import Icon from './Icon.svelte';
	import TranscriptView from './Transcript.svelte';

	let { transcript = $bindable<Transcript | undefined>() } = $props<{ transcript?: Transcript }>();
	let draggingWorkspace = $state(false);
	let dragPreference = $state<WorkspaceTilePreference>({ ...session.workspaceLayout });
	let dragTargetIndex = $state(0);
	let dragAddsPane = $state(false);

	let tiles = $derived(
		resolveWorkspaceTileLayout(session.activeEnvironmentPanes.map((pane) => pane.id), session.workspaceLayout)
	);
	let previewIds = $derived([
		...session.activeEnvironmentPanes.map((pane) => pane.id),
		...(dragAddsPane ? ['__fella-drop-workspace__'] : [])
	]);
	let previewTiles = $derived(
		draggingWorkspace && previewIds.length <= 4
			? resolveWorkspaceTileLayout(previewIds, dragPreference)
			: []
	);

	function folderName(path: string): string {
		return path.replace(/[/\\]+$/, '').split(/[/\\]/).at(-1) || path;
	}

	function workspaceName(paneId: string): string {
		const pane = session.paneAt(paneId);
		if (!pane?.workspaceId) return 'General';
		const workspace = session.workspaceAt(pane.workspaceId);
		return folderName(workspace?.path ?? pane.workspaceId);
	}

	function workspacePreview(paneId: string): string {
		const pane = session.paneAt(paneId);
		const conversation = pane && session.conversations.find((item) => item.id === pane.conversationId);
		return conversation?.title?.trim() || firstActualQuestion(conversation?.messages ?? [])?.text.trim() || 'Start a conversation';
	}

	function workspaceAnswerPreview(paneId: string): string {
		const pane = session.paneAt(paneId);
		const conversation = pane && session.conversations.find((item) => item.id === pane.conversationId);
		const answer = conversation?.messages.findLast(
			(message) => message.role === 'assistant' && !!message.text.trim() && !message.pending
		)?.text.trim();
		if (!answer) return '';
		return answer.length > 240 ? `${answer.slice(0, 237).trimEnd()}…` : answer;
	}

	function gridPlacement(rect: { x: number; y: number; width: number; height: number }): string {
		const column = Math.round(rect.x * 4) + 1;
		const row = Math.round(rect.y * 4) + 1;
		const columns = Math.round(rect.width * 4);
		const rows = Math.round(rect.height * 4);
		return `grid-column: ${column} / span ${columns}; grid-row: ${row} / span ${rows};`;
	}

	function placement(event: DragEvent, total: number): { preference: WorkspaceTilePreference; index: number } {
		const preference: WorkspaceTilePreference = { ...session.workspaceLayout };
		if (total > 4) return { preference, index: 0 };
		const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
		const x = Math.max(0, Math.min(1, (event.clientX - rect.left) / Math.max(1, rect.width)));
		const y = Math.max(0, Math.min(1, (event.clientY - rect.top) / Math.max(1, rect.height)));
		if (total === 2) {
			preference.two = Math.abs(x - 0.5) >= Math.abs(y - 0.5) ? 'side-by-side' : 'stacked';
		} else if (total === 3) {
			if (x < 0.22) preference.three = 'two-left-one-right';
			else if (x > 0.78) preference.three = 'one-left-two-right';
			else if (y < 0.22) preference.three = 'two-top-one-bottom';
			else if (y > 0.78) preference.three = 'one-top-two-bottom';
		}
		const ids = [...session.activeEnvironmentPanes.map((pane) => pane.id), ...(total > session.activeEnvironmentPanes.length ? ['__fella-drop-workspace__'] : [])];
		const slots = resolveWorkspaceTileLayout(ids, preference);
		let index = 0;
		let distance = Number.POSITIVE_INFINITY;
		for (const [slotIndex, slot] of slots.entries()) {
			const dx = x - (slot.x + slot.width / 2);
			const dy = y - (slot.y + slot.height / 2);
			const nextDistance = dx * dx + dy * dy;
			if (nextDistance < distance) {
				distance = nextDistance;
				index = slotIndex;
			}
		}
		return { preference, index };
	}

	function onDragOver(event: DragEvent): void {
		const transfer = event.dataTransfer;
		const paneDrag = transfer?.types.includes('application/x-fella-environment-pane') ?? false;
		const workspaceDrag = transfer?.types.includes('application/x-fella-workspace') ?? false;
		if (paneDrag || workspaceDrag) {
			event.preventDefault();
			dragAddsPane = workspaceDrag && !transfer?.types.includes('application/x-fella-existing-workspace');
			transfer!.dropEffect = paneDrag || !dragAddsPane ? 'move' : 'copy';
			const total = session.activeEnvironmentPanes.length + (dragAddsPane ? 1 : 0);
			const proposed = placement(event, total);
			dragPreference = proposed.preference;
			dragTargetIndex = proposed.index;
			draggingWorkspace = true;
		}
	}

	function onDragLeave(event: DragEvent): void {
		if (event.currentTarget === event.target && event.relatedTarget === null) {
			draggingWorkspace = false;
		}
	}

	async function onDrop(event: DragEvent): Promise<void> {
		const paneId = event.dataTransfer?.getData('application/x-fella-environment-pane');
		const path = event.dataTransfer?.getData('application/x-fella-workspace');
		if (!path && !paneId) return;
		event.preventDefault();
		event.stopPropagation();
		draggingWorkspace = false;
		const existingPane = paneId
			? session.activeEnvironmentPanes.find((pane) => pane.id === paneId)
			: session.paneForWorkspace(path ?? '');
		const total = session.activeEnvironmentPanes.length + (!existingPane && !paneId ? 1 : 0);
		if (total > 4) {
			session.addSystem('This environment already has four workspaces. Create or switch environments to add another.');
			return;
		}
		const proposed = placement(event, total);
		if (paneId && existingPane) {
			session.setWorkspaceLayout(proposed.preference);
			session.placeWorkspacePane(paneId, proposed.index);
			session.focusEnvironmentPane(paneId);
			return;
		}
		if (!path) return;
		if (existingPane) {
			session.setWorkspaceLayout(proposed.preference);
			session.placeWorkspacePane(existingPane.id, proposed.index);
			session.focusEnvironmentPane(existingPane.id);
			return;
		}
		const existingWorkspace = session.workspaceAt(path);
		const opened = existingWorkspace && !existingWorkspace.historyOnly
			? session.focusWorkspace(path)
			: await openRepository(path);
		if (opened) {
			session.setWorkspaceLayout(proposed.preference);
			const pane = session.paneForWorkspace(path);
			if (pane) session.placeWorkspacePane(pane.id, proposed.index);
		}
	}

	function startWorkspaceDrag(event: DragEvent, id: string): void {
		if (!event.dataTransfer) return;
		event.dataTransfer.effectAllowed = 'move';
		event.dataTransfer.setData('application/x-fella-environment-pane', id);
	}

	function closeWorkspace(event: MouseEvent, id: string): void {
		event.stopPropagation();
		void session.closeWorkspaceWindow(id);
	}
</script>

<section
	class="board"
	class:dragging={draggingWorkspace}
	aria-label="Workspace board"
	ondragover={onDragOver}
	ondragleave={onDragLeave}
	ondrop={(event) => void onDrop(event)}
>
	<div class="tile-grid" class:empty={tiles.length === 0}>
		{#each tiles as tile (tile.id)}
			{@const pane = session.paneAt(tile.id)}
			{@const workspace = pane?.workspaceId ? session.workspaceAt(pane.workspaceId) : null}
			{#if pane}
				<article
					class="workspace-tile"
					class:focused={session.activeEnvironment?.activePaneId === pane.id}
					style={gridPlacement(tile)}
					aria-label={`${workspaceName(pane.id)} workspace`}
				>
					<header
						class="tile-head"
						role="group"
						aria-label={`Drag ${workspaceName(pane.id)} to rearrange workspaces`}
						draggable="true"
						ondragstart={(event) => startWorkspaceDrag(event, pane.id)}
					>
						<Button
							variant="ghost"
							class="tile-focus"
							aria-pressed={session.activeEnvironment?.activePaneId === pane.id}
							title={workspace?.path ?? 'Conversations that are not tied to a folder'}
							onclick={() => session.focusEnvironmentPane(pane.id)}
						>
							<Icon name={workspace ? 'repository' : 'ask'} size={16} />
							<span>{workspaceName(pane.id)}</span>
						</Button>
						<Button variant="ghost" size="icon" class="tile-close" aria-label={`Close ${workspaceName(pane.id)} workspace`} title="Remove from this environment" onclick={(event) => closeWorkspace(event, pane.id)}>
							<Icon name="x" size={14} />
						</Button>
					</header>
					{#if session.activeEnvironment?.activePaneId === pane.id}
						<div class="tile-chat">
							<TranscriptView bind:this={transcript} />
							{#if session.activeChat.companionPane}<CompanionPane />{/if}
						</div>
					{:else}
						{@const answerPreview = workspaceAnswerPreview(pane.id)}
						<Button
							variant="ghost"
							class="tile-preview"
							title={workspacePreview(pane.id)}
							aria-label={`Focus ${workspaceName(pane.id)}: ${workspacePreview(pane.id)}${
								answerPreview ? `. Latest reply: ${answerPreview}` : ''
							}`}
							onclick={() => session.focusEnvironmentPane(pane.id)}
						>
							<span class="preview-label">Recent conversation</span>
							<strong>{workspacePreview(pane.id)}</strong>
							{#if answerPreview}<span class="preview-answer">{answerPreview}</span>{/if}
						</Button>
					{/if}
				</article>
			{/if}
		{/each}
		{#if tiles.length === 0}
			<div class="empty-board"><p>This environment is empty.</p></div>
		{/if}
	</div>
	{#if draggingWorkspace}
		<div class="placement-preview" aria-live="polite">
			{#if previewTiles.length}
				{#each previewTiles as preview, index (preview.id)}
					<div class="preview-slot" class:target={index === dragTargetIndex} style={gridPlacement(preview)}>
						{index === dragTargetIndex ? 'Drop to place' : preview.id === '__fella-drop-workspace__' ? 'Workspace' : workspaceName(preview.id)}
					</div>
				{/each}
			{:else}
				<div class="full-notice">Close a workspace to add another.</div>
			{/if}
		</div>
	{/if}
</section>

<style>
	.board {
		position: relative;
		flex: 1;
		min-width: 0;
		min-height: 0;
		display: flex;
		flex-direction: column;
		padding: 14px;
		background: var(--workspace-canvas);
	}
	.tile-grid {
		position: relative;
		flex: 1;
		min-width: 0;
		min-height: 0;
		display: grid;
		grid-template-columns: repeat(4, minmax(0, 1fr));
		grid-template-rows: repeat(4, minmax(0, 1fr));
		gap: 9px;
	}
	.tile-grid.empty { place-items: center; }
	.workspace-tile {
		min-width: 0;
		min-height: 0;
		display: flex;
		flex-direction: column;
		overflow: hidden;
		border: 1px solid var(--pane-edge);
		border-radius: var(--radius-window);
		background: var(--pane-surface);
		box-shadow: none;
		transition: border-color 150ms ease, background-color 150ms ease;
	}
	.workspace-tile:not(.focused) {
		background: color-mix(in srgb, var(--pane-surface) 82%, transparent);
	}
	.workspace-tile.focused {
		border: 2px solid var(--brand);
		background: var(--pane-surface);
	}
	.tile-head {
		flex: none;
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 6px;
		min-height: 39px;
		padding: 0 9px 0 12px;
		border-bottom: 1px solid var(--pane-edge);
		background: var(--pane-head);
		cursor: grab;
	}
	.workspace-tile.focused .tile-head {
		background: var(--pane-head);
	}
	.workspace-tile:not(.focused) .tile-head {
		background: color-mix(in srgb, var(--pane-head) 82%, transparent);
	}
	.tile-head:active { cursor: grabbing; }
	.tile-head :global(.tile-focus) {
		min-width: 0;
		display: flex;
		align-items: center;
		gap: 7px;
		height: auto;
		min-height: 0;
		justify-content: flex-start;
		padding: 0;
		border-radius: 0;
		background: transparent;
		color: var(--text-dim);
		text-align: left;
		font-size: var(--fs-sm);
		font-weight: 550;
		white-space: normal;
	}
	.workspace-tile.focused .tile-head :global(.tile-focus) { color: var(--text); }
	.tile-head :global(.tile-focus > span) { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.tile-head :global(.tile-focus > svg) { flex: none; color: var(--text-faint); }
	.workspace-tile.focused .tile-head :global(.tile-focus > svg) { color: var(--brand-icon); }
	.tile-head :global(.tile-close) {
		flex: none;
		display: grid;
		place-items: center;
		width: 26px;
		min-width: 26px;
		height: 26px;
		min-height: 26px;
		padding: 0;
		border-radius: var(--radius-sm);
		background: transparent;
		color: var(--text-faint);
	}
	.tile-head :global(.tile-close:hover) { background: var(--bg-inset); color: var(--text); }
	.tile-chat { flex: 1; min-width: 0; min-height: 0; display: flex; }
	.tile-chat :global(.transcript) { flex: 1; min-width: 0; min-height: 0; padding: 16px; }
	.workspace-tile :global(.tile-preview) {
		flex: 1;
		width: 100%;
		height: auto;
		min-height: 0;
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		justify-content: flex-start;
		gap: 8px;
		padding: 18px;
		border-radius: 0;
		background: transparent;
		color: var(--text-faint);
		text-align: left;
		white-space: normal;
	}
	.workspace-tile :global(.tile-preview:hover) { background: color-mix(in srgb, var(--bg-inset) 36%, transparent); color: var(--text-dim); }
	.workspace-tile :global(.tile-preview .preview-label) { color: var(--text-faint); font-size: var(--fs-xs); }
	.workspace-tile :global(.tile-preview strong) { width: 100%; overflow: hidden; color: var(--text-dim); font-size: var(--fs-sm); font-weight: 520; line-height: 1.5; text-overflow: ellipsis; }
	.workspace-tile :global(.tile-preview .preview-answer) {
		display: -webkit-box;
		width: 100%;
		overflow: hidden;
		color: var(--text-dim);
		font-size: var(--fs-sm);
		line-height: 1.5;
		line-clamp: 3;
		-webkit-box-orient: vertical;
		-webkit-line-clamp: 3;
	}
	.empty-board { display: grid; justify-items: center; padding: 24px; text-align: center; }
	.empty-board p { margin: 0; color: var(--text-faint); font-size: var(--fs-sm); }
	.placement-preview {
		position: absolute;
		inset: 14px;
		z-index: 4;
		display: grid;
		grid-template-columns: repeat(4, minmax(0, 1fr));
		grid-template-rows: repeat(4, minmax(0, 1fr));
		gap: 9px;
		pointer-events: none;
	}
	.preview-slot {
		display: grid;
		place-items: center;
		min-width: 0;
		border: 1px dashed var(--border-strong);
		border-radius: var(--radius-window);
		background: color-mix(in srgb, var(--bg) 82%, transparent);
		color: var(--text-faint);
		font-size: var(--fs-xs);
		font-weight: 600;
		text-align: center;
	}
	.preview-slot.target {
		border: 2px dashed var(--brand);
		background: color-mix(in srgb, var(--brand) 10%, var(--bg-raised));
		color: var(--brand);
	}
	.full-notice {
		grid-column: 1 / -1;
		grid-row: 1 / -1;
		display: grid;
		place-items: center;
		border: 1px solid var(--border);
		border-radius: var(--radius-window);
		background: color-mix(in srgb, var(--bg) 82%, transparent);
		color: var(--text-dim);
		font-size: var(--fs-xs);
	}
	.board.dragging .workspace-tile { opacity: .62; }
	@media (prefers-reduced-motion: reduce) {
		.workspace-tile { transition: none; }
	}
	@media (max-width: 860px) {
		.board { padding: 8px; }
		.workspace-tile { border-radius: var(--radius); }
	}
</style>

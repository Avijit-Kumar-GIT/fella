<script lang="ts">
	import { openFolder, openRepository } from '$lib/commands';
	import { firstActualQuestion, session } from '$lib/session.svelte';
	import { resolveWorkspaceTileLayout, type ThreeTileLayout, type TwoTileLayout } from '$lib/workspace-layout';
	import type Transcript from './Transcript.svelte';
	import CompanionPane from './CompanionPane.svelte';
	import Icon from './Icon.svelte';
	import TranscriptView from './Transcript.svelte';

	let { transcript = $bindable<Transcript | undefined>() } = $props<{ transcript?: Transcript }>();
	let draggingWorkspace = $state(false);

	let tiles = $derived(
		resolveWorkspaceTileLayout(session.workspaceWindows.map((workspace) => workspace.id), session.workspaceLayout)
	);
	let previewTiles = $derived(
		session.workspaceWindows.length < 4
			? resolveWorkspaceTileLayout(
					[...session.workspaceWindows.map((workspace) => workspace.id), '__fella-new-workspace__'],
					session.workspaceLayout
				)
			: []
	);

	function folderName(path: string): string {
		return path.replace(/[/\\]+$/, '').split(/[/\\]/).at(-1) || path;
	}

	function workspaceName(workspace: NonNullable<ReturnType<typeof session.workspaceAt>>): string {
		return workspace.kind === 'general' ? 'General' : folderName(workspace.path ?? 'Workspace');
	}

	function workspacePreview(workspace: NonNullable<ReturnType<typeof session.workspaceAt>>): string {
		const conversation = session.conversations.find((item) => item.id === workspace.conversationId);
		return conversation?.title?.trim() || firstActualQuestion(conversation?.messages ?? [])?.text.trim() || 'New conversation';
	}

	function gridPlacement(rect: { x: number; y: number; width: number; height: number }): string {
		const column = Math.round(rect.x * 4) + 1;
		const row = Math.round(rect.y * 4) + 1;
		const columns = Math.round(rect.width * 4);
		const rows = Math.round(rect.height * 4);
		return `grid-column: ${column} / span ${columns}; grid-row: ${row} / span ${rows};`;
	}

	function chooseTwoLayout(event: Event): void {
		const two = (event.currentTarget as HTMLSelectElement).value as TwoTileLayout;
		session.setWorkspaceLayout({ ...session.workspaceLayout, two });
	}

	function chooseThreeLayout(event: Event): void {
		const three = (event.currentTarget as HTMLSelectElement).value as ThreeTileLayout;
		session.setWorkspaceLayout({ ...session.workspaceLayout, three });
	}

	function onDragOver(event: DragEvent): void {
		if (event.dataTransfer?.types.includes('application/x-fella-workspace')) {
			event.preventDefault();
			event.dataTransfer.dropEffect = 'copy';
			draggingWorkspace = true;
		}
	}

	function onDragLeave(event: DragEvent): void {
		if (event.currentTarget === event.target && event.relatedTarget === null) {
			draggingWorkspace = false;
		}
	}

	async function onDrop(event: DragEvent): Promise<void> {
		const path = event.dataTransfer?.getData('application/x-fella-workspace');
		if (!path) return;
		event.preventDefault();
		event.stopPropagation();
		draggingWorkspace = false;
		if (session.workspaceAt(path)) session.focusWorkspace(path);
		else await openRepository(path);
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
	<header class="board-head">
		<div class="board-actions">
			{#if tiles.length === 2}
				<label class="layout-select">
					<span class="sr-only">Workspace arrangement</span>
					<select aria-label="Workspace arrangement" value={session.workspaceLayout.two ?? 'side-by-side'} onchange={chooseTwoLayout}>
						<option value="side-by-side">Side by side</option>
						<option value="stacked">Stacked</option>
					</select>
				</label>
			{:else if tiles.length === 3}
				<label class="layout-select">
					<span class="sr-only">Workspace arrangement</span>
					<select aria-label="Workspace arrangement" value={session.workspaceLayout.three ?? 'two-top-one-bottom'} onchange={chooseThreeLayout}>
						<option value="two-top-one-bottom">Two top · one bottom</option>
						<option value="one-top-two-bottom">One top · two bottom</option>
						<option value="two-left-one-right">Two left · one right</option>
						<option value="one-left-two-right">One left · two right</option>
					</select>
				</label>
			{/if}
			{#if session.sidebarCollapsed || session.focus}
				<button class="add" type="button" aria-label="Add repository" title="Add repository" disabled={session.workspaceWindows.length >= 4} onclick={() => void openFolder()}>
					<Icon name="plus" size={16} />
				</button>
			{/if}
		</div>
	</header>

	<div class="tile-grid" class:empty={tiles.length === 0}>
		{#each tiles as tile (tile.id)}
			{@const workspace = session.workspaceAt(tile.id)}
			{#if workspace}
				<article
					class="workspace-tile"
					class:focused={session.activeWorkspaceId === workspace.id}
					style={gridPlacement(tile)}
					aria-label={`${workspaceName(workspace)} workspace`}
				>
					<header class="tile-head">
						<button
							class="tile-focus"
							type="button"
							aria-pressed={session.activeWorkspaceId === workspace.id}
							title={workspace.path ?? 'Conversations that are not tied to a folder'}
							onclick={() => session.focusWorkspace(workspace.id)}
						>
							<Icon name={workspace.kind === 'general' ? 'ask' : 'repository'} size={16} solid />
							<span>{workspaceName(workspace)}</span>
						</button>
						<button class="tile-close" type="button" aria-label={`Close ${workspaceName(workspace)} workspace`} title="Close workspace" onclick={(event) => closeWorkspace(event, workspace.id)}>
							<Icon name="x" size={14} />
						</button>
					</header>
					{#if session.activeWorkspaceId === workspace.id}
						<div class="tile-chat">
							<TranscriptView bind:this={transcript} />
							{#if session.activeChat.companionPane}<CompanionPane />{/if}
						</div>
					{:else}
						<button
							class="tile-preview"
							type="button"
							title={workspacePreview(workspace)}
							aria-label={`Focus ${workspaceName(workspace)}: ${workspacePreview(workspace)}`}
							onclick={() => session.focusWorkspace(workspace.id)}
						>
							<span>{workspacePreview(workspace)}</span>
						</button>
					{/if}
				</article>
			{/if}
		{/each}
		{#if tiles.length === 0}
			<div class="empty-board">
				<p>No open workspaces</p>
				<button class="empty-action" type="button" onclick={() => session.openGeneralWorkspace()}>Open General</button>
			</div>
		{/if}
	</div>
	{#if draggingWorkspace}
		<div class="placement-preview" aria-live="polite">
			{#if previewTiles.length}
				{#each previewTiles as preview (preview.id)}
					<div class="preview-slot" class:target={preview.id === '__fella-new-workspace__'} style={gridPlacement(preview)}>
						{preview.id === '__fella-new-workspace__' ? 'New workspace' : workspaceName(session.workspaceAt(preview.id)!)}
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
	.board-head {
		flex: none;
		display: flex;
		align-items: center;
		justify-content: flex-end;
		gap: 14px;
		min-height: 34px;
		padding: 0 2px 8px;
	}
	.board-actions { display: flex; align-items: center; gap: 8px; }
	.layout-select select {
		max-width: 190px;
		min-height: 30px;
		padding: 0 26px 0 9px;
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		background: var(--bg-raised);
		color: var(--text-dim);
		font: inherit;
		font-size: var(--fs-xs);
	}
	.add {
		display: grid;
		align-items: center;
		justify-content: center;
		width: 30px;
		height: 30px;
		border: 1px solid var(--border);
		border-radius: 50%;
		background: var(--bg-raised);
		color: var(--text-dim);
	}
	.add:hover:not(:disabled) { border-color: var(--border-strong); color: var(--text); }
	.add:disabled { opacity: .48; cursor: not-allowed; }
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
		box-shadow: var(--window-shadow);
		transition: border-color 120ms ease, background 120ms ease;
	}
	.workspace-tile.focused {
		border-color: var(--pane-edge-active);
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
	}
	.tile-focus {
		min-width: 0;
		display: flex;
		align-items: center;
		gap: 7px;
		color: var(--text-dim);
		text-align: left;
		font-size: var(--fs-sm);
		font-weight: 550;
	}
	.focused .tile-focus { color: var(--text); }
	.tile-focus > span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.tile-focus > :global(svg) { flex: none; color: var(--text-faint); }
	.focused .tile-focus > :global(svg) { color: var(--brand-icon); }
	.tile-close {
		flex: none;
		display: grid;
		place-items: center;
		width: 26px;
		height: 26px;
		border-radius: var(--radius-sm);
		color: var(--text-faint);
	}
	.tile-close:hover { background: var(--bg-inset); color: var(--text); }
	.tile-chat { flex: 1; min-width: 0; min-height: 0; display: flex; }
	.tile-chat :global(.transcript) { flex: 1; min-width: 0; min-height: 0; padding: 16px; }
	.tile-preview {
		flex: 1;
		min-height: 0;
		display: flex;
		align-items: center;
		padding: 13px 15px;
		color: var(--text-faint);
		text-align: left;
	}
	.tile-preview:hover { background: color-mix(in srgb, var(--bg-inset) 36%, transparent); color: var(--text-dim); }
	.tile-preview > span { width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--fs-xs); }
	.empty-board { display: grid; justify-items: center; gap: 12px; padding: 24px; text-align: center; }
	.empty-board p { margin: 0; color: var(--text-faint); font-size: var(--fs-sm); }
	.empty-action { padding: 5px 10px; border-radius: var(--radius-chip); background: var(--bg-inset); color: var(--text-dim); font-size: var(--fs-xs); }
	.empty-action:hover { color: var(--text); }
	.placement-preview {
		position: absolute;
		inset: 52px 14px 14px;
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
	.sr-only { position: absolute; width: 1px; height: 1px; padding: 0; margin: -1px; overflow: hidden; clip: rect(0, 0, 0, 0); white-space: nowrap; border: 0; }
	@media (max-width: 860px) {
		.board { padding: 8px; }
		.board-head { align-items: flex-start; }
		.board-actions { flex-wrap: wrap; justify-content: flex-end; }
		.workspace-tile { border-radius: var(--radius); }
	}
</style>

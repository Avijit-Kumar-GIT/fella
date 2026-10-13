<script lang="ts">
	import { Button } from '$lib/components/ui/button';
	import { DropdownMenu } from '$lib/components/ui/dropdown-menu';
	import { firstActualQuestion, session } from '$lib/session.svelte';
	import { openRepository } from '$lib/commands';
	import { ipc } from '$lib/ipc';
	import { focusInvestigationConversation } from '$lib/investigation-navigation';
	import type { Investigation } from '$lib/types';
	import Icon from './Icon.svelte';

	let workspacePath = $derived(session.activeChat.workspaceScope);
	let workspaceName = $derived(
		workspacePath?.replace(/[/\\]+$/, '').split(/[\\/]/).at(-1) ?? ''
	);
	let workspaceAvailable = $derived(workspacePath === session.catalog.workspace);
	let historyOnly = $derived(Boolean(workspacePath && (
		session.workspaceAt(workspacePath)?.historyOnly || session.historyOnlyRepositoryPaths.includes(workspacePath)
	)));
	let isWorkspaceView = $derived(
		session.workspaceView === 'ask' ||
		session.workspaceView === 'board' ||
		session.workspaceView === 'workspace'
	);
	let investigations = $state<Investigation[]>([]);
	let summaryLoadFailed = $state(false);
	let investigationSelectionBusy = $state(false);
	let investigationCreationBusy = $state(false);
	let investigationError = $state('');
	let summaryRequest = 0;
	let selectionRequest = 0;
	let observedWorkspacePath: string | null | undefined;
	let currentInvestigation = $derived.by(() => {
		const firstQuestion = firstActualQuestion(session.activeChat.messages);
		const belongsToActiveConversation = (item: Investigation) =>
			item.conversation_id === session.activeChat.id ||
			Boolean(firstQuestion && item.brief_source_message_id === firstQuestion.id);
		return investigations.find((item) => item.id === session.activeInvestigationId && belongsToActiveConversation(item))
			?? investigations.find(belongsToActiveConversation)
			?? null;
	});
	let sortedInvestigations = $derived([...investigations].sort((left, right) => right.updated_at_ms - left.updated_at_ms));
	let coverageSummary = $derived.by(() => {
		if (summaryLoadFailed) return 'Unavailable';
		if (!currentInvestigation) return investigations.length ? 'Select to continue' : 'None yet';
		const active = (currentInvestigation.questions ?? []).filter((question) => question.status === 'active');
		if (!active.length) return currentInvestigation.status === 'complete' ? 'Complete · no active Questions' : 'No Questions yet';
		const answered = active.filter((question) => question.answer_status === 'answered').length;
		const needsAttention = active.filter((question) => question.answer_status !== 'answered').length;
		if (currentInvestigation.status === 'archived') return `Archived · ${answered}/${active.length} answered`;
		if (currentInvestigation.status === 'complete') {
			return needsAttention
				? `Closed with gaps · ${needsAttention} need review`
				: `Complete · ${answered}/${active.length} answered`;
		}
		return needsAttention
			? `${answered}/${active.length} answered · ${needsAttention} need attention`
			: `${answered}/${active.length} answered · Ready to close`;
	});
	let currentInvestigationLabel = $derived(currentInvestigation?.title?.trim() || 'Choose Investigation');

	function investigationCoverage(investigation: Investigation): string {
		const active = (investigation.questions ?? []).filter((question) => question.status === 'active');
		if (!active.length) return investigation.status === 'complete' ? 'Complete · no Questions' : 'No Questions yet';
		const answered = active.filter((question) => question.answer_status === 'answered').length;
		const unresolved = active.length - answered;
		if (investigation.status === 'archived') return `Archived · ${answered}/${active.length} answered`;
		if (investigation.status === 'complete') {
			return unresolved
				? `Closed with gaps · ${unresolved} need review`
				: `Complete · ${answered}/${active.length} answered`;
		}
		return unresolved ? `${answered}/${active.length} answered · ${unresolved} need attention` : `${answered}/${active.length} answered`;
	}

	function investigationOptionLabel(investigation: Investigation): string {
		const title = investigation.title.trim() || 'Untitled Investigation';
		const normalizedTitle = title.toLocaleLowerCase();
		const duplicates = investigations
			.filter((item) => (item.title.trim() || 'Untitled Investigation').toLocaleLowerCase() === normalizedTitle)
			.sort((left, right) => left.created_at_ms - right.created_at_ms || left.id.localeCompare(right.id));
		if (duplicates.length < 2) return title;
		return `${title} (${duplicates.findIndex((item) => item.id === investigation.id) + 1})`;
	}

	async function refreshInvestigationSummary(path: string, conversationId: string): Promise<void> {
		const request = ++summaryRequest;
		summaryLoadFailed = false;
		try {
			const records = await ipc.investigationsList();
			if (request !== summaryRequest || path !== workspacePath || conversationId !== session.activeChat.id) return;
			investigations = records.filter((item) => item.workspace_id === path);
			const firstQuestion = firstActualQuestion(session.activeChat.messages);
			const belongsToActiveConversation = (item: Investigation) =>
				item.conversation_id === conversationId ||
				(Boolean(firstQuestion) && item.brief_source_message_id === firstQuestion?.id);
			const active = investigations.find((item) => item.id === session.activeInvestigationId && belongsToActiveConversation(item))
				?? investigations.find(belongsToActiveConversation);
			if (active && session.activeInvestigationId !== active.id) session.setActiveInvestigation(active.id);
		} catch {
			if (request === summaryRequest) {
				investigations = [];
				summaryLoadFailed = true;
			}
		}
	}

	async function selectInvestigation(id: string): Promise<void> {
		const item = investigations.find((record) => record.id === id);
		if (!item || investigationSelectionBusy || investigationCreationBusy) return;
		if (item.conversation_id === session.activeChat.id && currentInvestigation?.id === item.id) return;
		const request = ++selectionRequest;
		const sourceConversationId = session.activeChat.id;
		investigationSelectionBusy = true;
		investigationError = '';
		try {
			const linked = await focusInvestigationConversation(
				item,
				'preserve',
				undefined,
				() => request === selectionRequest &&
					workspacePath === item.workspace_id &&
					workspaceAvailable &&
					session.activeChat.id === sourceConversationId
			);
			if (!linked || request !== selectionRequest || workspacePath !== item.workspace_id) return;
			investigations = investigations.map((record) => record.id === linked.id ? linked : record);
			session.investigationsVersion += 1;
		} catch (cause) {
			if (request === selectionRequest) {
				investigationError = cause instanceof Error ? cause.message : String(cause);
			}
		} finally {
			if (request === selectionRequest) investigationSelectionBusy = false;
		}
	}

	function nextInvestigationTitle(): string {
		const titles = new Set(investigations.map((item) => item.title.trim().toLocaleLowerCase()));
		if (!titles.has('new investigation')) return 'New investigation';
		let suffix = 2;
		while (titles.has(`new investigation ${suffix}`)) suffix += 1;
		return `New investigation ${suffix}`;
	}

	async function createInvestigation(): Promise<void> {
		if (!workspacePath || !workspaceAvailable || investigationCreationBusy || investigationSelectionBusy) return;
		const path = workspacePath;
		const request = ++selectionRequest;
		investigationCreationBusy = true;
		investigationError = '';
		try {
			if (!session.newConversation(path)) throw new Error('Fella could not start a conversation in this workspace.');
			const conversation = session.activeChat;
			if (conversation.workspaceScope !== path) throw new Error('The new conversation does not belong to this workspace.');
			session.setWorkspacePane('investigation');
			const record = await ipc.investigationCreate(path, nextInvestigationTitle(), undefined, undefined, conversation.id);
			if (workspacePath === path) investigations = [record, ...investigations.filter((item) => item.id !== record.id)];
			session.investigationsVersion += 1;
			if (request === selectionRequest && workspacePath === path && session.activeChat.id === conversation.id) {
				session.setActiveInvestigation(record.id);
			}
		} catch (cause) {
			if (request === selectionRequest) investigationError = cause instanceof Error ? cause.message : String(cause);
		} finally {
			if (request === selectionRequest) investigationCreationBusy = false;
		}
	}

	$effect(() => {
		const path = workspacePath;
		const conversationId = session.activeChat.id;
		const messageCount = session.activeChat.messages.length;
		const investigationsVersion = session.investigationsVersion;
		const summaryVersion = session.workspaceInvestigationSummaryVersion;
		const available = workspaceAvailable;
		void messageCount;
		void investigationsVersion;
		void summaryVersion;
		if (path !== observedWorkspacePath) {
			observedWorkspacePath = path;
			selectionRequest += 1;
			investigationSelectionBusy = false;
			investigationCreationBusy = false;
			investigationError = '';
		}
		if (!path || !available) {
			summaryRequest += 1;
			investigations = [];
			return;
		}
		void refreshInvestigationSummary(path, conversationId);
	});
</script>

{#if workspacePath && isWorkspaceView && session.workspaceView !== 'board'}
	<div class="workspace-context-bar">
		<div class="workspace-identity" role="group" aria-label={`Workspace: ${workspaceName}`}>
			<Icon name="folder" size={16} />
			<span class="workspace-identity-label">Workspace</span>
			<strong class="workspace-name" title={workspacePath}>{workspaceName}</strong>
		</div>

		{#if (session.workspaceView === 'ask' || session.workspaceView === 'workspace') && workspaceAvailable}
			<div class="workspace-context-actions">
				{#if session.workspacePaneOpen && session.workspacePane === 'investigation' && workspaceAvailable}
				<DropdownMenu.Root>
					<DropdownMenu.Trigger
						class="workspace-investigation-trigger"
						aria-label={`Switch Investigation. Current: ${currentInvestigationLabel}. ${coverageSummary}`}
						title="Choose an Investigation or start a new one"
						data-testid="workspace-investigation-summary"
					>
						<Icon name="bookmark" size={14} />
						<span class="workspace-investigation-copy">
							<strong title={currentInvestigationLabel}>{currentInvestigationLabel}</strong>
							<span class="workspace-investigation-coverage">{coverageSummary}</span>
						</span>
						<Icon name="chevron-down" size={14} />
					</DropdownMenu.Trigger>
					<DropdownMenu.Portal>
						<DropdownMenu.Content class="fella-ui-menu-content workspace-investigation-menu" side="bottom" align="start" sideOffset={6}>
							{#if investigations.length}
								<DropdownMenu.RadioGroup value={currentInvestigation?.id ?? ''} onValueChange={(id) => void selectInvestigation(id)}>
									{#each sortedInvestigations as item (item.id)}
										<DropdownMenu.RadioItem
											class="fella-ui-menu-item workspace-investigation-option"
											value={item.id}
											aria-label={investigationOptionLabel(item)}
											disabled={investigationSelectionBusy || investigationCreationBusy}
										>
											<span class="workspace-investigation-option-copy">
												<strong>{investigationOptionLabel(item)}</strong>
												<small>{investigationCoverage(item)}</small>
											</span>
											{#if item.id === currentInvestigation?.id}<Icon name="check" size={14} />{/if}
										</DropdownMenu.RadioItem>
									{/each}
								</DropdownMenu.RadioGroup>
							{:else}
								<DropdownMenu.Item class="fella-ui-menu-item workspace-investigation-empty" disabled>
									{summaryLoadFailed ? 'Investigations unavailable' : 'No Investigations yet'}
								</DropdownMenu.Item>
							{/if}
							<div class="workspace-investigation-separator" role="separator"></div>
							<DropdownMenu.Item
								class="fella-ui-menu-item workspace-investigation-new"
								disabled={investigationCreationBusy || investigationSelectionBusy}
								onSelect={() => void createInvestigation()}
							>
								<Icon name="plus" size={14} />
								{investigationCreationBusy ? 'Creating…' : 'New Investigation'}
							</DropdownMenu.Item>
						</DropdownMenu.Content>
					</DropdownMenu.Portal>
				</DropdownMenu.Root>
				{/if}
				<Button
					variant="ghost"
					size="sm"
					class="workspace-pane-toggle"
					type="button"
					aria-expanded={session.workspacePaneOpen}
					aria-controls="workspace-tools-pane"
					aria-label={session.workspaceToolChooserOpen ? 'Close workspace tools' : session.workspacePaneOpen ? 'Choose a workspace tool' : 'Open workspace tools'}
					title={session.workspaceToolChooserOpen ? 'Close workspace tools' : session.workspacePaneOpen ? 'Choose a workspace tool' : 'Open workspace tools'}
					onclick={() => session.toggleWorkspacePane()}
				>
					<Icon name="panel" size={16} />
					<span>Tools</span>
				</Button>
				{#if investigationError}<span class="workspace-investigation-error" role="alert" title={investigationError}>Couldn’t switch Investigation</span>{/if}
			</div>
		{:else if session.workspaceView === 'ask' && historyOnly}
			<div class="workspace-history-action">
				<span class="workspace-history-label" role="status">History only</span>
				<Button
					variant="outline"
					size="sm"
					class="workspace-open-investigation"
					onclick={() => void openRepository(workspacePath, { reportFailure: true })}
				>
					<Icon name="refresh" size={14} />Reconnect folder
				</Button>
			</div>
		{/if}
	</div>
{/if}

<style>
	.workspace-context-bar {
		flex: none;
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-4);
		min-height: 48px;
		padding: 0 var(--pad);
		border-bottom: 1px solid var(--border);
		background: var(--workspace-canvas);
	}
	.workspace-identity {
		flex: 1;
	}
	.workspace-identity {
		display: flex;
		align-items: center;
		gap: 8px;
		min-width: 0;
		color: var(--text-faint);
		font-size: var(--fs-sm);
	}
	.workspace-identity :global(svg) {
		flex: none;
	}
	.workspace-identity-label {
		flex: none;
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.workspace-name {
		min-width: 0;
		overflow: hidden;
		color: var(--text);
		font-weight: 600;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.workspace-history-action {
		display: flex;
		flex: none;
		align-items: center;
		gap: var(--space-2);
	}
	.workspace-history-label {
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.workspace-context-actions {
		display: flex;
		align-items: center;
		justify-content: flex-end;
		gap: var(--space-3);
		min-width: 0;
	}
	:global(.workspace-pane-toggle) {
		flex: none;
		min-height: 34px;
		gap: 7px;
		padding-inline: 10px;
		border-radius: var(--radius);
		color: var(--text-dim);
	}
	:global(.workspace-pane-toggle:hover),
	:global(.workspace-pane-toggle[aria-expanded='true']) {
		background: var(--sidebar-hover);
		color: var(--text);
	}
	:global(.workspace-investigation-trigger) {
		display: flex;
		flex: 0 1 320px;
		align-items: center;
		gap: 8px;
		min-width: 190px;
		max-width: 360px;
		min-height: 34px;
		padding: 3px 10px;
		border: 1px solid var(--pane-edge);
		border-radius: 999px;
		background: color-mix(in srgb, var(--text) 3%, var(--workspace-canvas));
		color: var(--text-dim);
		text-align: left;
		transition: background var(--dur-fast) var(--ease), border-color var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
	}
	:global(.workspace-investigation-trigger:hover),
	:global(.workspace-investigation-trigger[data-state='open']) {
		border-color: var(--border-strong);
		background: var(--sidebar-hover);
		color: var(--text);
	}
	:global(.workspace-investigation-trigger:focus-visible) {
		outline: 2px solid var(--brand);
		outline-offset: 2px;
	}
	:global(.workspace-investigation-trigger > svg) { flex: none; }
	.workspace-investigation-copy {
		display: flex;
		flex: 1;
		align-items: baseline;
		gap: 7px;
		min-width: 0;
		font-size: var(--fs-xs);
		white-space: nowrap;
	}
	.workspace-investigation-copy strong {
		flex: 0 1 150px;
		min-width: 0;
		overflow: hidden;
		color: var(--text);
		font-size: var(--fs-sm);
		font-weight: 550;
		text-overflow: ellipsis;
	}
	.workspace-investigation-coverage {
		flex: 1 1 auto;
		min-width: 0;
		overflow: hidden;
		color: var(--text-faint);
		text-overflow: ellipsis;
	}
	:global(.workspace-investigation-menu) {
		width: min(360px, calc(100vw - 24px));
		max-height: min(65vh, 440px);
		overflow-y: auto;
		overscroll-behavior: contain;
	}
	:global(.workspace-investigation-option) {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 10px;
		min-height: 48px;
		padding: 7px 10px;
		white-space: normal;
	}
	.workspace-investigation-option-copy {
		display: grid;
		flex: 1;
		gap: 3px;
		min-width: 0;
	}
	.workspace-investigation-option-copy strong {
		overflow: hidden;
		color: var(--text);
		font-size: var(--fs-sm);
		font-weight: 550;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.workspace-investigation-option-copy small {
		color: var(--text-faint);
		font-size: var(--fs-xs);
		line-height: 1.35;
	}
	:global(.workspace-investigation-option svg) { flex: none; color: var(--brand); }
	.workspace-investigation-separator {
		height: 1px;
		margin: 5px 4px;
		background: var(--border);
	}
	:global(.workspace-investigation-new) {
		display: flex;
		align-items: center;
		gap: 8px;
		min-height: 36px;
		padding-inline: 10px;
	}
	.workspace-investigation-error {
		max-width: 190px;
		overflow: hidden;
		color: var(--danger, #b42318);
		font-size: var(--fs-xs);
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	:global(.workspace-open-investigation) {
		flex: none;
		min-height: 30px;
		gap: 6px;
		border-radius: var(--radius);
		font-size: var(--fs-sm);
	}
	@media (max-width: 620px) {
		.workspace-context-bar {
			flex-wrap: wrap;
			gap: var(--space-2);
			padding-inline: var(--space-2);
		}
		.workspace-context-actions {
			flex: 1 1 auto;
			flex-wrap: wrap;
			gap: var(--space-1);
		}
		:global(.workspace-investigation-trigger) { flex: 1 1 210px; max-width: none; }
		.workspace-identity-label {
			display: none;
		}
	}
</style>

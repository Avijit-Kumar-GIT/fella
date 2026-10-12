<script lang="ts">
	import { Button } from '$lib/components/ui/button';
	import { firstActualQuestion, session } from '$lib/session.svelte';
	import { openRepository } from '$lib/commands';
	import { ipc } from '$lib/ipc';
	import type { Investigation } from '$lib/types';
	import WorkbenchPanelPicker from './WorkbenchPanelPicker.svelte';
	import Icon from './Icon.svelte';

	const presets = [
		{ id: 'focus', label: 'Focus', description: 'Conversation and current result' },
		{ id: 'investigation', label: 'Investigation', description: 'Brief, Questions, and coverage' },
		{ id: 'workbench', label: 'Workbench', description: 'Focus beside Investigation details and an optional preview' }
	] as const;

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
	let summaryRequest = 0;
	let currentInvestigation = $derived.by(() => {
		if (
			session.workspaceView === 'workspace' &&
			session.workspacePane === 'investigation' &&
			session.activeInvestigationId
		) {
			const selected = investigations.find((item) => item.id === session.activeInvestigationId);
			if (selected) return selected;
		}
		const firstQuestion = firstActualQuestion(session.activeChat.messages);
		return investigations.find((item) =>
			item.conversation_id === session.activeChat.id ||
			(Boolean(firstQuestion) && item.brief_source_message_id === firstQuestion?.id)
		) ?? null;
	});
	let coverageSummary = $derived.by(() => {
		if (summaryLoadFailed) return 'Coverage unavailable';
		if (!currentInvestigation) return 'Ready to start';
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
	let currentInvestigationLabel = $derived(currentInvestigation?.title?.trim() || 'No Investigation yet');

	function isPresetSelected(preset: (typeof presets)[number]['id']): boolean {
		if (preset === 'focus') return session.workspaceView === 'ask' && session.workspaceLayoutPreset === 'focus';
		return session.workspaceView === 'workspace' && session.workspaceLayoutPreset === preset;
	}

	async function refreshInvestigationSummary(path: string, conversationId: string): Promise<void> {
		const request = ++summaryRequest;
		summaryLoadFailed = false;
		try {
			const records = await ipc.investigationsList();
			if (request !== summaryRequest || path !== workspacePath || conversationId !== session.activeChat.id) return;
			investigations = records.filter((item) => item.workspace_id === path);
		} catch {
			if (request === summaryRequest) {
				investigations = [];
				summaryLoadFailed = true;
			}
		}
	}

	function choosePreset(preset: (typeof presets)[number]['id']): void {
		session.setWorkspaceLayoutPreset(preset);
		if (workspacePath && workspaceAvailable) void refreshInvestigationSummary(workspacePath, session.activeChat.id);
	}

	$effect(() => {
		const path = workspacePath;
		const conversationId = session.activeChat.id;
		const messageCount = session.activeChat.messages.length;
		const investigationsVersion = session.investigationsVersion;
		const available = workspaceAvailable;
		void messageCount;
		void investigationsVersion;
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
			<div class="workspace-preset-actions">
				{#if workspaceAvailable}
					<span
						class="workspace-investigation-summary"
						role="status"
						aria-label={`Investigation ${currentInvestigationLabel}. Coverage: ${coverageSummary}`}
						data-testid="workspace-investigation-summary"
					><strong title={currentInvestigationLabel}>{currentInvestigationLabel}</strong><span class="workspace-investigation-coverage">{coverageSummary}</span></span>
				{/if}
				<div class="workspace-preset-control fella-pill-nav" role="group" aria-label="Workspace layout">
					{#each presets as preset (preset.id)}
						<button
							class="fella-pill-nav-item"
							type="button"
							class:active={isPresetSelected(preset.id)}
							aria-label={`${preset.label}: ${preset.description}`}
							aria-pressed={isPresetSelected(preset.id)}
							title={preset.description}
							onclick={() => choosePreset(preset.id)}
						>{preset.label}</button>
					{/each}
				</div>
				{#if session.workspaceView === 'workspace' && session.workspaceLayoutPreset === 'workbench'}
					<WorkbenchPanelPicker />
				{/if}
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
	.workspace-preset-actions {
		display: flex;
		align-items: center;
		justify-content: flex-end;
		gap: var(--space-3);
		min-width: 0;
	}
	.workspace-investigation-summary {
		display: flex;
		flex: 0 1 360px;
		align-items: center;
		gap: 6px;
		min-width: 0;
		max-width: 360px;
		color: var(--text-faint);
		font-size: var(--fs-xs);
		text-align: right;
		white-space: nowrap;
	}
	.workspace-investigation-summary strong {
		flex: 0 1 140px;
		min-width: 50px;
		overflow: hidden;
		color: var(--text-dim);
		font-weight: 550;
		text-overflow: ellipsis;
	}
	.workspace-investigation-coverage {
		flex: 1 1 auto;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.workspace-preset-control {
		flex: none;
		min-width: 0;
		overflow-x: auto;
		scrollbar-width: none;
	}
	.workspace-preset-control::-webkit-scrollbar { display: none; }
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
		.workspace-preset-actions {
			flex: 1 1 auto;
			flex-wrap: wrap;
			gap: var(--space-1);
		}
		.workspace-investigation-summary {
			flex: 1 1 120px;
			max-width: none;
		}
		.workspace-identity-label {
			display: none;
		}
	}
</style>

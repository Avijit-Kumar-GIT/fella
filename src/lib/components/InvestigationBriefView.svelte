<script lang="ts">
	import { firstActualQuestion, session } from '$lib/session.svelte';
	import { Button } from '$lib/components/ui/button';
	import { openConversation } from '$lib/commands';
	import { Textarea } from '$lib/components/ui/textarea';
	import { ipc, isDesktop } from '$lib/ipc';
	import { analysisTurnIdsAfterSourceMessage } from '$lib/investigation-runs';
	import type { ConversationSummary, Investigation, InvestigationBrief } from '$lib/types';
	import { findBriefProposal, type BriefProposal, type BriefProposalTarget } from '$lib/investigation-brief-proposal';
	import Icon from './Icon.svelte';

	let investigations = $state<Investigation[]>([]);
	let selectedId = $state<string | null>(null);
	let workspacePath = $state<string | null>(null);
	let loadedWorkspace = $state<string | null>(null);
	let loadedInvestigationsVersion = -1;
	let goal = $state('');
	let audience = $state('');
	let desiredOutput = $state('');
	let constraintsText = $state('');
	let savedBrief = $state('');
	let loadedBriefKey = '';
	let proposalMessageId = $state<string | null>(null);
	let proposalText = $state('');
	let proposalTarget = $state<BriefProposalTarget>('goal');
	let loading = $state(false);
	let creating = $state(false);
	let openingConversation = $state(false);
	let saving = $state(false);
	let error = $state('');
	let requestSerial = 0;
	const syncingInvestigations = new Set<string>();

	let selected = $derived(investigations.find((item) => item.id === selectedId) ?? null);
	let currentConversationSource = $derived.by(() => {
		const conversation = session.activeChat;
		if (!workspacePath || conversation.workspaceScope !== workspacePath) return undefined;
		return firstActualQuestion(conversation.messages);
	});
	let currentConversationInvestigation = $derived(
		currentConversationSource
			? investigations.find((item) => item.workspace_id === workspacePath && (item.conversation_id === session.activeChat.id || item.brief_source_message_id === currentConversationSource?.id)) ?? null
			: null
	);
	let unlinkedAnalysisTurnIds = $derived.by(() => {
		const item = selected;
		const conversation = session.activeChat;
		if (!item || conversation.workspaceScope !== item.workspace_id) return [];
		const linked = new Set(item.analysis_turn_ids ?? []);
		return analysisTurnIdsAfterSourceMessage(
			conversation.messages,
			item.brief_source_message_id,
			item.workspace_id
		).filter((turnId) => !linked.has(turnId));
	});
	let pendingProposal: BriefProposal | null = $derived.by(() => {
		const item = selected;
		const conversation = session.activeChat;
		if (!item?.brief_source_message_id || conversation.workspaceScope !== item.workspace_id) return null;
		return findBriefProposal(
			conversation.messages,
			item.brief_source_message_id,
			item.brief_reviewed_message_id
		);
	});
	let currentBrief: InvestigationBrief = $derived({
		goal,
		audience,
		desired_output: desiredOutput,
		constraints: constraintsText.split('\n').map((item) => item.trim()).filter(Boolean)
	});
	let briefKey = $derived(JSON.stringify(currentBrief));
	let dirty = $derived(Boolean(selected && briefKey !== savedBrief));

	async function loadWorkspace(path: string): Promise<void> {
		const serial = ++requestSerial;
		loading = true;
		error = '';
		investigations = [];
		selectedId = null;
		try {
			const all = await ipc.investigationsList();
			if (serial !== requestSerial) return;
			investigations = all.filter((item) => item.workspace_id === path);
			selectedId = investigations[0]?.id ?? null;
		} catch (cause) {
			if (serial === requestSerial) error = cause instanceof Error ? cause.message : String(cause);
		} finally {
			if (serial === requestSerial) loading = false;
		}
	}

	async function syncAnalysisTurns(investigationId: string): Promise<void> {
		if (syncingInvestigations.has(investigationId)) return;
		syncingInvestigations.add(investigationId);
		try {
			while (true) {
				const item = investigations.find((record) => record.id === investigationId);
				const conversation = session.activeChat;
				if (!item || conversation.workspaceScope !== item.workspace_id) return;
				const linked = new Set(item.analysis_turn_ids ?? []);
				const missing = analysisTurnIdsAfterSourceMessage(
					conversation.messages,
					item.brief_source_message_id,
					item.workspace_id
				).filter((turnId) => !linked.has(turnId));
				if (!missing.length) return;
				const updated = await ipc.investigationLinkAnalysisTurns(item.id, missing);
				const confirmed = new Set(updated.analysis_turn_ids ?? []);
				if (missing.some((turnId) => !confirmed.has(turnId))) {
					error = 'The saved analysis-run links could not be confirmed. Reopen this Investigation to try again.';
					return;
				}
				investigations = investigations.map((record) => record.id === updated.id ? updated : record);
			}
		} catch (cause) {
			error = cause instanceof Error ? cause.message : String(cause);
		} finally {
			syncingInvestigations.delete(investigationId);
		}
	}

	$effect(() => {
		const path = session.catalog.workspace;
		const version = session.investigationsVersion;
		workspacePath = path;
		if (!path) {
			requestSerial += 1;
			loadedWorkspace = null;
			loadedInvestigationsVersion = version;
			investigations = [];
			selectedId = null;
			loading = false;
			return;
		}
		if (path === loadedWorkspace && version === loadedInvestigationsVersion) return;
		loadedWorkspace = path;
		loadedInvestigationsVersion = version;
		void loadWorkspace(path);
	});

	$effect(() => {
		const item = selected;
		const missing = unlinkedAnalysisTurnIds;
		if (item && missing.length && isDesktop()) void syncAnalysisTurns(item.id);
	});

	$effect(() => {
		const item = selected;
		const key = item ? `${item.id}:${JSON.stringify(item.brief)}` : '';
		if (key === loadedBriefKey) return;
		loadedBriefKey = key;
		if (!item) {
			goal = '';
			audience = '';
			desiredOutput = '';
			constraintsText = '';
			savedBrief = '';
			return;
		}
		goal = item.brief.goal;
		audience = item.brief.audience;
		desiredOutput = item.brief.desired_output;
		constraintsText = item.brief.constraints.join('\n');
		savedBrief = JSON.stringify(item.brief);
	});

	$effect(() => {
		const proposal = pendingProposal;
		if ((proposal?.messageId ?? null) === proposalMessageId) return;
		proposalMessageId = proposal?.messageId ?? null;
		proposalText = proposal?.text ?? '';
		proposalTarget = proposal?.target ?? 'goal';
	});

	async function createInvestigation(): Promise<void> {
		if (!workspacePath || creating || !isDesktop()) return;
		creating = true;
		error = '';
		const request = currentConversationSource;
		try {
			const record = request
				? await ipc.investigationEnsure(workspacePath, session.activeChat.id, request.text, request.id)
				: await ipc.investigationCreate(workspacePath);
			investigations = [record, ...investigations.filter((item) => item.id !== record.id)];
			selectedId = record.id;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : String(cause);
		} finally {
			creating = false;
		}
	}

	async function openSelectedConversation(): Promise<void> {
		const item = selected;
		if (!item || openingConversation || !isDesktop()) return;
		openingConversation = true;
		error = '';
		try {
			let conversationId = item.conversation_id;
			let summary: ConversationSummary | null = null;
			if (!conversationId && item.brief_source_message_id) {
				const summaries = await ipc.conversationsList();
				summary = summaries.find((candidate) =>
					candidate.workspace === item.workspace_id &&
					candidate.first_question_message_id === item.brief_source_message_id
				) ?? null;
				conversationId = summary?.id;
				if (conversationId) {
					const linked = await ipc.investigationEnsure(
						item.workspace_id,
						conversationId,
						item.brief.goal,
						item.brief_source_message_id
					);
					investigations = investigations.map((record) => record.id === linked.id ? linked : record);
				}
			}
			if (!conversationId) throw new Error('The source conversation is not available in local history.');

			const loadedIndex = session.conversations.findIndex((conversation) =>
				conversation.id === conversationId && conversation.workspaceScope === item.workspace_id
			);
			if (loadedIndex >= 0) {
				if (!session.activateConversation(loadedIndex)) throw new Error('Fella could not focus the linked conversation.');
			} else {
				summary ??= (await ipc.conversationsList()).find((candidate) =>
					candidate.id === conversationId && candidate.workspace === item.workspace_id
				) ?? null;
				if (!summary) throw new Error('The linked conversation is not available in local history.');
				if (!(await openConversation(summary))) throw new Error('Fella could not open the linked conversation.');
			}
			session.setWorkspaceView('ask');
		} catch (cause) {
			error = cause instanceof Error ? cause.message : String(cause);
		} finally {
			openingConversation = false;
		}
	}

	async function saveBrief(): Promise<boolean> {
		if (!selected || saving || !dirty) return !dirty;
		saving = true;
		error = '';
		try {
			const updated = await ipc.investigationUpdateBrief(selected.id, currentBrief);
			investigations = investigations.map((item) => item.id === updated.id ? updated : item);
			savedBrief = JSON.stringify(updated.brief);
			return true;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : String(cause);
			return false;
		} finally {
			saving = false;
		}
	}

	async function reviewProposal(accept: boolean): Promise<void> {
		const item = selected;
		const proposal = pendingProposal;
		if (!item || !proposal || saving || (accept && !proposalText.trim())) return;
		saving = true;
		error = '';
		let nextBrief = item.brief;
		if (accept) {
			nextBrief = { ...currentBrief };
			if (proposalTarget === 'goal') nextBrief.goal = proposalText.trim();
			else if (proposalTarget === 'desired_output') nextBrief.desired_output = proposalText.trim();
			else nextBrief.constraints = [...nextBrief.constraints, proposalText.trim()];
		}
		try {
			const updated = await ipc.investigationUpdateBrief(item.id, nextBrief, proposal.messageId);
			investigations = investigations.map((record) => record.id === updated.id ? updated : record);
			if (accept) savedBrief = JSON.stringify(updated.brief);
		} catch (cause) {
			error = cause instanceof Error ? cause.message : String(cause);
		} finally {
			saving = false;
		}
	}

	async function selectInvestigation(id: string): Promise<void> {
		if (id === selectedId || saving) return;
		if (dirty && !(await saveBrief())) return;
		selectedId = id;
	}
</script>

<section class="investigation-page" aria-labelledby="investigation-title">
	<header class="page-head">
		<div>
			<h1 id="investigation-title">Investigation brief</h1>
			<p>Keep the goal, audience, deliverable, and constraints clear as the analysis develops.</p>
		</div>
		{#if workspacePath}
			<Button type="button" disabled={loading || creating} onclick={() => void createInvestigation()}>
				<Icon name="plus" size={16} />{creating ? 'Opening…' : currentConversationInvestigation ? 'Open current Investigation' : currentConversationSource ? 'Start from conversation' : 'New investigation'}
			</Button>
		{/if}
	</header>
	{#if error && !selected}
		<p class="error" role="alert">{error}</p>
	{/if}

	{#if !workspacePath}
		<div class="empty-state">
			<Icon name="folder" size={20} />
			<h2>Open a workspace to start an Investigation</h2>
			<p>Investigations are scoped to a folder so their intent stays alongside the data being analyzed.</p>
		</div>
	{:else if loading}
		<p class="status" role="status">Loading investigations…</p>
	{:else if !investigations.length}
		<div class="empty-state">
			<Icon name="bookmark" size={20} />
			<h2>No investigations yet</h2>
			<p>Workspace questions start an Investigation automatically. You can also create one here before beginning.</p>
			<Button type="button" disabled={creating} onclick={() => void createInvestigation()}>
				<Icon name="plus" size={16} />{creating ? 'Opening…' : currentConversationSource ? 'Start from conversation' : 'Start from this workspace'}
			</Button>
		</div>
	{:else}
		<div class="investigation-layout">
			<nav class="investigation-list" aria-label="Workspace investigations">
				{#each investigations as item (item.id)}
					<button
						class="investigation-choice"
						class:selected={item.id === selectedId}
						type="button"
						aria-current={item.id === selectedId ? 'true' : undefined}
						disabled={saving}
						onclick={() => void selectInvestigation(item.id)}
					>
						<span>{item.title}</span>
						<small>{new Date(item.updated_at_ms).toLocaleDateString()}</small>
					</button>
				{/each}
			</nav>

			{#if selected}
				<div class="brief-editor">
					<div class="editor-heading">
						<div>
							<h2>{selected.title}</h2>
							{#if selected.brief_source_message_id}
								<p>Goal seeded from the conversation’s first request. Editing this brief does not change that message.</p>
							{/if}
						</div>
						<div class="editor-actions">
							<Button type="button" variant="outline" disabled={openingConversation || (!selected.conversation_id && !selected.brief_source_message_id)} onclick={() => void openSelectedConversation()}>
								<Icon name="ask" size={16} />{openingConversation ? 'Opening…' : 'Open conversation'}
							</Button>
							<Button type="button" disabled={!dirty || saving} onclick={() => void saveBrief()}>
								<Icon name="check" size={16} />{saving ? 'Saving…' : 'Save brief'}
							</Button>
						</div>
					</div>
					{#if pendingProposal}
						<section class="brief-proposal" aria-labelledby="brief-proposal-title">
							<div class="proposal-copy">
								<h3 id="brief-proposal-title">Possible change to the brief</h3>
								<p>A message uses explicit revision or source-scope language. Review it before updating the brief.</p>
							</div>
							<label for="proposal-target">Apply as</label>
							<select id="proposal-target" bind:value={proposalTarget} disabled={saving}>
								<option value="goal">New goal</option>
								<option value="desired_output">Desired output</option>
								<option value="constraint">Additional constraint</option>
							</select>
							<Textarea id="proposal-text" class="brief-field proposal-field" bind:value={proposalText} disabled={saving} aria-label="Proposed brief update" />
							<div class="proposal-actions">
								<Button type="button" disabled={saving || !proposalText.trim()} onclick={() => void reviewProposal(true)}>
									<Icon name="check" size={16} />{saving ? 'Saving…' : 'Apply update'}
								</Button>
								<Button type="button" variant="ghost" disabled={saving} onclick={() => void reviewProposal(false)}>Dismiss</Button>
							</div>
						</section>
					{/if}
					<label for="investigation-goal">Goal</label>
					<Textarea id="investigation-goal" class="brief-field goal-field" bind:value={goal} disabled={saving} placeholder="What question should this Investigation answer?" />
					<label for="investigation-audience">Audience</label>
					<Textarea id="investigation-audience" class="brief-field audience-field" bind:value={audience} disabled={saving} placeholder="Who needs this analysis?" />
					<label for="investigation-output">Desired output</label>
					<Textarea id="investigation-output" class="brief-field" bind:value={desiredOutput} disabled={saving} placeholder="What should the analyst deliver?" />
					<label for="investigation-constraints">Constraints</label>
					<Textarea id="investigation-constraints" class="brief-field constraints-field" bind:value={constraintsText} disabled={saving} placeholder="One constraint per line" />
					<div class="editor-foot">
						<span role={error ? 'alert' : 'status'}>{error || (saving ? 'Saving…' : dirty ? 'Unsaved changes' : 'Saved locally')}</span>
						{#if selected.brief_source_message_id}<span>Original request preserved</span>{/if}
						<span>{selected.analysis_turn_ids?.length ?? 0} saved run{(selected.analysis_turn_ids?.length ?? 0) === 1 ? '' : 's'} linked</span>
					</div>
				</div>
			{/if}
		</div>
	{/if}
</section>

<style>
	.investigation-page {
		flex: 1;
		min-height: 0;
		width: 100%;
		max-width: var(--content-max);
		margin: 0 auto;
		padding: var(--space-6) var(--pad);
		overflow: auto;
	}
	.page-head,
	.editor-heading,
	.editor-foot {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-4);
	}
	.page-head { margin-bottom: var(--space-5); }
	h1, h2, p { margin: 0; }
	h1 { font-size: clamp(24px, 3vw, 32px); font-weight: 650; letter-spacing: -0.03em; }
	.page-head p, .editor-heading p { margin-top: var(--space-1); color: var(--text-dim); }
	.investigation-layout { display: grid; grid-template-columns: minmax(180px, 240px) minmax(0, 1fr); gap: var(--space-6); }
	.investigation-list { display: flex; flex-direction: column; gap: 4px; }
	.investigation-choice {
		display: flex; flex-direction: column; align-items: flex-start; gap: 3px;
		padding: 9px 11px; border: 1px solid transparent; border-radius: var(--radius-md);
		background: transparent; color: var(--text); text-align: left; cursor: pointer;
	}
	.investigation-choice:hover { background: var(--bg-inset); }
	.investigation-choice.selected { border-color: var(--border); background: var(--bg-inset); }
	.investigation-choice span { max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--fs-sm); font-weight: 600; }
	.investigation-choice small, .status, .editor-foot { color: var(--text-faint); font-size: var(--fs-xs); }
	.error { margin: 0 0 var(--space-3); color: var(--danger, #b42318); font-size: var(--fs-sm); overflow-wrap: anywhere; }
	.brief-editor { min-width: 0; border-block: 1px solid var(--border); }
	.editor-heading { display: flex; align-items: center; justify-content: space-between; gap: var(--space-4); padding: var(--space-3) 0; border-bottom: 1px solid var(--border); }
	.editor-actions { display: flex; flex: none; align-items: center; gap: var(--space-2); }
	.brief-proposal { display: grid; grid-template-columns: minmax(0, 1fr) auto; align-items: center; gap: var(--space-3); padding: var(--space-3); margin-top: var(--space-3); border: 1px solid var(--border); border-radius: var(--radius-md); background: var(--bg-inset); }
	.proposal-copy { grid-column: 1 / -1; }
	.proposal-copy h3 { margin: 0; font-size: var(--fs-sm); font-weight: 650; }
	.proposal-copy p { margin-top: 3px; color: var(--text-dim); font-size: var(--fs-xs); }
	.brief-proposal label { margin: 0; font-size: var(--fs-xs); }
	.brief-proposal select { min-width: 190px; padding: 6px 9px; border: 1px solid var(--border); border-radius: var(--radius-sm); background: var(--bg); color: var(--text); font: inherit; }
	.brief-proposal :global(.proposal-field) { grid-column: 1 / -1; min-height: 78px; }
	.proposal-actions { grid-column: 1 / -1; display: flex; justify-content: flex-end; gap: var(--space-2); }
	h2 { font-size: var(--fs-lg); font-weight: 650; }
	.brief-editor label { display: block; margin: var(--space-4) 0 var(--space-2); font-size: var(--fs-sm); font-weight: 600; }
	.brief-editor :global(.brief-field) { width: 100%; min-height: 74px; resize: vertical; }
	.brief-editor :global(.goal-field) { min-height: 112px; }
	.brief-editor :global(.constraints-field) { min-height: 96px; }
	.editor-foot { padding: var(--space-3) 0; }
	.empty-state { max-width: 48ch; margin: 12vh auto 0; display: grid; justify-items: center; gap: var(--space-3); text-align: center; }
	.empty-state > :global(svg) { color: var(--text-faint); }
	.empty-state h2 { font-size: var(--fs-lg); }
	.empty-state p { color: var(--text-dim); }
	@media (max-width: 720px) {
		.investigation-layout { grid-template-columns: 1fr; gap: var(--space-4); }
		.investigation-list { max-height: 180px; overflow: auto; }
	}
</style>

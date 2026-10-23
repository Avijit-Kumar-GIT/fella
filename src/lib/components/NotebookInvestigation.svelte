<script lang="ts">
	import { untrack } from 'svelte';
	import { ipc } from '$lib/ipc';
	import { session } from '$lib/session.svelte';
	import { Button } from '$lib/components/ui/button';
	import { Badge } from '$lib/components/ui/badge';
	import type { Investigation, InvestigationQuestion } from '$lib/types';
	import Icon from './Icon.svelte';
	import InvestigationBriefView from './InvestigationBriefView.svelte';

	let { workspace, investigationId, conversationId, onopenquestion, onopenconversation }: {
		workspace: string;
		investigationId?: string;
		conversationId?: string;
		onopenquestion?: (questionId: string, turnId?: string) => void;
		onopenconversation?: () => void;
	} = $props();

	let record = $state<Investigation | null>(null);
	let loadedIdentity = $state('');
	let loading = $state(false);
	let error = $state('');
	let reload = $state(0);
	let editingTitle = $state(false);
	let editingGoal = $state(false);
	let titleDraft = $state('');
	let goalDraft = $state('');
	let saving = $state(false);
	let saveError = $state('');
	let request = 0;
	let settingsOpen = $state(false);
	let identity = $derived(JSON.stringify([workspace, investigationId ?? '', conversationId ?? '']));
	let current = $derived(loadedIdentity === identity ? record : null);
	let ownerConversation = $derived(current?.conversation_id ?? conversationId);
	let questions = $derived([...(current?.questions ?? [])].filter(question => question.status === 'active')
		.sort((a, b) => a.position - b.position || a.created_at_ms - b.created_at_ms || a.id.localeCompare(b.id)));
	let unfinished = $derived(questions.filter(question => question.answer_status !== 'answered'));
	let answeredCount = $derived(questions.filter(question => question.answer_status === 'answered').length);
	let attentionCount = $derived(questions.filter(question => [
		'incomplete', 'unsupported', 'insufficient_data', 'needs_review', 'waiting_for_input', 'blocked', 'outdated'
	].includes(question.answer_status)).length);

	$effect(() => {
		const key = identity, id = investigationId, path = workspace, chatId = conversationId;
		const version = session.investigationsVersion;
		const retry = reload;
		void version; void retry;
		const serial = ++request;
		if (untrack(() => loadedIdentity !== key)) {
			record = null;
			loadedIdentity = key;
			editingTitle = false; editingGoal = false; saving = false; saveError = '';
			settingsOpen = false;
		}
		error = '';
		if (!id || !path) { loading = false; return; }
		loading = untrack(() => !record);
		void ipc.investigationLoad(id).then(value => {
			if (serial !== request || key !== identity) return;
			if (value.id !== id || value.workspace_id !== path) throw new Error('This Investigation belongs to another workspace.');
			if (chatId && value.conversation_id && chatId !== value.conversation_id) throw new Error('This Investigation is linked to another conversation.');
			record = value;
		}).catch(cause => {
			if (serial === request && key === identity) error = describeError(cause);
		}).finally(() => {
			if (serial === request && key === identity) loading = false;
		});
		return () => { if (serial === request) request++; };
	});

	function describeError(cause: unknown): string {
		return cause instanceof Error ? cause.message : String(cause);
	}
	async function saveTitle(): Promise<void> {
		const owner = current, key = identity;
		if (!owner || saving || !titleDraft.trim()) return;
		saving = true; saveError = '';
		try {
			const updated = await ipc.investigationRename(owner.id, titleDraft.trim());
			if (key !== identity || updated.id !== owner.id || updated.workspace_id !== workspace) return;
			record = updated; editingTitle = false;
			session.rememberInvestigationNames([updated]); session.investigationsVersion++;
		} catch (cause) { if (key === identity) saveError = describeError(cause); }
		finally { if (key === identity) saving = false; }
	}
	async function saveGoal(): Promise<void> {
		const owner = current, key = identity;
		if (!owner || saving) return;
		saving = true; saveError = '';
		try {
			const updated = await ipc.investigationUpdateBrief(owner.id, { ...owner.brief, goal: goalDraft.trim(), constraints: [...owner.brief.constraints] });
			if (key !== identity || updated.id !== owner.id || updated.workspace_id !== workspace) return;
			record = updated; editingGoal = false; session.investigationsVersion++;
		} catch (cause) { if (key === identity) saveError = describeError(cause); }
		finally { if (key === identity) saving = false; }
	}
	function openConversation(): void {
		if (onopenconversation) onopenconversation();
		else document.querySelector<HTMLElement>('[role="combobox"][aria-label^="Ask"]')?.focus();
	}
	function openQuestion(question: InvestigationQuestion): void {
		const turnId = question.latest_analysis_turn_id;
		if (onopenquestion) { onopenquestion(question.id, turnId); return; }
		if (turnId && ownerConversation) {
			session.focusConversationAnalysis(ownerConversation, turnId);
			openConversation();
			return;
		}
		settingsOpen = true;
	}
	function questionStatus(question: InvestigationQuestion): string {
		return ({ not_started: 'Not started', running: 'Running', answered: 'Answered', incomplete: 'Incomplete', unsupported: 'Unsupported', insufficient_data: 'Insufficient data', needs_review: 'Needs review', waiting_for_input: 'Waiting', blocked: 'Blocked', outdated: 'Outdated' })[question.answer_status];
	}
	function questionStatusVariant(question: InvestigationQuestion): 'default' | 'success' | 'warning' | 'destructive' {
		if (question.answer_status === 'answered') return 'success';
		if (question.answer_status === 'blocked') return 'destructive';
		if (['incomplete', 'unsupported', 'insufficient_data', 'needs_review', 'waiting_for_input', 'outdated'].includes(question.answer_status)) return 'warning';
		return 'default';
	}
	function sameLabel(left: string | undefined, right: string | undefined): boolean {
		return Boolean(left?.trim()) && left!.trim().toLocaleLowerCase() === right?.trim().toLocaleLowerCase();
	}
</script>

<section class="notebook" aria-label="Investigation notebook" aria-busy={loading}>
	{#if settingsOpen && current}
		<Button variant="ghost" size="sm" onclick={() => settingsOpen = false}>Back to notebook</Button>
		<InvestigationBriefView viewKey={`notebook-settings:${workspace}:${current.id}`} />
	{:else if !investigationId}
		<div class="empty"><h1>Start with a question</h1><p>Ask in the conversation. Your Investigation and its saved analyses will take shape here.</p><Button variant="outline" onclick={openConversation}>Ask a question</Button></div>
	{:else if loading && !current}
		<p class="quiet" role="status">Loading Investigation…</p>
	{:else if error}
		<div class="empty" role="alert"><h1>Investigation unavailable</h1><p>{error}</p><Button variant="outline" onclick={() => reload++}>Retry</Button></div>
	{:else if current}
		<header class="notebook-header">
			<div class="notebook-actions"><Button variant="ghost" size="icon" aria-label="Investigation settings" title="Investigation settings" onclick={() => settingsOpen = true}><Icon name="sliders" size={14} /></Button></div>
			<div class="eyebrow">Investigation{#if current.status !== 'active'} · {current.status === 'complete' ? 'Closed' : 'Archived'}{/if}</div>
			{#if editingTitle}
				<form class="inline-edit" onsubmit={event => { event.preventDefault(); void saveTitle(); }}>
					<label class="sr-only" for="notebook-name">Investigation name</label><input id="notebook-name" aria-label="Investigation name" bind:value={titleDraft} maxlength="120" disabled={saving} />
					<div class="edit-actions"><Button size="sm" type="submit" disabled={saving || !titleDraft.trim()}>Save name</Button><Button size="sm" variant="ghost" disabled={saving} onclick={() => { editingTitle = false; saveError = ''; }}>Cancel</Button></div>
				</form>
			{:else}
				<div class="title-row"><h1>{current.title}</h1><Button variant="ghost" size="sm" aria-label="Rename Investigation" onclick={() => { titleDraft = current!.title; editingTitle = true; saveError = ''; }}><Icon name="pencil" size={14} /></Button></div>
			{/if}
			{#if editingGoal}
				<form class="inline-edit objective" onsubmit={event => { event.preventDefault(); void saveGoal(); }}>
					<label for="notebook-objective">Objective</label><textarea id="notebook-objective" aria-label="Investigation objective" bind:value={goalDraft} rows="3" disabled={saving}></textarea>
					<div class="edit-actions"><Button size="sm" type="submit" disabled={saving}>Save objective</Button><Button size="sm" variant="ghost" disabled={saving} onclick={() => { editingGoal = false; saveError = ''; }}>Cancel</Button></div>
				</form>
			{:else}
				{#if !sameLabel(current.brief.goal, current.title)}<div class="objective-row">{#if current.brief.goal}<p class="objective-copy">{current.brief.goal}</p>{/if}<Button variant="ghost" size="sm" class="objective-edit" onclick={() => { goalDraft = current!.brief.goal; editingGoal = true; saveError = ''; }}>{current.brief.goal ? 'Edit objective' : 'Add objective'}</Button></div>{/if}
			{/if}
			{#if saveError}<p class="inline-error" role="alert">{saveError}</p>{/if}
		</header>
		<section class="question-outline" aria-label="Investigation Questions">
			<div class="questions-heading">
				<h2>Questions</h2>
				<p>{questions.length ? `${answeredCount} of ${questions.length} answered` : 'No Questions yet'}{#if attentionCount}<span> · {attentionCount} need attention</span>{/if}</p>
			</div>
			{#if questions.length}
				<ol class="question-list">
					{#each questions as question (question.id)}
						<li class:subquestion={!!question.parent_id} data-question-id={question.id}>
							<Button variant="ghost" class="question-link" aria-label={question.latest_analysis_turn_id ? `Open answer for ${question.text}` : `Open Question settings for ${question.text}`} onclick={() => openQuestion(question)}>
								<span class="question-copy">{question.text}</span>
								<Badge variant={questionStatusVariant(question)}>{questionStatus(question)}</Badge>
							</Button>
							{#if question.answer_status_reason && question.answer_status !== 'answered' && question.answer_status !== 'not_started'}
								<p class="question-reason" title={question.answer_status_reason}>{question.answer_status_reason}</p>
							{/if}
						</li>
					{/each}
				</ol>
			{:else}
				<div class="empty-questions">
					<p>Add Questions to track the analysis here.</p>
					<Button variant="outline" size="sm" onclick={() => settingsOpen = true}>Set up Questions</Button>
				</div>
			{/if}
			{#if current.status === 'complete' && unfinished.length}<p class="closed-note">Closed with {unfinished.length} unanswered or incomplete {unfinished.length === 1 ? 'Question' : 'Questions'}.</p>{/if}
		</section>
	{/if}
</section>

<style>
	.notebook-actions { position: absolute; top: 16px; right: 16px; display: flex; }
	.notebook { position: relative; display:flex; flex:1; flex-direction:column; width:100%; max-width:620px; min-width:0; min-height:0; margin-inline:auto; padding:24px; overflow-y:auto; overscroll-behavior-y:contain; scrollbar-width:thin; color:var(--text); }
	.notebook-header { margin-bottom: 24px; padding-right: 42px; }
	.eyebrow { color: var(--text-faint); font-size: var(--fs-xs); font-weight: 500; }
	.title-row { display: flex; align-items: center; gap: 6px; margin-top: 8px; }
	h1 { min-width:0; margin: 0; font-size: clamp(21px, 2.4vw, 28px); font-weight: 580; letter-spacing: -.025em; line-height: 1.25; overflow-wrap: anywhere; }
	h2 { margin: 0; font-size: var(--fs); font-weight: 600; line-height: 1.3; }
	.objective-row { margin-top: 12px; }
	.objective-copy { display:-webkit-box; max-width:52ch; overflow:hidden; -webkit-box-orient:vertical; line-clamp:3; -webkit-line-clamp:3; color:var(--text-dim); font-size:var(--fs-sm); line-height:1.5; white-space:pre-wrap; margin:0 0 4px; overflow-wrap:anywhere; }
	.objective-row :global(.objective-edit) { color: var(--text-faint); font-size: var(--fs-xs); padding-inline: 0; }
	.inline-edit { display: grid; gap: 10px; margin-top: 12px; max-width: 520px; }
	.inline-edit input, .inline-edit textarea { width: 100%; box-sizing: border-box; color: var(--text); background: var(--bg-inset); border: 1px solid var(--border); border-radius: var(--radius-control, 10px); padding: 10px 12px; font: inherit; line-height: 1.5; }
	.inline-edit textarea { resize: vertical; min-height: 92px; }
	.inline-edit label { color: var(--text-dim); font-size: var(--fs-sm); }
	.edit-actions { display: flex; align-items: center; gap: 8px; }
	.question-outline { min-width:0; color:var(--text); }
	.questions-heading { display:flex; align-items:baseline; justify-content:space-between; gap:12px; margin-bottom:10px; }
	.questions-heading p { flex:none; margin:0; color:var(--text-faint); font-size:var(--fs-xs); }
	.questions-heading p span { color:var(--warn); }
	.question-list { display:grid; gap:4px; list-style:none; margin:0; padding:0; }
	.question-list li { min-width:0; }
	.question-list li.subquestion { margin-left:14px; }
	:global(.question-link) { display:flex; width:100%; min-width:0; height:auto; min-height:38px; justify-content:space-between; align-items:flex-start; gap:10px; padding:8px 9px; border-radius:var(--radius-control); white-space:normal; text-align:left; }
	.question-copy { min-width:0; flex:1; color:var(--text); font-size:var(--fs-sm); font-weight:450; line-height:1.45; overflow-wrap:anywhere; }
	:global(.question-link [data-slot='badge']) { flex:none; margin-top:1px; font-size:10px; }
	.question-reason { margin:0 10px 6px; color:var(--text-faint); font-size:var(--fs-xs); line-height:1.45; display:-webkit-box; overflow:hidden; -webkit-box-orient:vertical; line-clamp:2; -webkit-line-clamp:2; }
	.empty-questions { display:grid; justify-items:start; gap:8px; padding:8px 0; }
	.empty-questions p { margin:0; color:var(--text-dim); font-size:var(--fs-sm); }
	.closed-note { margin:12px 0 0; color:var(--warn); font-size:var(--fs-xs); }
	.empty { max-width: 520px; padding-block: 32px; }
	.empty h1 { font-size: 24px; }
	.empty p, .quiet { color: var(--text-dim); font-size: var(--fs-sm); line-height: 1.55; }
	.inline-error { color: var(--danger, #bd5b5b); font-size: var(--fs-sm); line-height: 1.6; }
	.sr-only { position: absolute; width: 1px; height: 1px; padding: 0; overflow: hidden; clip-path: inset(50%); white-space: nowrap; }
	@media (max-width: 600px) { .notebook { padding:18px 14px; } .notebook-header { margin-bottom:20px; } }
</style>

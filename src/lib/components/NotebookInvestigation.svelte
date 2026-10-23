<script lang="ts">
	import { onDestroy, untrack } from 'svelte';
	import { ipc } from '$lib/ipc';
	import { session } from '$lib/session.svelte';
	import { ensureAnalyticalOutput } from '$lib/analytical-output.svelte';
	import { Button } from '$lib/components/ui/button';
	import type { AnalysisTurn, EvidenceItem, Investigation, InvestigationQuestion, Message } from '$lib/types';
	import AnalyticalAnswer from './AnalyticalAnswer.svelte';
	import MarkdownContent from './MarkdownContent.svelte';
	import EvidenceBlock from './EvidenceBlock.svelte';
	import { answerNeedsReview } from './EvidenceSummary.svelte';
	import Icon from './Icon.svelte';
	import InvestigationBriefView from './InvestigationBriefView.svelte';

	let { workspace, investigationId, conversationId, onopenquestion, onopenconversation }: {
		workspace: string;
		investigationId?: string;
		conversationId?: string;
		onopenquestion?: (questionId: string) => void;
		onopenconversation?: () => void;
	} = $props();

	interface Output {
		id: string;
		message?: Message;
		question: string;
		loading: boolean;
		ready: boolean;
		error: string;
	}

	// Deliberately no ask, rerun, SQL, or suggestion API.
	// The shared resolver may persist the existing output link/cache, but can
	// neither call a model nor rerun analysis through this bridge.
	const savedOutputBridge = {
		presentationResults: ipc.presentationResults,
		artifactList: ipc.artifactList,
		artifactCreate: ipc.artifactCreate,
		artifactResolve: ipc.artifactResolve,
		artifactRevise: ipc.artifactRevise
	};
	let record = $state<Investigation | null>(null);
	let loadedIdentity = $state('');
	let loading = $state(false);
	let error = $state('');
	let reload = $state(0);
	let outputs = $state<Record<string, Output>>({});
	let editingTitle = $state(false);
	let editingGoal = $state(false);
	let titleDraft = $state('');
	let goalDraft = $state('');
	let saving = $state(false);
	let saveError = $state('');
	let request = 0;
	let outputRequest = 0;
	let settingsOpen = $state(false);
	const pending = new Map<string, Promise<void>>();
	onDestroy(() => { request++; outputRequest++; pending.clear(); });
	let identity = $derived(JSON.stringify([workspace, investigationId ?? '', conversationId ?? '']));
	let current = $derived(loadedIdentity === identity ? record : null);
	let ownerConversation = $derived(current?.conversation_id ?? conversationId);
	let linkedChat = $derived(session.conversations.find(chat => chat.id === ownerConversation && chat.workspaceScope === workspace));
	let liveMessages = $derived((linkedChat?.messages ?? []).filter(message =>
		message.role === 'assistant' && !message.pending && message.answer?.turn_id
		&& message.answer.workspace?.path === workspace));
	let questions = $derived([...(current?.questions ?? [])].filter(question => question.status === 'active')
		.sort((a, b) => a.position - b.position || a.created_at_ms - b.created_at_ms || a.id.localeCompare(b.id)));
	let unfinished = $derived(questions.filter(question => question.answer_status !== 'answered'));
	let runIds = $derived.by(() => {
		if (!current) return [];
		return [...new Set([
			...(current.analysis_turn_ids ?? []),
			...questions.flatMap(question => [...(question.analysis_turn_ids ?? []), ...(question.latest_analysis_turn_id ? [question.latest_analysis_turn_id] : [])]),
			...liveMessages.map(message => message.answer!.turn_id!)
		])];
	});
	let hasPendingAnswer = $derived(Boolean(linkedChat?.busy && linkedChat.messages.some(message => message.role === 'assistant' && message.pending)));

	$effect(() => {
		const key = identity, id = investigationId, path = workspace, chatId = conversationId;
		const version = session.investigationsVersion;
		const retry = reload;
		void version; void retry;
		const serial = ++request;
		if (untrack(() => loadedIdentity !== key)) {
			outputRequest++;
			outputs = {};
			pending.clear();
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

	$effect(() => {
		const key = identity;
		const ids = runIds;
		const messages = liveMessages;
		// An unchanged Message keeps the same presentation cache and local state
		// as chat. A completed answer becoming available must not require reload.
		for (const id of ids) {
			const live = messages.find(message => message.answer?.turn_id === id);
			untrack(() => {
				if (!outputs[id] || (live && outputs[id].message !== live)) void loadOutput(id, key, live);
			});
		}
	});

	function describeError(cause: unknown): string {
		return cause instanceof Error ? cause.message : String(cause);
	}
	function persistedEvidence(value: unknown): value is EvidenceItem {
		return !!value && typeof value === 'object' && !Array.isArray(value)
			&& typeof (value as EvidenceItem).tool === 'string'
			&& typeof (value as EvidenceItem).result_summary === 'string';
	}
	function projectRun(run: AnalysisTurn): Message {
		return {
			id: `notebook_${run.id}`, role: 'assistant', text: run.result.text, ts: 0,
			answer: {
				turn_id: run.id, trace: run.trace, plan: run.plan, contract: run.contract, provenance: run.provenance,
				text: run.result.text, presentation_spec: run.result.presentation_spec,
				evidence: run.result.evidence.filter(persistedEvidence),
				verification: run.verification?.checks ?? run.result.verification ?? [],
				status: run.result.status, usage: run.result.usage,
				workspace: { path: workspace, revision: run.workspace_revision ?? '' }
			}
		};
	}
	function titleForRun(id: string, run?: AnalysisTurn): string {
		const capturedQuestion = run?.investigation_context?.question_text?.trim();
		if (capturedQuestion) return capturedQuestion;
		const linkedQuestion = (run?.investigation_context?.question_id
			? questions.find(question => question.id === run.investigation_context?.question_id)
			: undefined)
			?? questions.find(question => question.latest_analysis_turn_id === id || question.analysis_turn_ids?.includes(id));
		if (linkedQuestion) return linkedQuestion.text;
		if (run?.question.trim()) return run.question.trim();
		return 'Saved analysis';
	}

	async function loadOutput(id: string, key: string, live?: Message): Promise<void> {
		const pendingKey = `${key}:${id}`;
		if (pending.has(pendingKey)) return;
		const generation = outputRequest;
		const path = workspace;
		outputs = { ...outputs, [id]: { id, message: live, question: titleForRun(id), loading: true, ready: false, error: '' } };
		const work = (async () => {
			try {
				let run: AnalysisTurn | undefined;
				if (!live) {
					run = await ipc.analysisTurnLoad(id);
					if (run.id !== id || run.workspace !== path) throw new Error('This saved analysis is outside the selected workspace.');
				}
				if (key !== identity || generation !== outputRequest) return;
				const message = live ?? projectRun(run!);
				const question = titleForRun(id, run);
				outputs = { ...outputs, [id]: { id, message, question, loading: false, ready: false, error: '' } };
				// No repair is possible through savedOutputBridge. A bad historical
				// layout stays a readable saved answer, not a hidden model request.
				const resolved = await ensureAnalyticalOutput(message, path, savedOutputBridge, () => {
					if (live && key === identity && generation === outputRequest) session.persist();
				});
				if (key !== identity || generation !== outputRequest) return;
				outputs = { ...outputs, [id]: { id, message, question: resolved ? titleForRun(id, resolved.run) : question, loading: false, ready: !!resolved,
					error: resolved ? '' : 'The saved layout is unavailable. The answer and its recorded evidence remain below.' } };
			} catch (cause) {
				if (key !== identity || generation !== outputRequest) return;
				const previous = outputs[id];
				outputs = { ...outputs, [id]: { id, message: previous?.message, question: previous?.question ?? titleForRun(id), loading: false, ready: false, error: describeError(cause) } };
			} finally { if (generation === outputRequest) pending.delete(pendingKey); }
		})();
		pending.set(pendingKey, work);
		await work;
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
		if (onopenquestion) { onopenquestion(question.id); return; }
		if (question.latest_analysis_turn_id) {
			const output = [...document.querySelectorAll<HTMLElement>('.notebook-output')].find(element => element.dataset.analysisTurnId === question.latest_analysis_turn_id);
			if (output) { output.scrollIntoView({ block: 'nearest' }); return; }
		}
		settingsOpen = true;
	}
	function questionStatus(question: InvestigationQuestion): string {
		return ({ not_started: 'Not started', running: 'Running', answered: 'Answered', incomplete: 'Partial coverage', unsupported: 'Unsupported', insufficient_data: 'Insufficient data', needs_review: 'Needs review', waiting_for_input: 'Waiting for input', blocked: 'Run blocked', outdated: 'Outdated' })[question.answer_status];
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
			<div class="notebook-actions"><Button variant="ghost" size="sm" onclick={() => settingsOpen = true}>Investigation settings</Button></div>
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
			{#if questions.length}
				<details class="question-outline"><summary>Questions <span>{questions.length}{#if unfinished.length} · {unfinished.length} open{/if}</span></summary>
					<ol>{#each questions as question (question.id)}<li class:subquestion={!!question.parent_id}>
						<button class="question-link" onclick={() => openQuestion(question)}>{question.text}</button>
						<span class="question-status">{questionStatus(question)}</span>{#if question.answer_status_reason}<p class="question-reason">{question.answer_status_reason}</p>{/if}
					</li>{/each}</ol>
				</details>
			{/if}
			{#if current.status === 'complete' && unfinished.length}<p class="quiet">Closed with {unfinished.length} unanswered or incomplete {unfinished.length === 1 ? 'Question' : 'Questions'}. Coverage details remain in the outline.</p>{/if}
		</header>
		<div class="notebook-results" aria-label="Saved analytical results">
			{#each runIds as id, index (id)}
				{@const output = outputs[id]}
				<article class="notebook-output" aria-label={`Analysis ${index + 1}`} data-analysis-turn-id={id}>
					<header><span class="eyebrow">Analysis {index + 1}</span>{#if !sameLabel(output?.question, current.title) && !sameLabel(output?.question, current.brief.goal)}<h2>{output?.question ?? titleForRun(id)}</h2>{/if}</header>
					{#if output?.message?.answer}
						{@const answer = output.message.answer}
						{#if answer.status === 'failed'}<p class="inline-error" role="status">Verification failed. Review the recorded checks before relying on this answer.</p>
						{:else if answer.status === 'insufficient_data'}<p class="quiet" role="status">Insufficient evidence. The saved response remains available.</p>
						{:else if answerNeedsReview(answer)}<p class="quiet" role="status">Review the recorded checks or unresolved interpretation.</p>{/if}
						{#if output.ready}<AnalyticalAnswer message={output.message} />{:else}<MarkdownContent text={answer.text} />{/if}
						{#if output.error}<p class="quiet" role="status">{output.error}</p>{/if}
						<details class="evidence"><summary>Evidence and checks</summary><EvidenceBlock {answer} bodyId={`notebook-evidence-${current.id}-${id}`} expanded={true} /></details>
					{:else if output?.error}<p class="inline-error" role="status">{output.error}</p><Button variant="ghost" size="sm" onclick={() => void loadOutput(id, identity)}>Retry saved analysis</Button>
					{:else}<p class="quiet" role="status">Loading saved analysis…</p>{/if}
				</article>
			{/each}
			{#if hasPendingAnswer}<p class="quiet" role="status">Fella is working on the next answer.</p>{:else if !runIds.length}<div class="empty-results"><p>Your analytical results will appear here as the conversation develops.</p><Button variant="ghost" onclick={openConversation}>Continue in conversation</Button></div>{/if}
		</div>
	{/if}
</section>

<style>
	.notebook-actions { display: flex; justify-content: flex-end; margin-bottom: 8px; }
	.notebook { display:flex; flex:1; flex-direction:column; width:100%; max-width:1040px; min-width:0; min-height:0; margin-inline:auto; padding:clamp(20px, 4vw, 52px); overflow-y:auto; overscroll-behavior-y:contain; scrollbar-width:thin; color:var(--text); }
	.notebook-header { margin-bottom: 44px; }
	.eyebrow { color: var(--text-faint); font-size: var(--fs-xs); font-weight: 500; }
	.title-row { display: flex; align-items: baseline; gap: 8px; margin-top: 10px; }
	h1 { margin: 0; font-size: clamp(24px, 3vw, 34px); font-weight: 580; letter-spacing: -.025em; line-height: 1.25; overflow-wrap: anywhere; }
	h2 { margin: 8px 0 22px; font-size: clamp(18px, 2vw, 23px); font-weight: 550; letter-spacing: -.015em; line-height: 1.4; overflow-wrap: anywhere; }
	.objective-row { margin-top: 15px; }
	.objective-copy { color: var(--text-dim); font-size: var(--fs); line-height: 1.65; white-space: pre-wrap; margin: 0 0 4px; overflow-wrap: anywhere; }
	.objective-row :global(.objective-edit) { color: var(--text-faint); font-size: var(--fs-xs); padding-inline: 0; }
	.inline-edit { display: grid; gap: 10px; margin-top: 12px; max-width: 720px; }
	.inline-edit input, .inline-edit textarea { width: 100%; box-sizing: border-box; color: var(--text); background: var(--bg-inset); border: 1px solid var(--border); border-radius: var(--radius-control, 10px); padding: 10px 12px; font: inherit; line-height: 1.5; }
	.inline-edit textarea { resize: vertical; min-height: 92px; }
	.inline-edit label { color: var(--text-dim); font-size: var(--fs-sm); }
	.edit-actions { display: flex; align-items: center; gap: 8px; }
	.question-outline { margin-top: 22px; color: var(--text-dim); font-size: var(--fs-sm); }
	summary { width: fit-content; cursor: pointer; color: var(--text-faint); font-size: var(--fs-xs); line-height: 1.6; }
	.question-outline summary { color: var(--text-dim); font-size: var(--fs-sm); }
	.question-outline summary span { margin-left: 6px; color: var(--text-faint); font-size: var(--fs-xs); }
	.question-outline ol { display: grid; gap: 14px; margin: 18px 0 0; padding-left: 20px; }
	.question-outline li { padding-left: 3px; }
	.question-outline li.subquestion { margin-left: 12px; }
	.question-link { display: inline; padding: 0; border: 0; background: none; color: var(--text); font: inherit; text-align: left; line-height: 1.5; cursor: pointer; }
	.question-link:hover:not(:disabled) { text-decoration: underline; text-underline-offset: 3px; }
	.question-link:disabled { cursor: default; }
	.question-status { display: block; color: var(--text-faint); font-size: var(--fs-xs); margin-top: 3px; }
	.question-reason { color: var(--text-dim); font-size: var(--fs-xs); line-height: 1.5; margin: 4px 0 0; }
	.notebook-results { display: grid; gap: 52px; }
	.notebook-output { min-width: 0; }
	.evidence { margin-top: 16px; }
	.empty { max-width: 520px; padding-block: 32px; }
	.empty h1 { font-size: 24px; }
	.empty p, .empty-results p, .quiet { color: var(--text-dim); font-size: var(--fs-sm); line-height: 1.65; }
	.empty-results { padding-block: 12px 32px; }
	.inline-error { color: var(--danger, #bd5b5b); font-size: var(--fs-sm); line-height: 1.6; }
	.sr-only { position: absolute; width: 1px; height: 1px; padding: 0; overflow: hidden; clip-path: inset(50%); white-space: nowrap; }
	@media (max-width: 600px) { .notebook { padding: 20px 16px; } .notebook-results { gap: 36px; } .notebook-header { margin-bottom: 32px; } }
</style>

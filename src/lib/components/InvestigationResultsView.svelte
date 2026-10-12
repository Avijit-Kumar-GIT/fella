<script lang="ts">
	import { untrack } from 'svelte';
	import { ipc } from '$lib/ipc';
	import { isRenderableChartEvidence } from '$lib/workbench';
	import type {
		AnalysisTurn,
		EvidenceItem,
		Investigation,
		InvestigationFinding,
		InvestigationFindingRevision,
		InvestigationQuestion,
		Message,
		VerificationCheck
	} from '$lib/types';
	import { runStateLabel } from '$lib/run-progress';
	import { Button } from '$lib/components/ui/button';
	import { Badge } from '$lib/components/ui/badge';
	import Chart from './Chart.svelte';
	import Icon from './Icon.svelte';

	let {
		investigation,
		conversationMessages = [],
		hasUnsavedEdits = false,
		onopenrun,
		onopenlegacy,
		onreviewquestion
	}: {
		investigation: Investigation;
		conversationMessages?: Message[];
		hasUnsavedEdits?: boolean;
		onopenrun: (turnId: string, evidenceId?: string) => void;
		onopenlegacy: (turnId?: string) => void;
		onreviewquestion: (questionId: string) => void;
	} = $props();

	let runs = $state<Record<string, AnalysisTurn>>({});
	let loadingRunIds = $state<string[]>([]);
	let runErrors = $state<Record<string, string>>({});
	let archivedConversationId = $state('');
	let archivedConversationMessages = $state<Message[]>([]);
	let findingCopyStatus = $state<Record<string, string>>({});
	let loadedInvestigationId = '';
	let generation = 0;
	let transcriptMessages = $derived(
		conversationMessages.length
			? conversationMessages
			: archivedConversationId === investigation.conversation_id
				? archivedConversationMessages
				: []
	);

	let activeQuestions = $derived((investigation.questions ?? []).filter((question) => question.status === 'active'));
	let unresolvedQuestions = $derived(activeQuestions.filter((question) => question.answer_status !== 'answered'));
	let primaryRunIds = $derived.by(() => {
		const ids = new Set<string>();
		for (const question of activeQuestions) {
			if (question.latest_analysis_turn_id) ids.add(question.latest_analysis_turn_id);
		}
		for (const finding of investigation.findings ?? []) ids.add(finding.analysis_turn_id);
		return [...ids];
	});

	$effect(() => {
		const conversationId = investigation.conversation_id;
		if (!conversationId || conversationMessages.length || archivedConversationId === conversationId) return;
		archivedConversationId = conversationId;
		void ipc.conversationLoad(conversationId).then((raw) => {
			if (investigation.conversation_id !== conversationId) return;
			const saved: { messages?: unknown } = JSON.parse(raw);
			archivedConversationMessages = Array.isArray(saved.messages) ? saved.messages as Message[] : [];
		}).catch(() => {
			// The current Results view can still provide its loaded conversation messages.
			if (investigation.conversation_id === conversationId) archivedConversationMessages = [];
		});
	});

	let transcriptRuns = $derived.by(() => {
		const messages = transcriptMessages;
		const sourceId = investigation.brief_source_message_id;
		if (!sourceId) return [];
		const sourceIndex = messages.findIndex((message) => message.id === sourceId);
		if (sourceIndex < 0) return [];
		let prompt = messages[sourceIndex].role === 'user' ? messages[sourceIndex].text : '';
		const seen = new Set<string>();
		const runs: { turnId: string; prompt: string; answer: string }[] = [];
		for (const message of messages.slice(sourceIndex + 1)) {
			if (message.role === 'user') {
				prompt = message.text;
				continue;
			}
			if (message.role !== 'assistant') continue;
			const turnId = message.answer?.turn_id?.trim();
			if (!turnId || seen.has(turnId)) continue;
			if (message.answer?.workspace?.path !== investigation.workspace_id) continue;
			seen.add(turnId);
			runs.push({
				turnId,
				prompt,
				answer: message.answer?.text || message.text
			});
		}
		return runs;
	});
	let runIds = $derived.by(() => {
		const ids = new Set(investigation.analysis_turn_ids ?? []);
		for (const question of investigation.questions ?? []) {
			if (question.latest_analysis_turn_id) ids.add(question.latest_analysis_turn_id);
			for (const turnId of question.analysis_turn_ids ?? []) ids.add(turnId);
		}
		for (const finding of investigation.findings ?? []) ids.add(finding.analysis_turn_id);
		return [...ids];
	});
	let historicalRunIds = $derived.by(() => {
		const current = new Set(primaryRunIds);
		const ids = new Set(runIds);
		for (const transcript of transcriptRuns) ids.add(transcript.turnId);
		return [...ids].filter((turnId) => !current.has(turnId));
	});
	let answeredCount = $derived(activeQuestions.filter((question) => question.answer_status === 'answered').length);
	let unavailableRunIds = $derived(runIds.filter((turnId) => runErrors[turnId] && !transcriptRunFor(turnId)));
	let chartCards = $derived.by(() => {
		const cards: {
			key: string;
			run: AnalysisTurn;
			evidence: EvidenceItem;
			sourceEvidence: EvidenceItem | null;
			question: InvestigationQuestion | undefined;
		}[] = [];
		for (const turnId of primaryRunIds) {
			const run = runs[turnId];
			if (!run) continue;
			const evidence = run.result.evidence.filter(isEvidenceItem);
			for (const [index, item] of evidence.entries()) {
				if (!isRenderableChartEvidence(item, run.verification?.checks ?? run.result.verification)) continue;
				const sourceId = item.chart?.metadata?.source_evidence_id;
				cards.push({
					key: `${turnId}:${item.id ?? index}`,
					run,
					evidence: item,
					sourceEvidence: sourceId ? evidence.find((candidate) => candidate.id === sourceId) ?? null : null,
					question: activeQuestions.find((question) => question.latest_analysis_turn_id === turnId)
						?? (investigation.findings ?? []).map((finding) => finding.question_id)
							.map((questionId) => investigation.questions?.find((question) => question.id === questionId))
							.find((question) => question && (question.latest_analysis_turn_id === turnId || question.analysis_turn_ids?.includes(turnId)))
				});
			}
		}
		return cards;
	});

	$effect(() => {
		const investigationId = investigation.id;
		const ids = runIds;
		if (loadedInvestigationId !== investigationId) {
			loadedInvestigationId = investigationId;
			generation += 1;
			runs = {};
			loadingRunIds = [];
			runErrors = {};
		}
		void loadMissingRuns(investigationId, ids, generation);
	});

	function isEvidenceItem(value: unknown): value is EvidenceItem {
		if (!value || typeof value !== 'object' || Array.isArray(value)) return false;
		const item = value as Record<string, unknown>;
		return typeof item.tool === 'string'
			&& Boolean(item.args && typeof item.args === 'object' && !Array.isArray(item.args))
			&& typeof item.result_summary === 'string';
	}

	function transcriptRunFor(turnId: string) {
		return transcriptRuns.find((run) => run.turnId === turnId);
	}

	function historicalChartEvidence(run: AnalysisTurn): EvidenceItem[] {
		return run.result.evidence
			.filter(isEvidenceItem)
			.filter((item) => isRenderableChartEvidence(item, run.verification?.checks ?? run.result.verification));
	}

	function historicalRunTitle(run: AnalysisTurn | undefined, transcript: ReturnType<typeof transcriptRunFor>, turnId: string): string {
		return run?.question?.trim() || transcript?.prompt?.trim() || `Analysis run ${turnId}`;
	}

	function historicalRunOutcome(run: AnalysisTurn): string {
		const status = run.verification?.status ?? run.result.status;
		return {
			verified: 'Checked',
			needs_review: 'Needs review',
			insufficient_data: 'Insufficient data',
			failed: 'Run failed'
		}[status] ?? runStateLabel(run.state) ?? 'Outcome not recorded';
	}

	function chartSourceEvidence(run: AnalysisTurn, chart: EvidenceItem): EvidenceItem | null {
		const sourceId = chart.chart?.metadata?.source_evidence_id;
		if (!sourceId) return null;
		return run.result.evidence.filter(isEvidenceItem).find((item) => item.id === sourceId) ?? null;
	}

	async function loadMissingRuns(investigationId: string, ids: string[], requestGeneration: number): Promise<void> {
		const existingRuns = untrack(() => runs);
		const existingLoading = untrack(() => loadingRunIds);
		const pending = ids.filter((id) => !existingRuns[id] && !existingLoading.includes(id));
		if (!pending.length) return;
		loadingRunIds = [...new Set([...existingLoading, ...pending])];
		const loaded = await Promise.all(pending.map(async (turnId) => {
			try {
				return { turnId, run: await ipc.analysisTurnLoad(turnId), error: '' };
			} catch (cause) {
				return { turnId, run: null, error: cause instanceof Error ? cause.message : String(cause) };
			}
		}));
		if (requestGeneration !== generation || investigationId !== investigation.id) return;
		const nextRuns = { ...runs };
		const nextErrors = { ...runErrors };
		for (const result of loaded) {
			if (result.run) {
				nextRuns[result.turnId] = result.run;
				delete nextErrors[result.turnId];
			} else {
				nextErrors[result.turnId] = result.error;
			}
		}
		runs = nextRuns;
		runErrors = nextErrors;
		loadingRunIds = loadingRunIds.filter((id) => !pending.includes(id));
	}

	function retryRun(turnId: string): void {
		const nextErrors = { ...runErrors };
		delete nextErrors[turnId];
		runErrors = nextErrors;
		void loadMissingRuns(investigation.id, [turnId], generation);
	}

	function questionStatusLabel(question: InvestigationQuestion): string {
		return {
			not_started: 'Not answered',
			running: 'Running',
			answered: 'Answered',
			incomplete: 'Partial answer',
			unsupported: 'Unsupported',
			blocked: 'Blocked',
			outdated: 'Outdated'
		}[question.answer_status];
	}

	function questionReason(question: InvestigationQuestion): string {
		if (question.answer_status_reason?.trim()) return question.answer_status_reason;
		return {
			not_started: 'No analysis run is linked to this Question yet.',
			running: 'Analysis is still running.',
			answered: 'This Question has a complete, current answer.',
			incomplete: 'The latest run returned a partial answer.',
			unsupported: 'The available data or analysis tools do not support this Question.',
			blocked: 'The latest run could not produce a complete answer.',
			outdated: 'The Question or its analysis context changed after the latest run.'
		}[question.answer_status];
	}

	function questionForFinding(questionId: string): InvestigationQuestion | undefined {
		return investigation.questions?.find((question) => question.id === questionId);
	}

	function findingsForChart(card: {
		run: AnalysisTurn;
		evidence: EvidenceItem;
		sourceEvidence: EvidenceItem | null;
	}): InvestigationFinding[] {
		const sourceEvidenceId = card.evidence.chart?.metadata?.source_evidence_id;
		const evidenceIds = new Set(
			[card.evidence.id, sourceEvidenceId, card.sourceEvidence?.id]
				.filter((id): id is string => Boolean(id))
		);
		return (investigation.findings ?? []).filter((finding) =>
			finding.analysis_turn_id === card.run.id
				&& finding.evidence_ids.some((id) => evidenceIds.has(id))
		);
	}

	async function copyFindingClaim(finding: InvestigationFinding): Promise<void> {
		try {
			if (!navigator.clipboard?.writeText) throw new Error('Clipboard access is unavailable.');
			await navigator.clipboard.writeText(finding.claim);
			findingCopyStatus = { ...findingCopyStatus, [finding.id]: 'Finding claim copied.' };
		} catch {
			findingCopyStatus = { ...findingCopyStatus, [finding.id]: 'Could not copy the Finding claim.' };
		}
	}

	function findingEvidenceItem(finding: InvestigationFinding, evidenceId: string): EvidenceItem | undefined {
		const run = runs[finding.analysis_turn_id];
		return run?.result.evidence
			.filter(isEvidenceItem)
			.find((item) => item.id === evidenceId);
	}

	function findingChecks(finding: InvestigationFinding): VerificationCheck[] {
		const run = runs[finding.analysis_turn_id];
		return run?.verification?.checks ?? run?.result.verification ?? [];
	}

	function verificationOutcome(check: VerificationCheck): string {
		const outcome = check.outcome ?? (check.ok ? 'passed' : 'flagged');
		return outcome === 'passed' ? 'Passed' : outcome === 'not_run' ? 'Not run' : 'Flagged';
	}

	function verificationStatusLabel(run: AnalysisTurn | undefined): string {
		if (!run) return 'Run not loaded';
		const status = run.verification?.status ?? run.result.status;
		return {
			verified: 'Checked',
			needs_review: 'Needs review',
			insufficient_data: 'Insufficient data',
			failed: 'Run failed'
		}[status] ?? 'Status not recorded';
	}

	function revisionQuestionLabel(revision: InvestigationFindingRevision): string {
		return questionForFinding(revision.question_id)?.text ?? `Question ${revision.question_id} (not in the current plan)`;
	}

	function evidenceSourceLabel(item: EvidenceItem): string {
		return item.sources?.map((source) => source.table ? `${source.source} · ${source.table}` : source.source).join(', ')
			|| 'Source not recorded';
}

	function sourceLabel(item: EvidenceItem): string {
		return item.chart?.metadata?.source_label
			|| item.sources?.map((source) => source.source).filter(Boolean).join(', ')
			|| 'Workspace data';
	}
</script>

<section class="results-view" aria-label="Current Investigation results" data-investigation-id={investigation.id}>
	{#if hasUnsavedEdits}
		<p class="results-warning" role="status"><Icon name="info" size={14} /> These results reflect the last saved brief, Question plan, and definitions. Save edits in Brief to include them.</p>
	{/if}
	{#if unavailableRunIds.length}
		<div class="results-run-errors" role="alert">
			<p>{unavailableRunIds.length} linked run{unavailableRunIds.length === 1 ? '' : 's'} could not be loaded. Answers and visualizations may be incomplete.</p>
			{#each unavailableRunIds as turnId (turnId)}<Button type="button" variant="ghost" size="sm" onclick={() => retryRun(turnId)}>Retry {turnId}</Button>{/each}
		</div>
	{/if}
	<section class="results-overview" aria-labelledby="results-goal-title">
		<div class="results-overview-heading">
			<div>
				<p class="results-eyebrow">{investigation.status === 'complete' ? 'Completed Investigation' : 'Current Investigation'}</p>
				<h3 id="results-goal-title">{investigation.brief.goal || investigation.title}</h3>
			</div>
			<Badge variant={unresolvedQuestions.length ? 'warning' : 'success'}>
				{activeQuestions.length ? `${answeredCount}/${activeQuestions.length} Questions answered` : 'No active Questions'}
			</Badge>
		</div>
		<dl class="results-brief-facts">
			{#if investigation.brief.audience.trim()}
				<div><dt>For</dt><dd>{investigation.brief.audience}</dd></div>
			{/if}
			{#if investigation.brief.desired_output.trim()}
				<div><dt>Deliverable</dt><dd>{investigation.brief.desired_output}</dd></div>
			{/if}
		</dl>
		{#if investigation.brief.constraints.length}
			<details class="results-supporting">
				<summary>Constraints and scope</summary>
				<ul>{#each investigation.brief.constraints as constraint (constraint)}<li>{constraint}</li>{/each}</ul>
			</details>
		{/if}
	</section>

	<section class="results-section" aria-labelledby="results-findings-title">
		<header class="results-section-heading">
			<div><h3 id="results-findings-title">Key Findings</h3><p>Analyst-reviewed conclusions, linked to their Question and evidence.</p></div>
			<span>{investigation.findings?.length ?? 0}</span>
		</header>
		{#if investigation.findings?.length}
			<div class="results-findings">
				{#each investigation.findings as finding, index (finding.id)}
					{@const question = questionForFinding(finding.question_id)}
					{@const run = runs[finding.analysis_turn_id]}
					{@const checks = findingChecks(finding)}
					<article class="results-finding" id="finding-{finding.id}" aria-label="Finding {index + 1}" data-artifact-kind="reviewed-finding">
						<div class="results-finding-heading">
							<span>Analyst-reviewed Finding · Revision {finding.revision}</span>
							<Button type="button" variant="ghost" size="sm" aria-label="Copy Finding claim" onclick={() => void copyFindingClaim(finding)}>{findingCopyStatus[finding.id] === 'Finding claim copied.' ? 'Copied' : 'Copy claim'}</Button>
						</div>
						{#if findingCopyStatus[finding.id]}<span class="results-artifact-status" role="status">{findingCopyStatus[finding.id]}</span>{/if}
						<p class="results-finding-claim">{finding.claim}</p>
						{#if question}
							<div class="results-finding-question-row">
								<p class="results-finding-question">Answers: {question.text}</p>
								<Button type="button" variant="ghost" size="sm" onclick={() => onreviewquestion(question.id)}>Open Question</Button>
							</div>
						{/if}
						<details class="results-supporting">
							<summary>Review coverage, provenance &amp; checks</summary>
							{#if finding.caveats.length}<div><h4>Caveats</h4><ul>{#each finding.caveats as caveat (caveat)}<li>{caveat}</li>{/each}</ul></div>{/if}
							{#if finding.assumptions.length}<div><h4>Assumptions</h4><ul>{#each finding.assumptions as assumption (assumption)}<li>{assumption}</li>{/each}</ul></div>{/if}
							<section class="results-finding-context" aria-label="Finding coverage and run status">
								<div><span>Question coverage</span><strong>{question ? questionStatusLabel(question) : 'Question unavailable'}</strong></div>
								{#if question?.answer_status_reason}<p>{question.answer_status_reason}</p>{/if}
								{#if question && question.latest_analysis_turn_id !== finding.analysis_turn_id}
									<p>This Finding is linked to a run other than the Question’s latest answer. Review its run before treating it as current.</p>
								{/if}
								<div><span>Run outcome</span><strong>{verificationStatusLabel(run)}</strong></div>
								{#if !run && loadingRunIds.includes(finding.analysis_turn_id)}<p role="status">Loading the saved run and evidence…</p>
								{:else if !run && runErrors[finding.analysis_turn_id]}<p role="alert">Saved run unavailable: {runErrors[finding.analysis_turn_id]}</p>
									<Button type="button" variant="ghost" size="sm" onclick={() => retryRun(finding.analysis_turn_id)}>Retry loading run</Button>
								{/if}
							</section>
							<div class="results-finding-evidence">
								<h4>Supporting evidence</h4>
								{#if finding.evidence_ids.length}
									<ul>
									{#each finding.evidence_ids as evidenceId (`${finding.id}-${evidenceId}`)}
										{@const item = findingEvidenceItem(finding, evidenceId)}
										<li>
											{#if item}
												<Button type="button" variant="ghost" size="sm" onclick={() => onopenrun(finding.analysis_turn_id, evidenceId)} aria-label="Inspect evidence: {item.result_summary}">{item.result_summary || evidenceId}</Button>
												<small><code>{evidenceId}</code> · {evidenceSourceLabel(item)}</small>
											{:else}<code>{evidenceId}</code><small>{run ? 'This ID is not present in the saved run evidence.' : 'Evidence details unavailable until the run loads.'}</small>{/if}
										</li>
									{/each}
									</ul>
								{:else}<p>No supporting evidence IDs were recorded.</p>{/if}
							</div>
							<div class="results-finding-checks" aria-label="Verification checks">
								<h4>Checks · {verificationStatusLabel(run)}</h4>
								{#if checks.length}
									<ul>
									{#each checks as check, checkIndex (`${finding.id}-check-${checkIndex}`)}
										<li data-check-outcome={check.outcome ?? (check.ok ? 'passed' : 'flagged')}><strong>{verificationOutcome(check)} · {check.label}</strong>{#if check.detail}<span>{check.detail}</span>{/if}</li>
									{/each}
									</ul>
									<p>Each check speaks only to the condition it tested; passing checks do not establish overall business correctness.</p>
								{:else}<p>No verification checks were recorded for this run.</p>{/if}
							</div>
							<p>Analysis run <code>{finding.analysis_turn_id}</code> · <Button type="button" variant="ghost" size="sm" onclick={() => onopenrun(finding.analysis_turn_id)}>Open full analysis record</Button></p>
							{#if finding.revisions.length}
								<details class="results-finding-history">
									<summary>Revision history · {finding.revisions.length} prior version{finding.revisions.length === 1 ? '' : 's'}</summary>
									<ol>
										{#each finding.revisions as revision (`${finding.id}-history-${revision.revision}`)}
											<li>
												<strong>Revision {revision.revision}</strong>
												<p>{revision.claim}</p>
												<dl>
													<div><dt>Question</dt><dd>{revisionQuestionLabel(revision)}</dd></div>
													<div><dt>Run</dt><dd><Button type="button" variant="ghost" size="sm" onclick={() => onopenrun(revision.analysis_turn_id)}>Open saved run · {revision.analysis_turn_id}</Button></dd></div>
													<div><dt>Evidence</dt><dd>{#if revision.evidence_ids.length}{#each revision.evidence_ids as evidenceId (`${finding.id}-${revision.revision}-${evidenceId}`)}<Button type="button" variant="ghost" size="sm" onclick={() => onopenrun(revision.analysis_turn_id, evidenceId)} aria-label="Inspect prior evidence {evidenceId}">{evidenceId}</Button>{/each}{:else}None recorded{/if}</dd></div>
													{#if revision.caveats.length}<div><dt>Caveats</dt><dd>{revision.caveats.join('; ')}</dd></div>{/if}
													{#if revision.assumptions.length}<div><dt>Assumptions</dt><dd>{revision.assumptions.join('; ')}</dd></div>{/if}
													<div><dt>Why it changed</dt><dd>{revision.revision_note || 'No reason recorded'}</dd></div>
												</dl>
											</li>
										{/each}
									</ol>
									{#if finding.revision > 1}<p class="results-current-revision-note">Current revision note: {finding.revision_note || 'No reason recorded.'}</p>{/if}
								</details>
							{/if}
						</details>
					</article>
				{/each}
			</div>
		{:else}
			<p class="results-empty">No Findings have been saved yet. Capture a reviewed conclusion from an analysis run to feature it here.</p>
		{/if}
	</section>

	<section class="results-section" aria-labelledby="results-visualizations-title">
		<header class="results-section-heading"><div><h3 id="results-visualizations-title">Visualizations</h3><p>Charts from the latest saved Question runs and the runs supporting current Findings.</p></div><span>{chartCards.length}</span></header>
		{#if chartCards.length}
			<div class="results-charts">
				{#each chartCards as card (card.key)}
					{@const linkedFindings = findingsForChart(card)}
					<article class="results-chart" aria-label="Visualization artifact from {card.question?.text ?? 'saved analysis'}" data-artifact-kind="model-visualization">
						<Chart
							spec={card.evidence.chart!}
							source={sourceLabel(card.evidence)}
							sourceEvidence={card.sourceEvidence}
							sourceRunId={card.run.id}
							findingSnapshot={card.evidence.chart?.metadata?.finding_snapshot}
							originLabel="Model-produced visualization"
						/>
						<details class="results-artifact-lineage">
							<summary>Lineage &amp; source scope</summary>
							<dl>
								<div>
									<dt>Question</dt>
									<dd>
										{#if card.question}
											{card.question.text}
											<Button type="button" variant="ghost" size="sm" onclick={() => onreviewquestion(card.question!.id)}>Open Question</Button>
										{:else}Question is not in the current plan{/if}
									</dd>
								</div>
								<div>
									<dt>Sources used</dt>
									<dd>
										{#if card.run.source_scope?.used_sources?.length}
											{card.run.source_scope.used_sources.map((source) => source.table ? `${source.source} · ${source.table}` : source.source).join(', ')}
										{:else}Source scope not recorded{/if}
									</dd>
								</div>
								{#if card.run.source_scope?.skipped_sources?.length}
									<div><dt>Sources skipped</dt><dd>{card.run.source_scope.skipped_sources.join(', ')}</dd></div>
								{/if}
								<div>
									<dt>Visualization evidence</dt>
									<dd>
										{#if card.evidence.id}
											<code>{card.evidence.id}</code>
											<Button type="button" variant="ghost" size="sm" onclick={() => onopenrun(card.run.id, card.evidence.id)}>Open chart evidence</Button>
										{:else}Evidence ID not recorded{/if}
									</dd>
								</div>
								{#if card.sourceEvidence}
									<div>
										<dt>Data evidence</dt>
										<dd>
											<code>{card.sourceEvidence.id || card.evidence.chart?.metadata?.source_evidence_id}</code>
											{#if card.sourceEvidence.id}<Button type="button" variant="ghost" size="sm" onclick={() => onopenrun(card.run.id, card.sourceEvidence!.id)}>Open source evidence</Button>{/if}
										</dd>
									</div>
								{/if}
								<div><dt>Canonical run</dt><dd><code>{card.run.id}</code><Button type="button" variant="ghost" size="sm" onclick={() => onopenrun(card.run.id)}>Open analysis record</Button></dd></div>
								<div>
									<dt>Reviewed Finding</dt>
									<dd>
										{#if linkedFindings.length}
											{#each linkedFindings as finding (`${card.key}-${finding.id}`)}<a href="#finding-{finding.id}">Revision {finding.revision} · {finding.claim}</a>{/each}
										{:else}No current Finding explicitly cites this visualization or its source evidence{/if}
									</dd>
								</div>
							</dl>
						</details>
					</article>
				{/each}
			</div>
		{:else if loadingRunIds.length}
			<p class="results-empty" role="status">Loading saved run evidence…</p>
		{:else}
			<p class="results-empty">No visualizations are linked to the current Questions or Findings.</p>
		{/if}
	</section>

	{#if historicalRunIds.length}
		<section class="results-section results-historical" aria-labelledby="results-historical-title">
			<header class="results-section-heading">
				<div>
					<h3 id="results-historical-title">Historical and unassigned analysis</h3>
					<p>Earlier runs stay available for context; they do not change current Question coverage or the Findings count.</p>
				</div>
				<span>{historicalRunIds.length}</span>
			</header>
			<div class="results-historical-list">
				{#each historicalRunIds as turnId (turnId)}
					{@const run = runs[turnId]}
					{@const transcript = transcriptRunFor(turnId)}
					{@const evidence = run?.result.evidence.filter(isEvidenceItem) ?? []}
					{@const charts = run ? historicalChartEvidence(run) : []}
					<article class="results-historical-run" data-run-id={turnId}>
						<header class="results-historical-heading">
							<div>
								<Badge>{run ? 'Historical saved run' : transcript ? 'Transcript fallback' : 'Saved run'}</Badge>
								<h4>{historicalRunTitle(run, transcript, turnId)}</h4>
							</div>
							{#if run}<span class="results-historical-outcome">{historicalRunOutcome(run)}</span>{/if}
						</header>
						{#if run}
							<p class="results-historical-answer-preview">{run.result.text}</p>
							<details class="results-supporting">
								<summary>Review saved answer, evidence &amp; provenance</summary>
								<div class="results-historical-detail">
									<h5>Recorded answer</h5>
									<p class="results-historical-answer">{run.result.text || 'No answer text was recorded.'}</p>
									<h5>Run provenance</h5>
									<dl class="results-historical-provenance">
										<div><dt>Workspace</dt><dd>{run.workspace || investigation.workspace_id}</dd></div>
										{#if run.workspace_revision}<div><dt>Data revision</dt><dd><code>{run.workspace_revision}</code></dd></div>{/if}
										{#if run.investigation_context?.question_text}<div><dt>Question context at the time</dt><dd>{run.investigation_context.question_text}</dd></div>{/if}
										{#if run.source_scope?.used_sources?.length}<div><dt>Sources used</dt><dd>{run.source_scope.used_sources.map((source) => source.table ? `${source.source} · ${source.table}` : source.source).join(', ')}</dd></div>{/if}
										{#if run.source_scope?.skipped_sources?.length}<div><dt>Skipped sources</dt><dd>{run.source_scope.skipped_sources.join(', ')}</dd></div>{/if}
									</dl>
									<div class="results-historical-evidence">
										<h5>Evidence · {evidence.length}</h5>
										{#if evidence.length}
											<ul>
												{#each evidence as item, index (`${turnId}-evidence-${item.id ?? index}`)}
													<li>
														{#if item.id}<Button type="button" variant="ghost" size="sm" onclick={() => onopenrun(turnId, item.id)}>{item.result_summary || item.id}</Button>
														{:else}<span>{item.result_summary || item.tool}</span>{/if}
														<small>{evidenceSourceLabel(item)}</small>
													</li>
												{/each}
											</ul>
										{:else}<p>No evidence was recorded for this run.</p>{/if}
									</div>
									{#if charts.length}
										<div class="results-historical-charts" aria-label="Historical visualizations">
											<h5>Historical visualizations · {charts.length}</h5>
											{#each charts as chartEvidence, index (`${turnId}-chart-${chartEvidence.id ?? index}`)}
												<article class="results-chart" aria-label="Historical visualization from {run.question}">
													<p class="results-chart-question">Historical visualization · {run.question}</p>
													<Chart
														spec={chartEvidence.chart!}
														source={sourceLabel(chartEvidence)}
														sourceEvidence={chartSourceEvidence(run, chartEvidence)}
														sourceRunId={run.id}
														findingSnapshot={chartEvidence.chart?.metadata?.finding_snapshot}
													/>
													<Button type="button" variant="ghost" size="sm" onclick={() => onopenrun(run.id, chartEvidence.id)}>Open source run</Button>
												</article>
											{/each}
										</div>
									{/if}
									<p><code>{turnId}</code> · <Button type="button" variant="ghost" size="sm" onclick={() => onopenrun(turnId)}>Open saved analysis run</Button></p>
								</div>
							</details>
						{:else if transcript}
							{#if loadingRunIds.includes(turnId)}
								<p class="results-historical-note" role="status">Checking for a saved run. This transcript copy is not canonical evidence.</p>
							{:else if runErrors[turnId]}
								<p class="results-historical-note" role="status">Saved run unavailable; showing the original transcript only. It is not canonical evidence.</p>
								<Button type="button" variant="ghost" size="sm" onclick={() => retryRun(turnId)}>Retry saved run</Button>
							{/if}
							<p class="results-historical-note">Not linked to a current Question · not counted toward coverage or Findings.</p>
							<details class="results-supporting">
								<summary>Read transcript answer</summary>
								<div class="results-historical-detail">
									<h5>Original request</h5><p>{transcript.prompt || 'Request text is not available in the loaded transcript.'}</p>
									<h5>Transcript answer</h5><p class="results-historical-answer">{transcript.answer || 'Answer text is not available in the loaded transcript.'}</p>
								</div>
							</details>
							<Button type="button" variant="ghost" size="sm" onclick={() => onopenlegacy(turnId)}>Open transcript answer</Button>
						{:else if loadingRunIds.includes(turnId)}
							<p class="results-historical-note" role="status">Loading historical run {turnId}…</p>
						{:else if runErrors[turnId]}
							<p class="results-historical-note" role="alert">Historical run unavailable: {runErrors[turnId]}</p>
							<Button type="button" variant="ghost" size="sm" onclick={() => retryRun(turnId)}>Retry loading run</Button>
						{:else}
							<p class="results-historical-note" role="status">Loading saved run {turnId}…</p>
						{/if}
					</article>
				{/each}
			</div>
		</section>
	{/if}

	<section class="results-section" aria-labelledby="results-questions-title">
		<header class="results-section-heading"><div><h3 id="results-questions-title">Questions to resolve</h3><p>Open Questions and answers that need review remain visible, even when the Investigation is closed.</p></div><span>{unresolvedQuestions.length}</span></header>
		{#if unresolvedQuestions.length}
			<ul class="results-questions">
				{#each unresolvedQuestions as question (question.id)}
					<li class="results-question" data-question-status={question.answer_status}>
						<div class="results-question-heading"><Badge variant={question.answer_status === 'blocked' ? 'destructive' : 'warning'}>{questionStatusLabel(question)}</Badge><strong>{question.text}</strong></div>
						<p>{questionReason(question)}</p>
						<div class="results-question-actions">
							{#if question.latest_analysis_turn_id && loadingRunIds.includes(question.latest_analysis_turn_id)}
								<span role="status">Loading saved answer…</span>
							{:else if question.latest_analysis_turn_id && runErrors[question.latest_analysis_turn_id]}
								{#if transcriptRunFor(question.latest_analysis_turn_id)}
									{@const transcript = transcriptRunFor(question.latest_analysis_turn_id)!}
									<span role="status">Canonical run unavailable. Transcript copy only; evidence and verification are unavailable.</span>
									<details class="results-answer-detail"><summary>Read transcript copy (not canonical)</summary><div>{transcript.answer}</div><Button type="button" variant="ghost" size="sm" onclick={() => onopenlegacy(question.latest_analysis_turn_id)}>Open transcript answer</Button></details>
								{:else}<span role="alert">Saved answer unavailable: {runErrors[question.latest_analysis_turn_id]}</span>{/if}
								<Button type="button" variant="ghost" size="sm" onclick={() => retryRun(question.latest_analysis_turn_id!)}>Retry</Button>
							{:else if question.latest_analysis_turn_id && runs[question.latest_analysis_turn_id]}
								<details class="results-answer-detail">
									<summary>Open latest saved answer</summary>
									<div>{runs[question.latest_analysis_turn_id].result.text}</div>
									<Button type="button" variant="ghost" size="sm" onclick={() => onopenrun(question.latest_analysis_turn_id!)}>Open analysis record</Button>
								</details>
							{/if}
							<Button type="button" variant="outline" size="sm" onclick={() => onreviewquestion(question.id)}>Review Question</Button>
						</div>
					</li>
				{/each}
			</ul>
		{:else if activeQuestions.length}
			<p class="results-complete" role="status"><Icon name="check" size={16} /> All active Questions have complete, current answers.</p>
		{:else}
			<p class="results-empty">No active Questions are recorded for this Investigation.</p>
		{/if}
	</section>

	<section class="results-section results-answer-index" aria-labelledby="results-answer-index-title">
		<details class="results-supporting">
			<summary id="results-answer-index-title">Current saved answers ({activeQuestions.filter((question) => question.latest_analysis_turn_id).length})</summary>
			<ul>
				{#each activeQuestions.filter((question) => question.latest_analysis_turn_id) as question (question.id)}
					<li>
						<strong>{question.text}</strong>
						<span>{questionStatusLabel(question)}</span>
						{#if loadingRunIds.includes(question.latest_analysis_turn_id!)}<p role="status">Loading saved answer…</p>
						{:else if runErrors[question.latest_analysis_turn_id!] && transcriptRunFor(question.latest_analysis_turn_id!)}{@const transcript = transcriptRunFor(question.latest_analysis_turn_id!)!}<p role="status">Canonical run unavailable; transcript copy only.</p><details><summary>Read transcript copy</summary><div>{transcript.answer}</div><Button type="button" variant="ghost" size="sm" onclick={() => onopenlegacy(question.latest_analysis_turn_id)}>Open transcript answer</Button></details><Button type="button" variant="ghost" size="sm" onclick={() => retryRun(question.latest_analysis_turn_id!)}>Retry saved run</Button>
						{:else if runErrors[question.latest_analysis_turn_id!]}<p role="alert">Saved answer unavailable: {runErrors[question.latest_analysis_turn_id!]}</p><Button type="button" variant="ghost" size="sm" onclick={() => retryRun(question.latest_analysis_turn_id!)}>Retry</Button>
						{:else if runs[question.latest_analysis_turn_id!]}<details><summary>Read answer</summary><div>{runs[question.latest_analysis_turn_id!].result.text}</div><Button type="button" variant="ghost" size="sm" onclick={() => onopenrun(question.latest_analysis_turn_id!)}>Open analysis record</Button></details>{/if}
					</li>
				{/each}
			</ul>
		</details>
	</section>
</section>

<style>
	.results-view { display: grid; min-width: 0; gap: var(--space-5); padding: var(--space-4) 0; }
	.results-warning { display: flex; align-items: flex-start; gap: var(--space-2); margin: 0; padding: var(--space-2) var(--space-3); border: 1px solid var(--border); border-radius: var(--radius-sm); color: var(--text-dim); font-size: var(--fs-xs); }
	.results-warning :global(svg) { flex: none; }
	.results-run-errors { display: flex; flex-wrap: wrap; align-items: center; gap: var(--space-2); padding: var(--space-2) var(--space-3); border: 1px solid var(--danger, #b42318); border-radius: var(--radius-sm); color: var(--danger, #b42318); font-size: var(--fs-xs); }
	.results-run-errors p { flex: 1 1 100%; margin: 0; }
	.results-overview { display: grid; gap: var(--space-3); padding: var(--space-4); border: 1px solid var(--border); border-radius: var(--radius-md); background: var(--bg-inset); }
	.results-overview-heading, .results-section-heading, .results-finding-heading, .results-question-heading { display: flex; align-items: flex-start; justify-content: space-between; gap: var(--space-3); }
	.results-overview-heading h3 { max-width: 850px; margin: 2px 0 0; font-size: clamp(var(--fs-md), 2vw, var(--fs-xl)); font-weight: 650; line-height: 1.3; overflow-wrap: anywhere; }
	.results-eyebrow { margin: 0; color: var(--text-faint); font-size: var(--fs-xs); font-weight: 650; text-transform: uppercase; letter-spacing: .06em; }
	.results-brief-facts { display: flex; flex-wrap: wrap; gap: var(--space-4); margin: 0; }
	.results-brief-facts > div { display: grid; min-width: min(280px, 100%); gap: 3px; }
	.results-brief-facts dt, .results-supporting h4 { color: var(--text-faint); font-size: var(--fs-xs); font-weight: 650; }
	.results-brief-facts dd { margin: 0; font-size: var(--fs-sm); overflow-wrap: anywhere; }
	.results-section { display: grid; min-width: 0; gap: var(--space-3); padding-top: var(--space-4); border-top: 1px solid var(--border); }
	.results-section-heading { align-items: center; }
	.results-section-heading h3 { margin: 0; font-size: var(--fs-md); font-weight: 650; }
	.results-section-heading p { margin: 3px 0 0; color: var(--text-dim); font-size: var(--fs-xs); }
	.results-section-heading > span { color: var(--text-faint); font-size: var(--fs-xs); font-variant-numeric: tabular-nums; }
	.results-findings, .results-questions { display: grid; gap: var(--space-2); margin: 0; padding: 0; list-style: none; }
	.results-finding, .results-question { display: grid; min-width: 0; gap: var(--space-2); padding: var(--space-3); border: 1px solid var(--border); border-radius: var(--radius-md); background: var(--bg); }
	.results-finding-heading { align-items: center; color: var(--text-faint); font-size: var(--fs-xs); font-weight: 650; }
	.results-finding-heading :global(button) { flex: none; }
	.results-artifact-status { color: var(--text-faint); font-size: var(--fs-xs); }
	.results-finding-claim { margin: 0; font-size: var(--fs-md); font-weight: 550; line-height: 1.5; overflow-wrap: anywhere; }
	.results-finding-question-row { display: flex; flex-wrap: wrap; align-items: center; gap: var(--space-1) var(--space-2); }
	.results-finding-question, .results-empty { margin: 0; color: var(--text-dim); font-size: var(--fs-xs); overflow-wrap: anywhere; }
	.results-supporting { min-width: 0; color: var(--text-dim); font-size: var(--fs-xs); }
	.results-supporting > summary, .results-answer-detail > summary { width: fit-content; color: var(--brand); cursor: pointer; }
	.results-supporting > div { margin-top: var(--space-2); }
	.results-supporting h4 { margin: 0 0 3px; }
	.results-supporting ul { margin: 0; padding-left: 1.2rem; }
	.results-supporting > p { margin: var(--space-2) 0; overflow-wrap: anywhere; }
	.results-supporting code { overflow-wrap: anywhere; }
	.results-finding-context { display: grid; gap: var(--space-1); margin-top: var(--space-3); padding: var(--space-2) var(--space-3); border-left: 2px solid var(--border-strong); background: var(--bg-inset); }
	.results-finding-context > div { display: flex; flex-wrap: wrap; gap: 0 var(--space-2); }
	.results-finding-context > div span, .results-finding-context > p { color: var(--text-faint); }
	.results-finding-context > div strong { color: var(--text-dim); font-weight: 600; }
	.results-finding-context p { margin: 0; }
	.results-finding-evidence, .results-finding-checks { margin-top: var(--space-3); }
	.results-finding-evidence ul, .results-finding-checks ul { display: grid; gap: var(--space-1); padding: 0; list-style: none; }
	.results-finding-evidence li { display: grid; justify-items: start; gap: 0 var(--space-2); min-width: 0; }
	.results-finding-evidence li :global(button) { max-width: 100%; justify-content: flex-start; text-align: left; }
	.results-finding-evidence li small { padding-left: var(--space-2); color: var(--text-faint); overflow-wrap: anywhere; }
	.results-finding-checks li { display: grid; gap: 2px; padding-left: var(--space-2); border-left: 2px solid var(--border-strong); }
	.results-finding-checks li[data-check-outcome='passed'] { border-color: var(--success, #15803d); }
	.results-finding-checks li[data-check-outcome='flagged'] { border-color: var(--danger, #b42318); }
	.results-finding-checks li strong { color: var(--text-dim); font-weight: 550; }
	.results-finding-checks li span, .results-finding-checks > p { color: var(--text-faint); }
	.results-finding-checks > p { margin: var(--space-2) 0 0; }
	.results-finding-history { margin-top: var(--space-3); padding-top: var(--space-2); border-top: 1px solid var(--border); }
	.results-finding-history > summary { width: fit-content; color: var(--brand); cursor: pointer; }
	.results-finding-history > ol { display: grid; gap: var(--space-2); margin: var(--space-2) 0 0; padding-left: var(--space-5); }
	.results-finding-history > ol > li { padding: var(--space-2) 0; }
	.results-finding-history > ol > li > p { margin: var(--space-1) 0; color: var(--text); white-space: pre-wrap; overflow-wrap: anywhere; }
	.results-current-revision-note { margin: var(--space-2) 0 0; padding-top: var(--space-2); border-top: 1px solid var(--border); color: var(--text-faint); }
	.results-finding-history dl { display: grid; gap: var(--space-1); margin: 0; }
	.results-finding-history dl > div { display: grid; grid-template-columns: minmax(72px, auto) minmax(0, 1fr); gap: var(--space-2); }
	.results-finding-history dt { color: var(--text-faint); }
	.results-finding-history dd { min-width: 0; margin: 0; overflow-wrap: anywhere; }
	.results-charts { display: grid; grid-template-columns: repeat(auto-fit, minmax(min(100%, 420px), 1fr)); gap: var(--space-3); }
	.results-chart { display: grid; min-width: 0; gap: var(--space-2); align-content: start; }
	.results-chart-question { margin: 0; color: var(--text-dim); font-size: var(--fs-xs); }
	.results-chart :global(.chart-card) { width: 100%; min-width: 0; }
	.results-artifact-lineage { font-size: var(--fs-xs); }
	.results-artifact-lineage > summary { width: fit-content; color: var(--text-faint); cursor: pointer; }
	.results-artifact-lineage > summary:hover { color: var(--text-dim); }
	.results-artifact-lineage dl { display: grid; gap: var(--space-2); margin: var(--space-2) 0 0; }
	.results-artifact-lineage dl > div { display: grid; grid-template-columns: minmax(120px, auto) minmax(0, 1fr); align-items: start; gap: var(--space-2); }
	.results-artifact-lineage dt { color: var(--text-faint); }
	.results-artifact-lineage dd { display: flex; min-width: 0; flex-wrap: wrap; align-items: center; gap: var(--space-1) var(--space-2); margin: 0; color: var(--text-dim); overflow-wrap: anywhere; }
	.results-artifact-lineage dd a { color: var(--link); overflow-wrap: anywhere; }
	.results-historical { padding: var(--space-3); border: 1px solid var(--border); border-radius: var(--radius-md); background: var(--bg-inset); }
	.results-historical-list { display: grid; gap: var(--space-2); }
	.results-historical-run { display: grid; min-width: 0; gap: var(--space-2); padding: var(--space-3); border: 1px solid var(--border); border-radius: var(--radius-sm); background: var(--bg); }
	.results-historical-heading { display: flex; align-items: flex-start; justify-content: space-between; gap: var(--space-3); }
	.results-historical-heading > div { display: grid; min-width: 0; justify-items: start; gap: var(--space-1); }
	.results-historical-heading h4 { margin: 0; font-size: var(--fs-sm); font-weight: 600; overflow-wrap: anywhere; }
	.results-historical-outcome, .results-historical-note { color: var(--text-faint); font-size: var(--fs-xs); }
	.results-historical-answer-preview { display: -webkit-box; overflow: hidden; margin: 0; color: var(--text-dim); font-size: var(--fs-xs); line-height: 1.5; overflow-wrap: anywhere; -webkit-box-orient: vertical; line-clamp: 3; -webkit-line-clamp: 3; }
	.results-historical-note { margin: 0; }
	.results-historical-detail { display: grid; gap: var(--space-2); margin-top: var(--space-2); }
	.results-historical-detail h5 { margin: var(--space-2) 0 0; color: var(--text-faint); font-size: var(--fs-xs); font-weight: 650; }
	.results-historical-detail > p { margin: 0; overflow-wrap: anywhere; }
	.results-historical-answer { max-height: 280px; overflow: auto; padding: var(--space-3); border: 1px solid var(--border); border-radius: var(--radius-sm); background: var(--bg-inset); color: var(--text); white-space: pre-wrap; line-height: 1.5; }
	.results-historical-provenance { display: grid; gap: var(--space-1); margin: 0; }
	.results-historical-provenance > div { display: grid; grid-template-columns: minmax(110px, auto) minmax(0, 1fr); gap: var(--space-2); }
	.results-historical-provenance dt { color: var(--text-faint); }
	.results-historical-provenance dd { min-width: 0; margin: 0; color: var(--text-dim); overflow-wrap: anywhere; }
	.results-historical-evidence { display: grid; gap: var(--space-1); }
	.results-historical-evidence ul { display: grid; gap: var(--space-1); margin: 0; padding: 0; list-style: none; }
	.results-historical-evidence li { display: grid; justify-items: start; min-width: 0; }
	.results-historical-evidence li :global(button) { max-width: 100%; justify-content: flex-start; text-align: left; }
	.results-historical-evidence small { padding-left: var(--space-2); color: var(--text-faint); overflow-wrap: anywhere; }
	.results-historical-charts { display: grid; gap: var(--space-2); }
	.results-question[data-question-status='blocked'] { border-color: var(--danger, #b42318); }
	.results-question-heading { align-items: center; justify-content: flex-start; }
	.results-question-heading strong { min-width: 0; font-size: var(--fs-sm); overflow-wrap: anywhere; }
	.results-question > p { margin: 0; color: var(--text-dim); font-size: var(--fs-xs); overflow-wrap: anywhere; }
	.results-question-actions { display: flex; flex-wrap: wrap; align-items: center; gap: var(--space-2); }
	.results-question-actions > span { color: var(--text-dim); font-size: var(--fs-xs); overflow-wrap: anywhere; }
	.results-answer-detail { flex-basis: 100%; font-size: var(--fs-xs); }
	.results-answer-detail > div, .results-answer-index details details > div { max-height: 280px; overflow: auto; margin: var(--space-2) 0; padding: var(--space-3); border: 1px solid var(--border); border-radius: var(--radius-sm); background: var(--bg-inset); color: var(--text); white-space: pre-wrap; overflow-wrap: anywhere; line-height: 1.5; }
	.results-complete { display: flex; align-items: center; gap: var(--space-2); margin: 0; color: var(--text-dim); font-size: var(--fs-sm); }
	.results-complete :global(svg) { color: var(--success, #15803d); }
	.results-answer-index { padding-top: 0; border-top: 0; }
	.results-answer-index > details > ul { display: grid; gap: var(--space-2); margin: var(--space-3) 0 0; padding: 0; list-style: none; }
	.results-answer-index > details > ul > li { display: grid; justify-items: start; gap: var(--space-2); padding: var(--space-3); border: 1px solid var(--border); border-radius: var(--radius-sm); }
	.results-answer-index > details > ul > li > strong { font-size: var(--fs-sm); }
	.results-answer-index > details > ul > li > span, .results-answer-index p { margin: 0; color: var(--text-faint); font-size: var(--fs-xs); }
</style>

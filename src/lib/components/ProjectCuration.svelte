<script lang="ts">
	import { Dialog } from '$lib/components/ui/dialog';
	import { Button } from '$lib/components/ui/button';
	import { Badge } from '$lib/components/ui/badge';
	import { ipc, isDesktop } from '$lib/ipc';
	import { session } from '$lib/session.svelte';
	import { isRenderableChartEvidence } from '$lib/workbench';
	import {
		investigationQuestionStatusDetail,
		investigationQuestionStatusLabel
	} from '$lib/investigation-question-status';
	import type {
		AnalysisTurn,
		EvidenceItem,
		Investigation,
		InvestigationDefinition,
		InvestigationFinding,
		InvestigationQuestion,
		Project,
		ProjectReference
	} from '$lib/types';
	import Icon from './Icon.svelte';

	type CurationOption = {
		key: string;
		reference: ProjectReference;
		kind: string;
		title: string;
		provenance: string;
		detail?: string;
	};
	type CurationGroup = { investigation: Investigation; options: CurationOption[] };
	type SavedReferenceView = {
		key: string;
		reference: ProjectReference;
		kind: string;
		title: string;
		provenance: string;
		detail?: string;
		available: boolean;
	};

	let { project }: { project: Project } = $props();
	let dialogOpen = $state(false);
	let investigations = $state<Investigation[]>([]);
	let runs = $state<Record<string, AnalysisTurn | null>>({});
	let loading = $state(false);
	let loadError = $state('');
	let loadedWorkspace = '';
	let loadGeneration = 0;

	$effect(() => {
		const workspace = project.workspace;
		if (workspace && workspace !== loadedWorkspace) void loadProjectRecords(workspace);
	});

	async function loadProjectRecords(workspace: string): Promise<void> {
		const generation = ++loadGeneration;
		loadedWorkspace = workspace;
		loading = true;
		loadError = '';
		try {
			const all = await ipc.investigationsList(session.conversations.map((conversation) => conversation.id));
			const matching = all.filter((item) => item.workspace_id === workspace);
			const runIds = new Set<string>();
			for (const investigation of matching) {
				for (const question of investigation.questions ?? []) {
					if (question.status === 'active' && question.latest_analysis_turn_id) runIds.add(question.latest_analysis_turn_id);
				}
				for (const finding of investigation.findings ?? []) runIds.add(finding.analysis_turn_id);
			}
			for (const reference of project.references ?? []) {
				if (reference.kind === 'analysis_artifact') runIds.add(reference.analysis_turn_id);
			}
			const loaded = await Promise.all([...runIds].map(async (turnId) => {
				try {
					return [turnId, await ipc.analysisTurnLoad(turnId)] as const;
				} catch {
					return [turnId, null] as const;
				}
			}));
			if (generation !== loadGeneration || project.workspace !== workspace) return;
			investigations = matching;
			runs = Object.fromEntries(loaded);
		} catch (cause) {
			if (generation !== loadGeneration) return;
			investigations = [];
			runs = {};
			loadError = cause instanceof Error ? cause.message : String(cause);
		} finally {
			if (generation === loadGeneration) loading = false;
		}
	}

	function isEvidenceItem(value: unknown): value is EvidenceItem {
		if (!value || typeof value !== 'object' || Array.isArray(value)) return false;
		const item = value as Record<string, unknown>;
		return typeof item.tool === 'string'
			&& Boolean(item.args && typeof item.args === 'object' && !Array.isArray(item.args))
			&& typeof item.result_summary === 'string';
	}

	function referenceKey(reference: ProjectReference): string {
		switch (reference.kind) {
			case 'finding': return `finding:${reference.investigation_id}:${reference.finding_id}:${reference.finding_revision}`;
			case 'analysis_artifact': return `analysis_artifact:${reference.analysis_turn_id}:${reference.evidence_id}`;
			case 'definition': return `definition:${reference.investigation_id}:${reference.definition_id}:${reference.definition_revision}`;
			case 'question': return `question:${reference.investigation_id}:${reference.question_id}`;
		}
	}

	function findingOption(investigation: Investigation, finding: InvestigationFinding): CurationOption {
		const reference: ProjectReference = {
			kind: 'finding',
			investigation_id: investigation.id,
			finding_id: finding.id,
			finding_revision: finding.revision ?? 1
		};
		const question = investigation.questions?.find((item) => item.id === finding.question_id);
		return {
			key: referenceKey(reference),
			reference,
			kind: 'Finding',
			title: finding.claim,
			provenance: `${investigation.title} · revision ${finding.revision ?? 1} · run ${finding.analysis_turn_id}`,
			detail: question?.text
		};
	}

	function definitionOption(investigation: Investigation, definition: InvestigationDefinition): CurationOption {
		const reference: ProjectReference = {
			kind: 'definition',
			investigation_id: investigation.id,
			definition_id: definition.id,
			definition_revision: definition.revision
		};
		return {
			key: referenceKey(reference),
			reference,
			kind: definition.kind === 'definition' ? 'Definition' : 'Assumption',
			title: definition.text,
			provenance: `${investigation.title} · revision ${definition.revision}`
		};
	}

	function questionOption(investigation: Investigation, question: InvestigationQuestion): CurationOption {
		const reference: ProjectReference = { kind: 'question', investigation_id: investigation.id, question_id: question.id };
		return {
			key: referenceKey(reference),
			reference,
			kind: 'Open Question',
			title: question.text,
			provenance: `${investigation.title} · ${investigationQuestionStatusLabel(question.answer_status)}`,
			detail: investigationQuestionStatusDetail(question)
		};
	}

	let groups = $derived.by(() => {
		return investigations.map((investigation) => {
			const options: CurationOption[] = [];
			for (const definition of investigation.definitions ?? []) {
				if (definition.status === 'active') options.push(definitionOption(investigation, definition));
			}
			for (const question of investigation.questions ?? []) {
				if (question.status === 'active' && question.answer_status !== 'answered') options.push(questionOption(investigation, question));
			}
			for (const finding of investigation.findings ?? []) options.push(findingOption(investigation, finding));

			const runIds = new Set<string>();
			for (const question of investigation.questions ?? []) {
				if (question.status === 'active' && question.latest_analysis_turn_id) runIds.add(question.latest_analysis_turn_id);
			}
			for (const finding of investigation.findings ?? []) runIds.add(finding.analysis_turn_id);
			for (const turnId of runIds) {
				const run = runs[turnId];
				if (!run) continue;
				for (const [index, item] of run.result.evidence.filter(isEvidenceItem).entries()) {
					if (!item.id || !isRenderableChartEvidence(item, run.verification?.checks ?? run.result.verification)) continue;
					const reference: ProjectReference = { kind: 'analysis_artifact', analysis_turn_id: turnId, evidence_id: item.id };
					options.push({
						key: referenceKey(reference),
						reference,
						kind: 'Visualization',
						title: item.chart?.title?.trim() || `Visualization ${index + 1}`,
						provenance: `${investigation.title} · run ${turnId} · evidence ${item.id}`,
						detail: item.chart?.metadata?.finding_snapshot || item.result_summary
					});
				}
			}
			return { investigation, options };
		}).filter((group) => group.options.length > 0);
	});

	function revisionText<Current extends { revision: number; text: string }, Revision extends { revision: number; text: string }>(
		current: Current,
		history: Revision[],
		revision: number
	): string | null {
		if (current.revision === revision) return current.text;
		return history.find((item) => item.revision === revision)?.text ?? null;
	}

	function resolveReference(reference: ProjectReference): SavedReferenceView {
		const key = referenceKey(reference);
		switch (reference.kind) {
			case 'finding': {
				const investigation = investigations.find((item) => item.id === reference.investigation_id);
				const finding = investigation?.findings?.find((item) => item.id === reference.finding_id);
				const snapshot = finding && (finding.revision === reference.finding_revision
					? finding
					: finding.revisions.find((item) => item.revision === reference.finding_revision));
				return {
					key, reference, kind: 'Finding',
					title: snapshot?.claim ?? 'Finding unavailable',
					provenance: `${investigation?.title ?? reference.investigation_id} · revision ${reference.finding_revision} · run ${snapshot?.analysis_turn_id ?? finding?.analysis_turn_id ?? 'not recorded'}`,
					detail: snapshot?.caveats?.join(' · '),
					available: Boolean(snapshot)
				};
			}
			case 'analysis_artifact': {
				const run = runs[reference.analysis_turn_id];
				const evidence = run?.result.evidence.filter(isEvidenceItem).find((item) => item.id === reference.evidence_id);
				const sourceInvestigation = investigations.find((item) =>
					(item.analysis_turn_ids ?? []).includes(reference.analysis_turn_id)
					|| (item.questions ?? []).some((question) => question.latest_analysis_turn_id === reference.analysis_turn_id)
					|| (item.findings ?? []).some((finding) => finding.analysis_turn_id === reference.analysis_turn_id)
				);
				return {
					key, reference, kind: 'Visualization',
					title: evidence?.chart?.title?.trim() || (evidence ? 'Saved visualization' : 'Visualization unavailable'),
					provenance: `${sourceInvestigation?.title ?? run?.investigation_context?.investigation_id ?? 'Investigation'} · run ${reference.analysis_turn_id} · evidence ${reference.evidence_id}`,
					detail: evidence?.chart?.metadata?.finding_snapshot || evidence?.result_summary,
					available: Boolean(evidence?.chart)
				};
			}
			case 'definition': {
				const investigation = investigations.find((item) => item.id === reference.investigation_id);
				const definition = investigation?.definitions?.find((item) => item.id === reference.definition_id);
				const text = definition && revisionText(definition, definition.history, reference.definition_revision);
				return {
					key, reference, kind: definition?.kind === 'assumption' ? 'Assumption' : 'Definition',
					title: text ?? 'Definition unavailable',
					provenance: `${investigation?.title ?? reference.investigation_id} · revision ${reference.definition_revision}`,
					available: Boolean(text)
				};
			}
			case 'question': {
				const investigation = investigations.find((item) => item.id === reference.investigation_id);
				const question = investigation?.questions?.find((item) => item.id === reference.question_id);
				return {
					key, reference, kind: 'Question',
					title: question?.text ?? 'Question unavailable',
					provenance: `${investigation?.title ?? reference.investigation_id} · ${question ? investigationQuestionStatusLabel(question.answer_status) : 'state unavailable'}`,
					detail: question ? investigationQuestionStatusDetail(question) : undefined,
					available: Boolean(question)
				};
			}
		}
	}

	let savedViews = $derived((project.references ?? []).map(resolveReference));
	let selectedKeys = $derived(new Set((project.references ?? []).map(referenceKey)));

	function toggleReference(reference: ProjectReference): void {
		const key = referenceKey(reference);
		const current = project.references ?? [];
		const exists = current.some((item) => referenceKey(item) === key);
		const next = current.filter((item) => referenceKey(item) !== key).map((item) => ({ ...item }));
		if (!exists) next.push({ ...reference });
		session.updateProject(project.id, { references: next });
	}

	function refresh(): void {
		if (isDesktop()) void loadProjectRecords(project.workspace);
	}
</script>

<section class="project-curation" aria-labelledby="project-curation-title">
	<header class="curation-heading">
		<div>
			<h2 id="project-curation-title">Curated from Investigations</h2>
			<p>Pin useful conclusions and context by reference. Project curation never copies or edits the source records.</p>
		</div>
		<Button variant="outline" onclick={() => { dialogOpen = true; refresh(); }}>
			<Icon name="plus" size={16} /> Curate outputs
		</Button>
	</header>

	{#if savedViews.length}
		<ul class="curated-list" aria-label="Project-curated Investigation references">
			{#each savedViews as saved (saved.key)}
				<li>
					<div class="saved-copy">
						<div class="saved-title-row"><Badge variant={saved.available ? 'default' : 'warning'}>{saved.kind}</Badge><strong>{saved.title}</strong></div>
						<small>{saved.provenance}</small>
						{#if saved.detail}<p>{saved.detail}</p>{/if}
						{#if !saved.available}<small class="unavailable-note">The source record is unavailable; this reference is retained until you remove it.</small>{/if}
					</div>
					<Button variant="ghost" size="sm" aria-label={`Remove ${saved.kind} from Project: ${saved.title}`} onclick={() => toggleReference(saved.reference)}>Remove</Button>
				</li>
			{/each}
		</ul>
	{:else}
		<p class="curation-empty">No outputs have been curated yet. Findings, visualizations, active definitions, and open Questions from this workspace can be added when useful.</p>
	{/if}

	<Dialog.Root bind:open={dialogOpen}>
		<Dialog.Portal>
			<Dialog.Overlay class="fella-ui-dialog-overlay" />
			<Dialog.Content class="fella-ui-dialog-content project-curation-dialog" aria-label="Curate Project outputs">
				<header class="curation-dialog-header">
					<div>
						<p class="curation-eyebrow">{project.name}</p>
						<Dialog.Title class="curation-dialog-title" level={2}>Curate from Investigations</Dialog.Title>
						<Dialog.Description class="curation-dialog-description">Choose saved records to reference in this Project. The source Investigation and pinned revision or run/evidence remain canonical.</Dialog.Description>
					</div>
					<Dialog.Close class="curation-close" type="button" aria-label="Close curation picker" title="Close"><Icon name="x" size={16} /></Dialog.Close>
				</header>
				<div class="curation-dialog-body">
					{#if loading}
						<p class="curation-empty" role="status">Loading saved Investigation outputs…</p>
					{:else if loadError}
						<p class="curation-error" role="alert">Could not load Investigation records: {loadError}</p>
					{:else if groups.length}
						{#each groups as group (group.investigation.id)}
							<section class="investigation-group" aria-label={`Outputs from ${group.investigation.title}`}>
								<header><h3>{group.investigation.title}</h3><small>{group.options.length} eligible output{group.options.length === 1 ? '' : 's'}</small></header>
								<ul>
									{#each group.options as option (option.key)}
										<li>
											<div class="option-copy">
												<div class="saved-title-row"><Badge>{option.kind}</Badge><strong>{option.title}</strong></div>
												<small>{option.provenance}</small>
												{#if option.detail}<p>{option.detail}</p>{/if}
											</div>
											<Button
												variant={selectedKeys.has(option.key) ? 'ghost' : 'outline'}
												size="sm"
												aria-pressed={selectedKeys.has(option.key)}
												aria-label={`${selectedKeys.has(option.key) ? 'Remove' : 'Add'} ${option.kind} ${option.title} ${selectedKeys.has(option.key) ? 'from' : 'to'} Project`}
												onclick={() => toggleReference(option.reference)}
											>
												{selectedKeys.has(option.key) ? 'Added' : 'Add to Project'}
											</Button>
										</li>
									{/each}
								</ul>
							</section>
						{/each}
					{:else}
						<p class="curation-empty">No eligible saved outputs are available for this workspace yet. Save a Finding or Question result in Investigation to curate it here.</p>
					{/if}
				</div>
				<footer class="curation-dialog-footer">
					{#if loadError}<Button variant="outline" onclick={refresh}>Retry</Button>{/if}
					<Dialog.Close class="curation-done" type="button">Done</Dialog.Close>
				</footer>
			</Dialog.Content>
		</Dialog.Portal>
	</Dialog.Root>
</section>

<style>
	.project-curation { display: grid; gap: var(--space-3); margin-top: var(--space-5); padding-top: var(--space-4); border-top: 1px solid var(--border); }
	.curation-heading { display: flex; align-items: flex-start; justify-content: space-between; gap: var(--space-4); }
	.curation-heading h2 { margin: 0; font-size: var(--fs-md); font-weight: 650; letter-spacing: -.02em; }
	.curation-heading p, .curation-empty { margin: 3px 0 0; color: var(--text-dim); font-size: var(--fs-sm); line-height: 1.5; }
	.curation-heading :global(button) { flex: none; }
	.curated-list, .investigation-group ul { display: grid; margin: 0; padding: 0; list-style: none; }
	.curated-list { gap: 0; }
	.curated-list > li, .investigation-group li { display: flex; align-items: flex-start; justify-content: space-between; gap: var(--space-3); min-width: 0; padding: var(--space-3) 0; border-top: 1px solid var(--border-subtle, var(--border)); }
	.saved-copy, .option-copy { display: grid; min-width: 0; gap: 4px; }
	.saved-title-row { display: flex; align-items: flex-start; flex-wrap: wrap; gap: var(--space-2); min-width: 0; }
	.saved-title-row strong { min-width: 0; font-size: var(--fs-sm); font-weight: 560; line-height: 1.45; overflow-wrap: anywhere; }
	.saved-copy small, .option-copy small, .investigation-group header small { color: var(--text-faint); font-size: var(--fs-xs); overflow-wrap: anywhere; }
	.saved-copy p, .option-copy p { margin: 0; color: var(--text-dim); font-size: var(--fs-xs); line-height: 1.45; overflow-wrap: anywhere; }
	.saved-copy > :global(button), .curated-list > li > :global(button) { flex: none; }
	.unavailable-note, .curation-error { color: var(--danger, #b42318) !important; }
	:global(.project-curation-dialog) { display: grid; grid-template-rows: auto minmax(0, 1fr) auto; width: min(760px, calc(100vw - 40px)); height: min(760px, calc(100vh - 40px)); padding: 0; overflow: hidden; }
	.curation-dialog-header, .curation-dialog-footer { display: flex; align-items: center; justify-content: space-between; gap: var(--space-4); padding: var(--space-4) var(--space-5); }
	.curation-dialog-header { border-bottom: 1px solid var(--border); }
	.curation-eyebrow { margin: 0 0 3px; color: var(--text-faint); font-size: var(--fs-xs); font-weight: 650; letter-spacing: .04em; text-transform: uppercase; }
	:global(.curation-dialog-title) { margin: 0; font-size: var(--fs-lg); font-weight: 650; letter-spacing: -.02em; }
	:global(.curation-dialog-description) { max-width: 540px; margin: 3px 0 0; color: var(--text-dim); font-size: var(--fs-sm); line-height: 1.45; }
	:global(.curation-close) { display: grid; width: 32px; height: 32px; flex: none; place-items: center; border: 1px solid transparent; border-radius: var(--radius-sm); background: transparent; color: var(--text-dim); cursor: pointer; }
	:global(.curation-close:hover) { border-color: var(--border); background: var(--bg-inset); color: var(--text); }
	.curation-dialog-body { display: grid; align-content: start; gap: var(--space-5); min-height: 0; overflow: auto; padding: var(--space-4) var(--space-5); }
	.investigation-group { display: grid; gap: var(--space-2); }
	.investigation-group > header { display: flex; align-items: baseline; justify-content: space-between; gap: var(--space-3); }
	.investigation-group h3 { margin: 0; font-size: var(--fs-sm); font-weight: 650; }
	.investigation-group ul { border-bottom: 1px solid var(--border-subtle, var(--border)); }
	.investigation-group li { align-items: center; }
	.investigation-group li > :global(button) { flex: none; }
	.curation-dialog-footer { justify-content: flex-end; border-top: 1px solid var(--border); }
	:global(.curation-done) { min-height: 36px; padding: 0 var(--space-3); border: 1px solid var(--border); border-radius: var(--radius-sm); background: var(--bg); color: var(--text); font: inherit; cursor: pointer; }
	:global(.curation-done:hover) { background: var(--bg-inset); }
	@media (max-width: 680px) {
		.curation-heading { align-items: flex-start; flex-direction: column; }
		.investigation-group li { align-items: flex-start; flex-direction: column; }
		:global(.project-curation-dialog) { width: calc(100vw - 24px); height: calc(100vh - 24px); }
	}
</style>

<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import { ipc } from '$lib/ipc';
	import type { ArtifactComparison, BoundResult, ResolvedArtifact } from '$lib/analytics-artifacts';
	import type { EvidenceItem } from '$lib/types';
	import { artifactCaveats, artifactSourceRevision } from '$lib/studio-report';
	import { Button } from '$lib/components/ui/button';
	import { Select } from '$lib/components/ui/select';
	import MarkdownContent from './MarkdownContent.svelte';
	import Icon from './Icon.svelte';
	import AnalyticalDiff from './AnalyticalDiff.svelte';
	import { session } from '$lib/session.svelte';

	let {
		resolved,
		viewState = $bindable({}),
		onstatechange,
		onrevisionchange,
		onrequestchange,
		reviewOnly = false,
		autoCompare = false,
		onadopted
	}: {
		resolved: ResolvedArtifact;
		viewState?: Record<string, unknown>;
		onstatechange?: (state: Record<string, unknown>) => void;
		onrevisionchange?: (revision: number) => void;
		onrequestchange?: (artifact: ResolvedArtifact, instruction: string) => void | Promise<void>;
		reviewOnly?: boolean;
		autoCompare?: boolean;
		onadopted?: (output: ResolvedArtifact) => void;
	} = $props();

	let displayed = $state<ResolvedArtifact>(untrack(() => resolved));
	let Presentation = $state<typeof import('./presentation/AnalyticalPresentation.svelte').default | null>(null);
	let rendererError = $state('');
	let actionError = $state('');
	let notice = $state('');
	let busy = $state(false);
	let instruction = $state('');
	let changeOpen = $state(false);
	let compareOpen = $state(false);
	let comparisonVersions = $state<ResolvedArtifact[]>([]);
	let comparisonReport = $state<ArtifactComparison | null>(null);
	let layoutEditing = $state(false);
	let specDraft = $state('');
	let operation = 0;
	let comparisonOperation = 0;
	let autoCompared = '';
	let caveats = $derived(artifactCaveats(displayed));
	let evidence = $derived(displayed.run.result.evidence.filter((item): item is EvidenceItem => Boolean(item && typeof item === 'object' && 'tool' in item)));
	let revisions = $derived([...displayed.artifact.revisions].sort((a, b) => b.revision - a.revision));
	let baseline = $derived(displayed.revision.revision !== displayed.artifact.current_revision
		? displayed.artifact.current_revision
		: displayed.revision.parent_revision ?? null);
	let initialState = $derived(JSON.parse(JSON.stringify(viewState)) as Record<string, unknown>);
	$effect(() => {
		const next = resolved;
		++operation; ++comparisonOperation;
		displayed = next;
		busy = false; actionError = ''; notice = ''; comparisonVersions = []; comparisonReport = null;
		compareOpen = false; layoutEditing = false; specDraft = JSON.stringify(next.revision.spec, null, 2);
		instruction = ''; changeOpen = false;
	});
	$effect(() => {
		const key = `${displayed.artifact.id}:${displayed.revision.revision}:${baseline}`;
		if (autoCompare && baseline && key !== autoCompared) { autoCompared = key; untrack(() => { void compare(); }); }
	});

	onMount(() => {
		let mounted = true;
		void import('./presentation/AnalyticalPresentation.svelte').then(module => {
			if (mounted) Presentation = module.default;
		}).catch(() => {
			if (mounted) rendererError = 'The interactive presentation could not be loaded. The saved answer and exact values remain available.';
		});
		return () => { mounted = false; ++operation; ++comparisonOperation; };
	});

	function updateState(state: Record<string, unknown>): void {
		try {
			const serialized = JSON.stringify(state);
			if (serialized.length > 65_536) throw new Error('Display selections are too large to save.');
			const next = JSON.parse(serialized) as Record<string, unknown>;
			viewState = next;
			onstatechange?.(next);
		} catch (cause) { actionError = cause instanceof Error ? cause.message : 'Display selections could not be saved.'; }
	}

	async function chooseRevision(value: string): Promise<void> {
		const revision = Number(value);
		if (!Number.isSafeInteger(revision) || revision === displayed.revision.revision) return;
		if (onrevisionchange) { onrevisionchange(revision); return; }
		const target = displayed;
		const request = ++operation;
		busy = true; actionError = ''; notice = '';
		try {
			const next = await ipc.artifactResolve(target.artifact.workspace, target.artifact.id, revision);
			if (request !== operation) return;
			displayed = next; compareOpen = false; comparisonVersions = []; comparisonReport = null;
			updateState({});
		} catch (cause) {
			if (request === operation) actionError = cause instanceof Error ? cause.message : String(cause);
		} finally { if (request === operation) busy = false; }
	}

	async function compare(): Promise<void> {
		if (compareOpen) { compareOpen = false; return; }
		if (!baseline) return;
		const target = displayed;
		const beforeRevision = Math.min(target.revision.revision, baseline);
		const afterRevision = Math.max(target.revision.revision, baseline);
		const request = ++comparisonOperation;
		actionError = '';
		try {
			const [before, after, report] = await Promise.all([
				ipc.artifactResolve(target.artifact.workspace, target.artifact.id, beforeRevision),
				ipc.artifactResolve(target.artifact.workspace, target.artifact.id, afterRevision),
				ipc.artifactCompare(target.artifact.workspace, target.artifact.id, beforeRevision, afterRevision)
			]);
			if (request !== comparisonOperation) return;
			comparisonVersions = [before, after]; comparisonReport = report; compareOpen = true;
		} catch (cause) { if (request === comparisonOperation) actionError = cause instanceof Error ? cause.message : String(cause); }
	}

	async function adopt(): Promise<void> {
		const target = displayed;
		const request = ++operation;
		busy = true; actionError = ''; notice = '';
		try {
			const next = await ipc.artifactAdopt(target.artifact.workspace, target.artifact.id, target.artifact.current_revision, target.revision.revision);
			if (request !== operation) return;
			displayed = next;
			compareOpen = false; comparisonReport = null; comparisonVersions = [];
			notice = 'Current revision updated. Existing reports keep their pinned versions.';
			onadopted?.(next);
		} catch (cause) { if (request === operation) actionError = cause instanceof Error ? cause.message : String(cause); }
		finally { if (request === operation) busy = false; }
	}

	async function saveLayout(): Promise<void> {
		const target = displayed;
		let spec: unknown;
		const request = ++operation;
		busy = true; actionError = ''; notice = '';
		try {
			try { spec = JSON.parse(specDraft); }
			catch { throw new Error('The presentation must be valid JSON.'); }
			const { validatePresentation } = await import('$lib/presentation/validate');
			const validation = validatePresentation(spec, target.results);
			if (!validation.valid) throw new Error(`The layout was not saved: ${validation.issues.map(issue => issue.message).join(' ')}`);
			const next = await ipc.artifactRevise(target.artifact.workspace, target.artifact.id, target.artifact.current_revision, { run_id: target.run.id, spec: validation.spec!, change_note: `Presentation edit based on revision ${target.revision.revision}.` });
			if (request !== operation) return;
			layoutEditing = false;
			if (onrevisionchange) onrevisionchange(next.revision.revision);
			else { displayed = next; updateState({}); }
			notice = 'Presentation revision saved. The analytical run is unchanged.';
		} catch (cause) { if (request === operation) actionError = cause instanceof Error ? cause.message : String(cause); }
		finally { if (request === operation) busy = false; }
	}

	async function requestChange(requestedInstruction = instruction): Promise<void> {
		const text = requestedInstruction.trim();
		if (!text || !onrequestchange || busy) return;
		// Capture the explicit workspace/artifact/revision, never a global active selection.
		const target = displayed;
		const request = ++operation;
		busy = true; actionError = ''; notice = '';
		try {
			await onrequestchange(target, text);
			if (request !== operation) return;
			instruction = ''; changeOpen = false;
			notice = 'Change requested in the conversation. This saved revision has not been overwritten.';
		} catch (cause) { if (request === operation) actionError = cause instanceof Error ? cause.message : String(cause); }
		finally { if (request === operation) busy = false; }
	}

	function scrollValues(event: Event, left: number, top = 0): void {
		const region = (event.currentTarget as HTMLElement).closest('.values-container')?.querySelector('.table-region');
		region?.scrollBy({ left, top, behavior: 'auto' });
	}
	function scrollKeys(event: KeyboardEvent): void {
		const offsets: Record<string, [number, number]> = { ArrowLeft: [-160, 0], ArrowRight: [160, 0], ArrowUp: [0, -80], ArrowDown: [0, 80], PageUp: [0, -320], PageDown: [0, 320] };
		const offset = offsets[event.key];
		if (offset) { event.preventDefault(); scrollValues(event, ...offset); }
	}
</script>

{#snippet exactTable(result: BoundResult, label: string)}
	<div class="values-container">
		<div class="table-region" role="region" aria-label={label}><table><thead><tr>{#each result.columns as column}<th scope="col">{column}</th>{/each}</tr></thead><tbody>{#each result.exact_rows as row}<tr>{#each result.columns as _, index}<td>{row[index] ?? '[unavailable]'}</td>{/each}</tr>{/each}</tbody></table></div>
		<div class="table-scroll-controls" aria-label={`Scroll ${label}`}><button type="button" onclick={event => scrollValues(event, -240)} onkeydown={scrollKeys} aria-label={`Scroll ${label} left`} title="Scroll left; arrow and Page keys scroll this table"><span aria-hidden="true">←</span></button><button type="button" onclick={event => scrollValues(event, 240)} onkeydown={scrollKeys} aria-label={`Scroll ${label} right`} title="Scroll right; arrow and Page keys scroll this table"><span aria-hidden="true">→</span></button></div>
	</div>
{/snippet}

<article class="artifact" aria-label={`Saved output: ${displayed.artifact.title}`} data-artifact-id={displayed.artifact.id} data-artifact-revision={displayed.revision.revision}>
	<header class="artifact-header">
		<div class="artifact-heading"><h2>{displayed.artifact.title}</h2><span class="revision-caption">Revision {displayed.revision.revision}{displayed.revision.revision === displayed.artifact.current_revision ? ' · current' : ' · historical'}</span></div>
		<div class="artifact-actions">
			<Select ariaLabel="Pinned artifact revision" value={String(displayed.revision.revision)} options={revisions.map(revision => ({ value: String(revision.revision), label: `Revision ${revision.revision}${revision.revision === displayed.artifact.current_revision ? ' · current' : ''}` }))} onValueChange={value => void chooseRevision(value)} disabled={busy} />
			{#if baseline}<Button variant="ghost" size="sm" aria-expanded={compareOpen} onclick={() => void compare()}>{compareOpen ? 'Close comparison' : 'Compare'}</Button>{/if}
			{#if onrequestchange}<Button variant="ghost" size="sm" aria-expanded={changeOpen} onclick={() => changeOpen = !changeOpen}><Icon name="pencil" size={14} />Request a change</Button>{/if}
		</div>
	</header>
	{#if actionError}<p class="inline-error" role="alert">{actionError}</p>{:else if notice}<p class="quiet-note" role="status">{notice}</p>{/if}
	{#if displayed.revision.revision !== displayed.artifact.current_revision}
		<div class="historical-note"><span>This placement stays on revision {displayed.revision.revision}.</span><Button variant="ghost" size="sm" disabled={busy} onclick={() => void adopt()}>Use as current revision</Button></div>
	{/if}
	{#if changeOpen && onrequestchange}
		<form class="change-form" onsubmit={event => { event.preventDefault(); requestChange(); }}>
			<label>What should change?<textarea aria-label="Requested analytical change" bind:value={instruction} placeholder="For example, exclude refunded orders and compare the result." rows="3"></textarea></label>
			<p class="quiet-note">Fella will create a new analysis against this output. The saved result stays intact.</p>
			<div class="form-actions"><Button variant="ghost" size="sm" onclick={() => changeOpen = false}>Cancel</Button><Button size="sm" type="submit" disabled={!instruction.trim() || busy}>{busy ? 'Sending…' : 'Send to conversation'}</Button></div>
		</form>
	{/if}
	{#if caveats.length}
		<details class="caveats"><summary>{caveats.length} recorded {caveats.length === 1 ? 'caveat' : 'caveats'}</summary><ul>{#each caveats as caveat}<li>{caveat}</li>{/each}</ul></details>
	{/if}

	{#if compareOpen && comparisonReport && comparisonVersions.length === 2}
		<section class="comparison" aria-label="Saved revision comparison">
			<AnalyticalDiff comparison={comparisonReport} {busy} error={actionError}
				onadopt={() => void adopt()}
				onkeep={() => { compareOpen = false; notice = 'Current version kept. The proposed version remains in history.'; }}
				onopenrevision={revision => void chooseRevision(String(revision))}
				onopenconsumer={consumer => { if (session.catalog.workspace === displayed.artifact.workspace) session.openStudioTool(consumer.document_id); }} />
			<details><summary>Exact before and after records</summary><div class="comparison-versions">
				{#each comparisonVersions as version (version.revision.revision)}
					<section><h4>Revision {version.revision.revision}</h4><p class="quiet-note">Run {version.run.id} · source revision {artifactSourceRevision(version)}</p><MarkdownContent text={version.run.result.text} />
						{#each version.results as result (result.id)}
							{@render exactTable(result, `Revision ${version.revision.revision} exact values`)}
							{#if !result.complete}<p class="quiet-note">Incomplete saved result: {result.exact_rows.length} of {result.row_count} rows.</p>{/if}
						{/each}
					</section>
				{/each}
			</div></details>
			<details><summary>Presentation changes</summary><div class="comparison-versions">{#each comparisonVersions as version (version.revision.revision)}<pre aria-label={`Revision ${version.revision.revision} presentation`}>{JSON.stringify(version.revision.spec, null, 2)}</pre>{/each}</div></details>
		</section>
	{/if}

	<div class="artifact-presentation" hidden={reviewOnly}>
		{#if displayed.revision.spec && Presentation && !rendererError}
			{#key `${displayed.artifact.id}:${displayed.revision.revision}`}
				<svelte:boundary>
					<Presentation spec={displayed.revision.spec} results={displayed.results} {initialState} onstatechange={updateState} onaction={onrequestchange ? action => { if (action.type === 'continue') void requestChange(action.message); } : undefined} />
					{#snippet failed()}
						<p class="quiet-note" role="status">The interactive presentation is unavailable. The saved answer and exact values are unchanged.</p>
						<MarkdownContent text={displayed.run.result.text} />
					{/snippet}
				</svelte:boundary>
			{/key}
			<details class="saved-answer"><summary>Original saved answer</summary><MarkdownContent text={displayed.run.result.text} /></details>
		{:else}
			{#if rendererError}<p class="quiet-note" role="status">{rendererError}</p>{/if}
			<MarkdownContent text={displayed.run.result.text || 'No narrative answer was recorded.'} />
		{/if}
	</div>

	<details class="exact-values"><summary>Exact saved values</summary>
		{#if !displayed.results.length}<p class="quiet-note">No bound tabular results were saved.</p>{/if}
		{#each displayed.results as result (result.id)}
			<section class="result-table"><h3>{result.id}</h3><p class="quiet-note">{result.complete ? `${result.row_count} rows · complete saved result` : `Incomplete saved result: ${result.exact_rows.length} of ${result.row_count} rows`}</p>
				{#if !result.exact_rows.length && result.row_count > 0}<p>Exact values are unavailable. Rounded display values have not been substituted.</p>{:else}
					{@render exactTable(result, `Exact values for ${result.id}`)}
				{/if}
			</section>
		{/each}
	</details>
	<details class="provenance"><summary>Evidence, sources and definitions</summary>
		<dl><dt>Original question</dt><dd>{displayed.run.question}</dd><dt>Artifact</dt><dd>{displayed.artifact.id} · revision {displayed.revision.revision}</dd><dt>Saved run</dt><dd>{displayed.run.id}</dd><dt>Source revision</dt><dd>{artifactSourceRevision(displayed)}</dd><dt>Verification</dt><dd>{displayed.run.verification?.status ?? displayed.run.result.status}</dd></dl>
		{#if displayed.run.source_scope?.used_sources.length}<h3>Sources used</h3><ul>{#each displayed.run.source_scope.used_sources as source}<li>{source.source}{source.table ? ` · ${source.table}` : ''}</li>{/each}</ul>{/if}
		{#if displayed.run.investigation_context?.definitions.length}<h3>Definitions at execution</h3><ul>{#each displayed.run.investigation_context.definitions as definition}<li>{definition.text}<span class="quiet-note"> · revision {definition.revision}</span></li>{/each}</ul>{/if}
		{#if displayed.run.contract?.assumptions.length}<h3>Assumptions</h3><ul>{#each displayed.run.contract.assumptions as assumption}<li>{assumption}</li>{/each}</ul>{/if}
		{#each evidence as item, index (item.id ?? index)}<details class="evidence-item"><summary>{item.note || item.result_summary || item.tool}</summary><p class="quiet-note">{item.id ?? 'No evidence ID'} · {item.tool}</p>{#if item.sql}<pre>{item.sql}</pre>{/if}{#if item.output}<pre>{item.output}</pre>{/if}{#if item.verifier_disposition}<p>{item.verifier_disposition.state}: {item.verifier_disposition.reason}</p>{/if}</details>{/each}
		<p class="quiet-note">Saved source revisions support historical inspection. Exact recomputation requires the original inputs to remain available.</p>
	</details>
	<details class="presentation-source"><summary>Presentation source</summary>
		<p class="quiet-note">Schema {displayed.revision.schema_version}. Editing the layout creates a revision; it does not rerun or change the analytical data.</p>
		{#if layoutEditing}
			<label class="layout-editor">JSON presentation<textarea aria-label="JSON presentation spec" rows="10" bind:value={specDraft} spellcheck="false"></textarea></label>
			<div class="form-actions"><Button size="sm" variant="ghost" onclick={() => layoutEditing = false}>Cancel</Button><Button size="sm" disabled={busy || !specDraft.trim() || specDraft === JSON.stringify(displayed.revision.spec, null, 2)} onclick={() => void saveLayout()}>Save layout revision</Button></div>
		{:else}<pre>{JSON.stringify(displayed.revision.spec, null, 2)}</pre><Button size="sm" variant="ghost" onclick={() => { specDraft = JSON.stringify(displayed.revision.spec, null, 2); layoutEditing = true; }}>Edit layout</Button>{/if}
	</details>
</article>

<style>
	.artifact { min-width: 0; display: grid; gap: var(--space-4); color: var(--text); font-size: var(--fs-sm); line-height: 1.6; container-type: inline-size; }
	.artifact-header { display: flex; align-items: start; justify-content: space-between; gap: var(--space-3); flex-wrap: wrap; }
	.artifact-heading { min-width: 0; flex: 1 1 180px; }
	h2 { margin: 0; font-size: 19px; font-weight: 600; line-height: 1.35; overflow-wrap: anywhere; }
	h3, h4 { margin: 0 0 var(--space-2); font-size: var(--fs-sm); font-weight: 600; }
	p { margin: 0; }
	.revision-caption, .quiet-note { color: var(--text-faint); font-size: var(--fs-xs); overflow-wrap: anywhere; }
	.artifact-actions { display: flex; align-items: center; flex-wrap: wrap; gap: var(--space-1); min-width: 0; }
	.artifact-actions :global(.fella-ui-select) { width: auto; max-width: 210px; min-height: 30px; border-radius: 999px; font-size: var(--fs-xs); }
	.inline-error { color: var(--err); font-size: var(--fs-xs); overflow-wrap: anywhere; }
	.historical-note { display: flex; align-items: center; justify-content: space-between; flex-wrap: wrap; gap: var(--space-2); color: var(--text-faint); font-size: var(--fs-xs); }
	.form-actions { display: flex; align-items: center; justify-content: end; gap: var(--space-2); }
	.change-form { display: grid; gap: var(--space-3); padding: var(--space-4); background: var(--bg-inset); border-radius: var(--radius-window); }
	label { display: grid; gap: var(--space-2); color: var(--text-dim); }
	textarea { box-sizing: border-box; width: 100%; min-width: 0; resize: vertical; border: 1px solid var(--border); border-radius: var(--radius); background: var(--bg-raised); color: var(--text); padding: 10px 12px; font: inherit; line-height: 1.5; }
	textarea:focus-visible, summary:focus-visible, .table-region:focus-visible { outline: 2px solid var(--brand); outline-offset: 3px; }
	summary { width: fit-content; max-width: 100%; cursor: pointer; color: var(--text-dim); font-size: var(--fs-xs); overflow-wrap: anywhere; }
	details[open] > summary { margin-bottom: var(--space-3); }
	ul { margin: 0; padding-left: var(--space-4); color: var(--text-dim); }
	.caveats { padding: 10px 14px; border-radius: var(--radius); background: var(--bg-inset); }
	.saved-answer { margin-top: var(--space-4); }
	.exact-values, .provenance, .presentation-source { min-width: 0; }
	.result-table + .result-table { margin-top: var(--space-4); }
	.table-region { max-width: 100%; max-height: 400px; margin-top: var(--space-2); overflow: auto; overscroll-behavior: contain; border-radius: var(--radius-sm); background: var(--bg-inset); }
	.table-scroll-controls { display: flex; justify-content: end; gap: 2px; margin-top: 2px; }
	.table-scroll-controls button { border: 0; border-radius: var(--radius-sm); padding: 2px 9px; background: transparent; color: var(--text-faint); cursor: pointer; }
	.table-scroll-controls button:hover { background: var(--bg-inset); }
	.table-scroll-controls button:focus-visible { outline: 2px solid var(--brand); outline-offset: 2px; }
	table { width: max-content; min-width: 100%; border-collapse: collapse; font-size: var(--fs-xs); font-variant-numeric: tabular-nums; }
	th, td { padding: 8px 12px; text-align: left; vertical-align: top; white-space: pre-wrap; max-width: 340px; overflow-wrap: anywhere; }
	th { position: sticky; top: 0; background: var(--bg-inset); color: var(--text-dim); font-weight: 600; }
	td { border-top: 1px solid var(--border); }
	dl { display: grid; grid-template-columns: minmax(0, 110px) minmax(0, 1fr); gap: 6px 14px; font-size: var(--fs-xs); margin: 0 0 var(--space-4); }
	dt { color: var(--text-faint); } dd { margin: 0; overflow-wrap: anywhere; }
	.provenance ul, .provenance .evidence-item { margin-bottom: var(--space-3); }
	pre { white-space: pre-wrap; overflow-wrap: anywhere; max-height: 320px; overflow: auto; background: var(--bg-inset); border-radius: var(--radius); padding: var(--space-3); font: 11px/1.6 var(--mono); }
	.layout-editor textarea { font: 12px/1.6 var(--mono); }
	.comparison { display: grid; gap: var(--space-3); background: var(--bg-inset); padding: var(--space-4); border-radius: var(--radius-window); }
	.comparison-versions { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: var(--space-4); }
	.comparison-versions > section { min-width: 0; display: grid; align-content: start; gap: var(--space-3); }
	@container (max-width: 600px) { .comparison-versions { grid-template-columns: minmax(0, 1fr); } }
</style>

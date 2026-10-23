<script lang="ts">
	import { untrack } from 'svelte';
	import { requestArtifactChange } from '$lib/commands';
	import { session } from '$lib/session.svelte';
	import type { Message } from '$lib/types';
	import type { ResolvedArtifact } from '$lib/analytics-artifacts';
	import { Button } from '$lib/components/ui/button';
	import { DropdownMenu } from '$lib/components/ui/dropdown-menu';
	import Icon from './Icon.svelte';
	import MarkdownContent from './MarkdownContent.svelte';
	import AnalyticalPresentation from './presentation/AnalyticalPresentation.svelte';
	import ArtifactView from './ArtifactView.svelte';
	import { ensureAnalyticalOutput, outputJobs, saveOutputToStudio } from '$lib/analytical-output.svelte';
	import { transcriptPresentation } from '$lib/presentation/snapshot';
	let { message, onopeninvestigationrun }: { message: Message; onopeninvestigationrun?: (turnId: string) => void } = $props();
	let resolved = $state<ResolvedArtifact | null>(null);
	let error = $state('');
	let loading = $state(false);
	let saving = $state(false);
	let reviewing = $state(false);
	async function inspect(review = false): Promise<void> {
		if (!workspace || workspace !== session.catalog.workspace) return;
		try {
			const output = resolved?.run.id === turnId ? resolved : await ensureAnalyticalOutput(message, workspace);
			if (!output) { error = 'Could not open the saved result.'; return; }
			resolved = output;
			if (review) reviewing = !reviewing;
			else { session.setContentLayout('notebook'); session.openResultToolTab(output.artifact.id, output.revision.revision, output.artifact.title); }
		} catch (cause) { error = cause instanceof Error ? cause.message : String(cause); }
	}
	let sequence = 0;
	let retry = $state(0);
	let workspace = $derived(message.answer?.workspace?.path);
	let turnId = $derived(message.answer?.turn_id);
	let snapshot = $derived(transcriptPresentation(message));
	let display = $derived(snapshot ?? (resolved && resolved.run.id === turnId ? { spec: resolved.revision.spec, results: resolved.results } : null));

	$effect(() => {
		const id = message.artifact_id;
		const revision = message.artifact_revision;
		const owner = workspace;
		const turn = turnId;
		const waitingForAnswer = message.pending;
		const retryRequested = retry;
		const hasPresentation = Boolean(id || message.answer?.presentation_spec);
		// Rendering a saved presentation is synchronous. Artifact resolution is
		// only needed for old uncached data or a deliberate save/change action.
		if (!retryRequested && id && untrack(() => !!snapshot)) return;
		if (untrack(() => resolved && resolved.artifact.id === id && resolved.revision.revision === revision)) return;
		const request = ++sequence;
		error = '';
		// Plain answers are a valid completion. Don't report an absent optional
		// layout as a failed render or make a pointless artifact lookup.
		if (!hasPresentation) { loading = false; return; }
		if (!owner || !turn || waitingForAnswer) return;
		loading = true;
		void ensureAnalyticalOutput(message, owner)
			.then((value) => {
				if (request !== sequence) return;
				if (value) resolved = value;
				else error = outputJobs[turn]?.error || 'The presentation could not be displayed. Your saved analysis is still available.';
			})
			.catch((cause) => {
				if (request === sequence) error = cause instanceof Error ? cause.message : String(cause);
			})
			.finally(() => {
				if (request === sequence) loading = false;
			});
		return () => { sequence++; };
	});

	async function save(): Promise<void> {
		if (!workspace) return;
		saving = true;
		error = '';
		try {
			const output = resolved?.run.id === turnId ? resolved : await ensureAnalyticalOutput(message, workspace);
			if (output) { resolved = output; await saveOutputToStudio(output); }
			else error = 'This output could not be saved. Your analysis is still available.';
		}
		catch (cause) { error = cause instanceof Error ? cause.message : String(cause); }
		finally { saving = false; }
	}
</script>

<section class="analytical-answer" aria-label="Analytical answer">
	{#if display}
			<AnalyticalPresentation spec={display.spec} results={display.results} initialState={message.presentation_state ?? {}} onstatechange={(state) => {
				message.presentation_state = state;
				session.persist();
			}} onaction={(action) => {
				if (session.catalog.workspace !== workspace || !workspace) return;
				void (async () => {
					const target = resolved?.run.id === turnId ? resolved : await ensureAnalyticalOutput(message, workspace!);
					if (target) await requestArtifactChange(target, action.message);
				})().catch(cause => error = cause instanceof Error ? cause.message : String(cause));
			}} />
	{:else}
		<MarkdownContent text={message.answer?.text ?? message.text} />
		{#if error}<div class="view-note" role="status"><p>The answer’s layout could not be displayed. Your analysis is still available.</p><Button variant="ghost" size="sm" disabled={loading} onclick={() => { resolved = null; retry++; }}>Retry display</Button><details><summary>Display details</summary><p>{error}</p></details></div>{/if}
	{/if}
	<div class="answer-actions">
		<Button variant="ghost" size="sm" class="save-report" disabled={saving} onclick={() => void save()}>{saving ? 'Saving…' : 'Create report'}</Button>
		<DropdownMenu.Root>
			<DropdownMenu.Trigger class="answer-more" aria-label="Answer actions" title="Answer actions"><Icon name="more-horizontal" size={16} /></DropdownMenu.Trigger>
			<DropdownMenu.Portal>
				<DropdownMenu.Content class="fella-ui-menu-content" align="end" sideOffset={4}>
					<DropdownMenu.Item class="fella-ui-menu-item" onSelect={() => void inspect()}>Open result</DropdownMenu.Item>
					{#if (message.artifact_revision ?? 1) > 1}
						<DropdownMenu.Item class="fella-ui-menu-item" onSelect={() => void inspect(true)}>{reviewing ? 'Close review' : `Review changes · revision ${message.artifact_revision}`}</DropdownMenu.Item>
					{/if}
					{#if message.answer?.turn_id && onopeninvestigationrun}
						<DropdownMenu.Item class="fella-ui-menu-item" onSelect={() => onopeninvestigationrun?.(message.answer!.turn_id!)}>Open in Investigation</DropdownMenu.Item>
					{/if}
				</DropdownMenu.Content>
			</DropdownMenu.Portal>
		</DropdownMenu.Root>
	</div>
	{#if reviewing && resolved}<ArtifactView {resolved} reviewOnly autoCompare onrequestchange={requestArtifactChange} onadopted={output => { resolved = output; reviewing = false; }} />{/if}
</section>

<style>
	.analytical-answer { min-width: 0; }
	.answer-actions { display:flex; align-items:center; gap:4px; margin-top:8px; }
	.answer-actions :global(.save-report) { height:28px; padding-inline:8px; color:var(--text-dim); font-size:var(--fs-xs); }
	:global(.answer-more) { display:grid; place-items:center; width:28px; height:28px; padding:0; border:0; border-radius:var(--radius-chip); background:transparent; color:var(--text-faint); cursor:pointer; }
	:global(.answer-more:hover), :global(.answer-more[data-state='open']) { background:var(--bg-inset); color:var(--text); }
	.view-note { margin:8px 0; color:var(--text-dim); font-size:var(--fs-xs); line-height:1.6; }
</style>

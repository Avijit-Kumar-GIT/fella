<script lang="ts">
	import { untrack } from 'svelte';
	import { requestArtifactChange } from '$lib/commands';
	import { session } from '$lib/session.svelte';
	import type { Message } from '$lib/types';
	import type { ResolvedArtifact } from '$lib/analytics-artifacts';
	import { Button } from '$lib/components/ui/button';
	import MarkdownContent from './MarkdownContent.svelte';
	import AnalyticalPresentation from './presentation/AnalyticalPresentation.svelte';
	import ArtifactView from './ArtifactView.svelte';
	import { ensureAnalyticalOutput, outputJobs, saveOutputToStudio } from '$lib/analytical-output.svelte';
	import { transcriptPresentation } from '$lib/presentation/snapshot';
	let { message }: { message: Message } = $props();
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
	let display = $derived(snapshot ?? (resolved && resolved.run.id === turnId ? { program: resolved.revision.program, results: resolved.results } : null));

	$effect(() => {
		const id = message.artifact_id;
		const revision = message.artifact_revision;
		const owner = workspace;
		const turn = turnId;
		const waitingForAnswer = message.pending;
		const retryRequested = retry;
		// Rendering a saved presentation is synchronous. Artifact resolution is
		// only needed for old uncached data or a deliberate save/change action.
		if (!retryRequested && id && untrack(() => !!snapshot)) return;
		if (untrack(() => resolved && resolved.artifact.id === id && resolved.revision.revision === revision)) return;
		const request = ++sequence;
		error = '';
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
			<AnalyticalPresentation program={display.program} results={display.results} initialState={message.presentation_state ?? {}} onstatechange={(state) => {
				message.presentation_state = state;
				session.persist();
			}} onaction={(action) => {
				if (session.catalog.workspace !== workspace || !workspace) return;
				void (async () => {
					const target = resolved?.run.id === turnId ? resolved : await ensureAnalyticalOutput(message, workspace!);
					if (target) await requestArtifactChange(target, action.message);
				})().catch(cause => error = cause instanceof Error ? cause.message : String(cause));
			}} />
		<div class="answer-actions">
			<Button variant="ghost" size="sm" onclick={() => void inspect()}>Open result</Button>
			{#if (message.artifact_revision ?? 1) > 1}<Button variant="ghost" size="sm" aria-expanded={reviewing} onclick={() => void inspect(true)}>{reviewing ? 'Close review' : 'Review changes'}</Button>{/if}
			<Button variant="ghost" size="sm" disabled={saving} onclick={() => void save()}>{saving ? 'Opening report…' : 'Create report'}</Button>
			{#if message.artifact_revision}<span>Analysis · revision {message.artifact_revision}</span>{/if}
		</div>
		{#if reviewing && resolved}<ArtifactView {resolved} reviewOnly autoCompare onrequestchange={requestArtifactChange} onadopted={output => { resolved = output; reviewing = false; }} />{/if}
		<details class="original"><summary>Analysis details</summary><MarkdownContent text={message.answer?.text ?? message.text} /></details>
	{:else}
		<MarkdownContent text={message.answer?.text ?? message.text} />
		{#if error}<div class="view-note" role="status"><p>The answer’s layout could not be displayed. Your analysis is still available.</p><Button variant="ghost" size="sm" disabled={loading} onclick={() => { resolved = null; retry++; }}>Retry display</Button><details><summary>Display details</summary><p>{error}</p></details></div>{/if}
	{/if}
</section>

<style>
	.analytical-answer { min-width: 0; }
	.answer-actions { display:flex; flex-wrap:wrap; align-items:center; gap:8px; margin-top:10px; color:var(--text-faint); font-size:var(--fs-xs); }
	.original { margin-top:8px; color:var(--text-dim); font-size:var(--fs-sm); }
	summary { cursor:pointer; color:var(--text-faint); font-size:var(--fs-xs); margin-bottom:8px; }
	.view-note { margin:8px 0; color:var(--text-dim); font-size:var(--fs-xs); line-height:1.6; }
</style>

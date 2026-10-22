<script lang="ts">
	import { requestArtifactChange } from '$lib/commands';
	import { session } from '$lib/session.svelte';
	import type { Message } from '$lib/types';
	import type { ResolvedArtifact } from '$lib/analytics-artifacts';
	import { Button } from '$lib/components/ui/button';
	import MarkdownContent from './MarkdownContent.svelte';
	let { message }: { message: Message } = $props();
	let resolved = $state<ResolvedArtifact | null>(null);
	let error = $state('');
	let loading = $state(false);
	let saving = $state(false);
	let outputModule = $state<typeof import('$lib/analytical-output.svelte') | null>(null);
	let sequence = 0;
	let workspace = $derived(message.answer?.workspace?.path);
	let turnId = $derived(message.answer?.turn_id);
	let job = $derived(turnId && outputModule ? outputModule.outputJobs[turnId] : undefined);

	$effect(() => {
		const id = message.artifact_id;
		const revision = message.artifact_revision;
		const owner = workspace;
		const turn = turnId;
		const waitingForAnswer = message.pending;
		if (resolved && resolved.artifact.id === id && resolved.revision.revision === revision) return;
		const request = ++sequence;
		resolved = null;
		error = '';
		if (!owner || !turn || waitingForAnswer) return;
		loading = true;
		void import('$lib/analytical-output.svelte')
			.then(async (module) => {
				outputModule = module;
				return module.ensureAnalyticalOutput(message, owner);
			})
			.then((value) => {
				if (request !== sequence) return;
				if (value) resolved = value;
				else error = outputModule?.outputJobs[turn]?.error || 'The interactive answer is unavailable. The verified answer is shown below.';
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
		if (!resolved || !outputModule) return;
		saving = true;
		error = '';
		try { await outputModule.saveOutputToStudio(resolved); }
		catch (cause) { error = cause instanceof Error ? cause.message : String(cause); }
		finally { saving = false; }
	}
</script>

<section class="analytical-answer" aria-label="Analytical answer">
	{#if resolved}
		{#await import('./presentation/AnalyticalPresentation.svelte')}
			<p class="answer-status" role="status">Loading the interactive answer…</p>
		{:then module}
			<module.default program={resolved.revision.program} results={resolved.results} onaction={(action) => {
				if (session.catalog.workspace !== resolved?.artifact.workspace) return;
				const target = resolved;
				void requestArtifactChange(target, action.message).catch(cause => error = cause instanceof Error ? cause.message : String(cause));
			}} />
		{:catch}
			<MarkdownContent text={message.answer?.text ?? message.text} />
			<p class="view-note" role="status">The interactive answer could not be displayed. The verified answer is unchanged.</p>
		{/await}
		<div class="answer-actions">
			<Button variant="ghost" size="sm" disabled={saving} onclick={() => void save()}>{saving ? 'Saving…' : resolved.artifact.saved ? 'Open in Studio' : 'Save to Studio'}</Button>
			<span>Analysis · revision {resolved.revision.revision}</span>
		</div>
		<details class="original"><summary>Analysis details</summary><MarkdownContent text={resolved.run.result.text} /></details>
	{:else if loading || job?.busy || (turnId && !error && !job?.error)}
		<p class="answer-status" role="status">Preparing the interactive answer…</p>
	{:else}
		<MarkdownContent text={message.answer?.text ?? message.text} />
		{#if error || job?.error}<p class="view-note" role="status">{error || job?.error}</p>{/if}
	{/if}
</section>

<style>
	.analytical-answer { min-width: 0; }
	.answer-actions { display:flex; flex-wrap:wrap; align-items:center; gap:8px; margin-top:10px; color:var(--text-faint); font-size:var(--fs-xs); }
	.answer-status { margin: 8px 0; color: var(--text-dim); font-size: var(--fs-sm); }
	.original { margin-top:8px; color:var(--text-dim); font-size:var(--fs-sm); }
	summary { cursor:pointer; color:var(--text-faint); font-size:var(--fs-xs); margin-bottom:8px; }
	.view-note { margin:8px 0; color:var(--text-dim); font-size:var(--fs-xs); line-height:1.6; }
</style>

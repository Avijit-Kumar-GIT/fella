<script lang="ts">
	import { onMount } from 'svelte';
	import { ipc } from '$lib/ipc';
	import { session } from '$lib/session.svelte';
	import { requestArtifactChange } from '$lib/commands';
	import type { Message } from '$lib/types';
	import type { ResolvedArtifact } from '$lib/analytics-artifacts';
	import { Button } from '$lib/components/ui/button';
	import MarkdownContent from './MarkdownContent.svelte';
	let { message }: { message: Message } = $props();
	let resolved = $state<ResolvedArtifact | null>(null);
	let error = $state('');
	let saving = $state(false);
	let presentationValid = $state(false);
	let outputModule = $state<typeof import('$lib/analytical-output.svelte') | null>(null);
	let sequence = 0;
	let workspace = $derived(message.answer?.workspace?.path);
	let turnId = $derived(message.answer?.turn_id);
	let job = $derived(turnId && outputModule ? outputModule.outputJobs[turnId] : undefined);
	onMount(() => {
		let mounted = true;
		void import('$lib/analytical-output.svelte').then(module => { if (mounted) outputModule = module; });
		return () => { mounted = false; };
	});
	$effect(() => {
		const id = message.artifact_id, revision = message.artifact_revision, owner = workspace;
		const request = ++sequence;
		resolved = null; error = '';
		if (!id || !owner) return;
		void ipc.artifactResolve(owner, id, revision).then(async value => {
			const { validatePresentation } = await import('$lib/presentation/validate');
			if (request === sequence) { presentationValid = validatePresentation(value.revision.program, value.results).valid; resolved = value; }
		})
			.catch(cause => { if (request === sequence) error = `Could not open the saved interactive view. ${String(cause)}`; });
		return () => { sequence++; };
	});
	async function save(): Promise<void> {
		if (!resolved || !outputModule) return;
		saving = true; error = '';
		try { await outputModule.saveOutputToStudio(resolved); }
		catch (cause) { error = cause instanceof Error ? cause.message : String(cause); }
		finally { saving = false; }
	}
</script>

<section class="analytical-answer" aria-label="Interactive analytical answer">
	{#if resolved}
		{#if !presentationValid}<MarkdownContent text={resolved.run.result.text} />{/if}
		{#await import('./presentation/AnalyticalPresentation.svelte')}
			<MarkdownContent text={message.text} />
		{:then module}
			<module.default program={resolved.revision.program} results={resolved.results} onaction={(action) => {
				if (session.catalog.workspace !== resolved?.artifact.workspace) return;
				const target = resolved;
				void requestArtifactChange(target, action.message).catch(cause => error = cause instanceof Error ? cause.message : String(cause));
			}} />
		{:catch}
			<MarkdownContent text={message.text} />
			<p role="status">The interactive view is unavailable. The original answer is unchanged.</p>
		{/await}
		<div class="answer-actions"><Button variant="ghost" size="sm" disabled={saving} onclick={() => void save()}>{saving ? 'Saving…' : resolved.artifact.saved ? 'Open in Studio' : 'Save to Studio'}</Button><span>Saved analysis · revision {resolved.revision.revision}</span></div>
		<details class="original"><summary>Original analytical answer</summary><MarkdownContent text={resolved.run.result.text} /></details>
	{:else if message.artifact_id}
		<MarkdownContent text={message.text} />
	{/if}
	{#if !message.artifact_id && workspace && turnId}
		<div class="answer-actions">
			{#if job?.busy}<span role="status">Designing and building view…</span><Button variant="ghost" size="sm" onclick={() => workspace && turnId && outputModule?.cancelAnalyticalOutput(turnId, workspace)}>Cancel view</Button>
			{:else if outputModule}<Button variant="ghost" size="sm" onclick={() => workspace && outputModule && void outputModule.createAnalyticalOutput(message, workspace, session.activeChat.model || undefined)}>Create interactive view</Button>{/if}
		</div>
	{/if}
	{#if error || job?.error}<p class="view-note" role="status">{error || job?.error}</p>{/if}
</section>
<style>
	.analytical-answer { min-width:0; }
	.answer-actions { display:flex; flex-wrap:wrap; align-items:center; gap:8px; margin-top:10px; color:var(--text-faint); font-size:var(--fs-xs); }
	.original { margin-top:8px; color:var(--text-dim); font-size:var(--fs-sm); }
	summary { cursor:pointer; color:var(--text-faint); font-size:var(--fs-xs); margin-bottom:8px; }
	.view-note { margin:8px 0; color:var(--text-dim); font-size:var(--fs-xs); line-height:1.6; }
</style>

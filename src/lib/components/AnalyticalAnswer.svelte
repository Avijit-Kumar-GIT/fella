<script lang="ts">
	import { onDestroy, untrack } from 'svelte';
	import { requestArtifactChange } from '$lib/commands';
	import { session } from '$lib/session.svelte';
	import type { Message } from '$lib/types';
	import type { ResolvedArtifact } from '$lib/analytics-artifacts';
	import { Button } from '$lib/components/ui/button';
	import { DropdownMenu } from '$lib/components/ui/dropdown-menu';
	import Icon from './Icon.svelte';
	import MarkdownContent from './MarkdownContent.svelte';
	import AnalyticalPresentation from './presentation/AnalyticalPresentation.svelte';
	import { ensureAnalyticalOutput, outputJobs, saveOutputToStudio, updateMessagePresentation } from '$lib/analytical-output.svelte';
	import { transcriptPresentation } from '$lib/presentation/snapshot';
	let { message, onopeninvestigationrun }: { message: Message; onopeninvestigationrun?: (turnId: string) => void } = $props();
	let resolved = $state<ResolvedArtifact | null>(null);
	function resolvedMatchesMessage(output: ResolvedArtifact | null): output is ResolvedArtifact {
		return Boolean(output && output.run.id === message.answer?.turn_id
			&& (!message.artifact_id || output.artifact.id === message.artifact_id)
			&& (!message.artifact_revision || output.revision.revision === message.artifact_revision));
	}
	let error = $state('');
	let loading = $state(false);
	let saving = $state(false);
	let reviewing = $state(false);
	let ReviewView = $state<typeof import('./ArtifactView.svelte').default | null>(null);
	let reviewError = $state('');
	let SourceView = $state<typeof import('./PresentationSource.svelte').default | null>(null);
	let sourceOpen = $state(false);
	let sourceLoading = $state(false);
	let sourceError = $state('');
	let sourceSequence = 0;
	let sourceMessage: Message | null = null;
	let sourceWorkspace: string | undefined;
	let sourceTurn: string | undefined;
	function sourceCurrent() {
		return !destroyed && message === sourceMessage && workspace === sourceWorkspace
			&& turnId === sourceTurn && session.catalog.workspace === sourceWorkspace;
	}
	async function viewSource() {
		if (sourceLoading || !display) return;
		if (!workspace || workspace !== session.catalog.workspace) { sourceError = 'Open this answer’s workspace to view its JSON.'; return; }
		const request = ++sourceSequence;
		sourceMessage = message; sourceWorkspace = workspace; sourceTurn = turnId;
		sourceLoading = true; sourceError = '';
		try {
			const output = await ensureAnalyticalOutput(message, workspace);
			if (request !== sourceSequence || !sourceCurrent()) return;
			if (!output) throw new Error(outputJobs[turnId!]?.error || 'Could not open the saved presentation.');
			const component = SourceView ?? (await import('./PresentationSource.svelte')).default;
			if (request !== sourceSequence || !sourceCurrent()) return;
			resolved = output; SourceView = component; sourceOpen = true;
		} catch (cause) {
			if (request === sourceSequence && sourceCurrent()) sourceError = cause instanceof Error ? cause.message : String(cause);
		} finally { if (request === sourceSequence) sourceLoading = false; }
	}
	let inspectSequence = 0;
	let destroyed = false;
	onDestroy(() => { destroyed = true; inspectSequence++; sourceSequence++; });
	let copied = $state(false);
	async function copyAnswer(): Promise<void> {
		try { await navigator.clipboard.writeText(message.answer?.text ?? message.text); copied = true; }
		catch { error = 'Could not copy this answer.'; }
	}
	async function inspect(review = false): Promise<void> {
		if (!workspace || workspace !== session.catalog.workspace) return;
		const request = ++inspectSequence;
		if (review && reviewing) { reviewing = false; reviewError = ''; return; }
		if (!review) reviewing = false;
		const source = message;
		const owner = workspace;
		const turn = turnId;
		const current = () => !destroyed && request === inspectSequence && message === source
			&& workspace === owner && turnId === turn
			&& session.catalog.workspace === owner;
		if (review) { reviewing = true; reviewError = ''; }
		try {
			const output = resolvedMatchesMessage(resolved) ? resolved : await ensureAnalyticalOutput(source, owner);
			if (!current()) return;
			if (!output) throw new Error('Could not open the saved result.');
			resolved = output;
			if (review) {
				const component = ReviewView ?? (await import('./ArtifactView.svelte')).default;
				if (current()) ReviewView = component;
			}
			else { session.setContentLayout('notebook'); session.openResultTool(output.artifact.id, output.revision.revision, output.artifact.title); }
		} catch (cause) {
			if (!current()) return;
			const detail = cause instanceof Error ? cause.message : String(cause);
			if (review) { reviewing = false; reviewError = `Review could not be opened: ${detail}`; }
			else error = detail;
		}
	}
	let sequence = 0;
	let retry = $state(0);
	let workspace = $derived(message.answer?.workspace?.path);
	let turnId = $derived(message.answer?.turn_id);
	let snapshot = $derived(transcriptPresentation(message));
	let display = $derived(snapshot ?? (resolvedMatchesMessage(resolved) ? { spec: resolved.revision.spec, results: resolved.results } : null));
	$effect(() => {
		// A replacement answer invalidates pending review work, even in a reused component.
		// Resolving an uncached answer may assign its artifact revision itself;
		// that bookkeeping must not cancel the review the user just requested.
		void message; void workspace; void turnId; void session.catalog.workspace;
		return () => { inspectSequence++; reviewing = false; reviewError = ''; sourceSequence++; sourceOpen = false; sourceLoading = false; sourceError = ''; };
	});

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
			const output = resolvedMatchesMessage(resolved) ? resolved : await ensureAnalyticalOutput(message, workspace);
			if (output) { resolved = output; await saveOutputToStudio(output); }
			else error = 'This output could not be saved. Your analysis is still available.';
		}
		catch (cause) { error = cause instanceof Error ? cause.message : String(cause); }
		finally { saving = false; }
	}
</script>

<section class="analytical-answer" aria-label="Analytical answer">
	<div class="answer-surface">
	{#if display}
			<AnalyticalPresentation spec={display.spec} results={display.results} initialState={message.presentation_state ?? {}} onstatechange={(state) => {
				message.presentation_state = state;
				session.persist();
			}} onaction={(action) => {
				if (session.catalog.workspace !== workspace || !workspace) return;
				void (async () => {
					const target = resolvedMatchesMessage(resolved) ? resolved : await ensureAnalyticalOutput(message, workspace!);
					if (target) await requestArtifactChange(target, action.message);
				})().catch(cause => error = cause instanceof Error ? cause.message : String(cause));
			}} />
	{:else}
		<MarkdownContent text={message.answer?.text ?? message.text} />
		{#if error}<div class="view-note" role="status"><p>The answer’s layout could not be displayed. Your analysis is still available.</p><Button variant="ghost" size="sm" disabled={loading} onclick={() => { resolved = null; retry++; }}>Retry display</Button><details><summary>Display details</summary><p>{error}</p></details></div>{/if}
	{/if}
	</div>
	<div class="answer-actions">
		<Button variant="ghost" size="sm" class="save-report" disabled={saving} onclick={() => void save()}>{saving ? 'Saving…' : 'Create report'}</Button>
		<Button variant="ghost" size="icon" class="answer-copy" aria-label={copied ? 'Answer copied' : 'Copy answer'} title={copied ? 'Copied' : 'Copy answer'} onclick={() => void copyAnswer()}><Icon name={copied ? 'check' : 'copy'} size={12} /></Button>
		<DropdownMenu.Root>
			<DropdownMenu.Trigger class="answer-more" aria-label="Answer actions" title="Answer actions"><Icon name="more-horizontal" size={16} /></DropdownMenu.Trigger>
			<DropdownMenu.Portal>
				<DropdownMenu.Content class="fella-ui-menu-content" align="end" sideOffset={4}>
					<DropdownMenu.Item class="fella-ui-menu-item" onSelect={() => void inspect()}>Open result</DropdownMenu.Item>
					{#if display}<DropdownMenu.Item class="fella-ui-menu-item" disabled={sourceLoading} onSelect={() => void viewSource()}>{sourceLoading ? 'Opening JSON…' : 'View JSON'}</DropdownMenu.Item>{/if}
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
	{#if sourceError}<p class="view-note" role="alert">{sourceError}</p>{/if}
	{#if sourceOpen && SourceView && resolved}
		<SourceView bind:open={sourceOpen} artifact={resolved} iscurrent={sourceCurrent} onupdated={output => {
			if (!sourceCurrent()) return;
			updateMessagePresentation(message, output);
			resolved = output;
			session.persist();
		}} />
	{/if}
	{#if reviewError}<p class="view-note" role="status">{reviewError}</p>{/if}
	{#if reviewing}
		{#if ReviewView && resolved}<ReviewView {resolved} reviewOnly autoCompare onrequestchange={requestArtifactChange} onadopted={output => { resolved = output; reviewing = false; }} />
		{:else}<p class="view-note" role="status">Opening review…</p>{/if}
	{/if}
</section>

<style>
	.analytical-answer { min-width:0; color:var(--chat-body); font-size:13px; line-height:21px; }
	.answer-surface { min-width:0; padding:14px 16px; border-radius:16px; background:var(--bg-raised); }
	.answer-surface :global(.analytical-document) { color:var(--chat-body); font-size:13px; line-height:21px; }
	:global(.answer-copy) { width:28px; min-width:28px; height:28px; min-height:28px; padding:0; border-radius:50%; color:var(--text-dim); }
	.answer-actions { display:flex; align-items:center; gap:4px; margin-top:8px; padding-inline:4px; }
	.answer-actions :global(.save-report) { height:28px; padding-inline:8px; color:var(--text-dim); font-size:var(--fs-xs); }
	:global(.answer-more) { display:grid; place-items:center; width:28px; height:28px; padding:0; border:0; border-radius:var(--radius-chip); background:transparent; color:var(--text-faint); cursor:pointer; }
	:global(.answer-more:hover), :global(.answer-more[data-state='open']) { background:var(--bg-inset); color:var(--text); }
	.view-note { margin:8px 0; color:var(--text-dim); font-size:var(--fs-xs); line-height:1.6; }
</style>

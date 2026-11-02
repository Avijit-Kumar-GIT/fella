<script lang="ts">
	import { onDestroy } from 'svelte';
	import type { ResolvedArtifact } from '$lib/analytics-artifacts';
	import { changeArtifactPresentation } from '$lib/commands';
	import { ipc } from '$lib/ipc';
	import { session } from '$lib/session.svelte';
	import { Dialog } from '$lib/components/ui/dialog';
	import { Button } from '$lib/components/ui/button';
	import { Textarea } from '$lib/components/ui/textarea';

	let { open = $bindable(false), artifact, onupdated, iscurrent }: {
		open?: boolean; artifact: ResolvedArtifact;
		onupdated: (output: ResolvedArtifact) => void;
		iscurrent: () => boolean;
	} = $props();
	let draft = $state('');
	let instruction = $state('');
	let pending = $state(false);
	let error = $state('');
	let notice = $state('');
	let operation = 0;
	let destroyed = false;
	const id = $props.id();
	$effect(() => {
		draft = JSON.stringify(artifact.revision.spec, null, 2);
		// A successful revision updates artifact. Keep its acknowledgement visible
		// instead of immediately clearing it during this rehydration.
		instruction = ''; error = '';
	});
	onDestroy(() => { destroyed = true; operation++; });
	function close(value = false) { operation++; open = value; pending = false; }
	function current(target: ResolvedArtifact, request: number) {
		return !destroyed && open && request === operation && artifact === target
			&& session.catalog.workspace === target.artifact.workspace && iscurrent();
	}
	async function copy() {
		if (pending || !iscurrent() || session.catalog.workspace !== artifact.artifact.workspace) return;
		const target = artifact;
		const request = ++operation;
		pending = true; error = ''; notice = '';
		try {
			await navigator.clipboard.writeText(JSON.stringify(target.revision.spec, null, 2));
			if (current(target, request)) notice = 'Saved JSON copied.';
		} catch (cause) { if (current(target, request)) error = cause instanceof Error ? cause.message : String(cause); }
		finally { if (request === operation) pending = false; }
	}
	async function apply(kind: 'json' | 'instruction') {
		if (pending) return;
		if (!iscurrent() || session.catalog.workspace !== artifact.artifact.workspace) {
			error = 'Open this answer’s workspace before editing its presentation.'; return;
		}
		const target = artifact;
		const request = ++operation;
		const base = target.artifact.current_revision;
		pending = true; error = ''; notice = '';
		try {
			let output: ResolvedArtifact | null;
			if (kind === 'json') {
				let input: unknown;
				try { input = JSON.parse(draft); } catch { throw new Error('Enter valid JSON before saving.'); }
				const { validatePresentation } = await import('$lib/presentation/validate');
				if (!current(target, request)) return;
				const checked = validatePresentation(input, target.results);
				if (!checked.valid || !checked.spec) throw new Error(checked.issues.map(issue => `${issue.path}: ${issue.message}`).join('\n') || 'The presentation is invalid.');
				const candidate = await ipc.artifactRevise(target.artifact.workspace, target.artifact.id, base, {
					run_id: target.run.id, spec: checked.spec, change_note: `JSON presentation edit based on revision ${target.revision.revision}.`
				});
				if (!current(target, request)) return;
				output = await ipc.artifactAdopt(target.artifact.workspace, target.artifact.id, base, candidate.revision.revision);
			} else {
				if (!instruction.trim()) throw new Error('Describe a view change first.');
				output = await changeArtifactPresentation(target, instruction.trim());
			}
			if (!current(target, request)) return;
			if (output) {
				onupdated(output);
				draft = JSON.stringify(output.revision.spec, null, 2);
				notice = `Revision ${output.revision.revision} saved.`;
			} else notice = 'Request sent to the analytical flow. No presentation revision was applied here.';
		} catch (cause) { if (current(target, request)) error = cause instanceof Error ? cause.message : String(cause); }
		finally { if (request === operation) pending = false; }
	}
</script>

<Dialog.Root {open} onOpenChange={close}>
	<Dialog.Portal>
		<Dialog.Overlay class="fella-ui-dialog-overlay" />
		<Dialog.Content class="fella-ui-dialog-content presentation-source">
			<Dialog.Title class="source-title">Presentation JSON</Dialog.Title>
			<Dialog.Description class="source-description">Edit the saved view for revision {artifact.revision.revision}. The canonical answer and evidence stay unchanged.</Dialog.Description>
			<div class="source-scroll" aria-busy={pending}>
				<label for={`${id}-json`}>Revision JSON</label>
				<Textarea id={`${id}-json`} class="source-json" bind:value={draft} disabled={pending} spellcheck={false} />
				<div class="source-actions">
					<Button variant="ghost" size="sm" disabled={pending} onclick={() => void copy()}>Copy saved JSON</Button>
					<Button size="sm" disabled={pending} onclick={() => void apply('json')}>Save revision</Button>
				</div>
				<label for={`${id}-instruction`}>Describe a view change</label>
				<Textarea id={`${id}-instruction`} bind:value={instruction} disabled={pending} placeholder="For example, show the results as a comparison table" rows={2} />
				<div class="source-actions"><Button variant="outline" size="sm" disabled={pending || !instruction.trim()} onclick={() => void apply('instruction')}>Apply view change</Button></div>
				{#if error}<p class="source-error" role="alert">{error}</p>{/if}
				{#if notice}<p class="source-notice" role="status">{notice}</p>{/if}
				{#if pending}<p class="source-notice" role="status">Working…</p>{/if}
			</div>
			<div class="source-actions"><Button variant="ghost" size="sm" onclick={() => close()}>Cancel</Button></div>
		</Dialog.Content>
	</Dialog.Portal>
</Dialog.Root>

<style>
	:global(.presentation-source) { width:min(760px, calc(100vw - 32px)); max-height:80vh; display:flex; flex-direction:column; gap:var(--space-3); overflow:hidden; }
	:global(.source-title) { margin:0; font-size:var(--fs-lg); color:var(--text); }
	:global(.source-description) { margin:0; color:var(--text-dim); font-size:var(--fs-xs); }
	.source-scroll { min-height:0; overflow:auto; display:flex; flex-direction:column; gap:var(--space-2); }
	.source-scroll label { color:var(--text-dim); font-size:var(--fs-xs); }
	:global(.source-json) { min-height:200px; height:32vh; flex-shrink:0; font-family:var(--font-mono, monospace); font-size:var(--fs-xs); tab-size:2; }
	.source-actions { display:flex; flex-wrap:wrap; justify-content:flex-end; gap:var(--space-2); }
	.source-error, .source-notice { margin:0; font-size:var(--fs-xs); white-space:pre-wrap; overflow-wrap:anywhere; }
	.source-error { color:var(--status-error, var(--text)); }
	.source-notice { color:var(--text-dim); }
</style>

<script lang="ts">
	import { ipc } from '$lib/ipc';
	import { requestArtifactChange } from '$lib/commands';
	import type { ResolvedArtifact } from '$lib/analytics-artifacts';
	import ArtifactView from './ArtifactView.svelte';
	let { workspace, artifactId, revision }: { workspace: string; artifactId: string; revision: number } = $props();
	let resolved = $state<ResolvedArtifact | null>(null);
	let error = $state('');
	let request = 0;
	$effect(() => {
		const owner = workspace, id = artifactId, version = revision, sequence = ++request;
		resolved = null; error = '';
		void ipc.artifactResolve(owner, id, version).then(value => {
			if (sequence !== request) return;
			if (value.artifact.workspace !== owner || value.artifact.id !== id || value.revision.revision !== version) throw new Error('This result does not match the selected workspace and revision.');
			resolved = value;
		}).catch(cause => { if (sequence === request) error = cause instanceof Error ? cause.message : String(cause); });
		return () => { request++; };
	});
</script>
<div class="result-document">
	{#if resolved}<ArtifactView {resolved} onrequestchange={requestArtifactChange} onadopted={output => resolved = output} />
	{:else if error}<p role="alert">{error}</p>{:else}<p role="status">Opening saved result…</p>{/if}
</div>
<style>.result-document { flex: 1; min-width: 0; min-height: 0; overflow: auto; padding: clamp(16px, 3vw, 32px); }</style>

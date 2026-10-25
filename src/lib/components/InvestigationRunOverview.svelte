<script lang="ts">
	import type { AnalysisTurn, EvidenceItem } from '$lib/types';
	import { analystStepForTool } from '$lib/run-progress';
	import Icon from './Icon.svelte';
	let { ids, records, selectedId, loading = false, error = '', onselect, oninspect }: {
		ids:string[]; records:Record<string, AnalysisTurn>; selectedId:string|null;
		loading?:boolean; error?:string; onselect:(id:string)=>void; oninspect:(id:string)=>void;
	} = $props();
	let run = $derived(selectedId ? records[selectedId] : null);
	let steps = $derived.by(() => {
		if (!run) return [];
		const evidence = run.result.evidence as EvidenceItem[];
		if (run.trace?.steps.length) return run.trace.steps.map(step => ({ id:step.id, label:analystStepForTool(step.operation), summary:step.summary, ms:step.duration_ms, error:!step.success, sql:evidence.find(item => item.id === step.id)?.sql }));
		return evidence.map((item,index) => ({ id:item.id ?? String(index), label:analystStepForTool(item.tool), summary:item.error ?? item.result_summary, ms:item.ms, error:Boolean(item.error), sql:item.sql }));
	});
	function time(ms:number|undefined):string { return ms == null ? '' : ms < 1000 ? `${ms} ms` : `${(ms / 1000).toFixed(ms < 10000 ? 1 : 0)} s`; }
</script>

<div class="run-overview">
	<section>
		<h3>Run</h3>
		{#if ids.length}
			<div class="run-choices" role="group" aria-label="Select analysis run">
				{#each ids as id,index (id)}<button class:active={selectedId === id} aria-pressed={selectedId === id} onclick={() => onselect(id)}>Run {index + 1}</button>{/each}
			</div>
		{:else}<p class="empty">No analysis runs yet.</p>{/if}
		{#if run}<div class="run-meta"><i class:failed={run.result.status === 'failed'}></i><span>{run.result.status === 'failed' ? 'Failed' : 'Finished'}{#if run.trace?.elapsed_ms != null} · {time(run.trace.elapsed_ms)}{/if}</span>{#if run.trace?.model}<code>{run.trace.model}</code>{/if}</div>{/if}
	</section>
	{#if loading}<p class="empty" role="status">Opening saved run…</p>{:else if error}<p class="error" role="alert">{error}</p>
	{:else if run}
		<section>
			<h3>Steps</h3>
			<ol class="steps">
				{#each steps as step,index (step.id)}
					<li class:error={step.error}>
						<span class="step-number">{#if step.error}<Icon name="alert" size={9} />{:else}{index + 1}{/if}</span>
						<div class="step-copy"><div class="step-heading"><strong>{step.label}</strong><small>{time(step.ms)}</small></div>{#if step.summary}<p>{step.summary}</p>{/if}{#if step.sql}<details><summary><Icon name="chevron-right" size={11} />SQL</summary><pre>{step.sql}</pre></details>{/if}</div>
					</li>
				{/each}
			</ol>
			{#if !steps.length}<p class="empty">No execution steps were recorded for this run.</p>{/if}
		<button class="run-inspect" onclick={() => oninspect(run!.id)}>Open run details <Icon name="chevron-right" size={11} /></button>
		</section>
	{/if}
</div>

<style>
	.run-overview { display:grid; min-width:0; gap:20px; }
	h3 { margin:0 4px 8px; color:var(--text-dim); font-size:12px; font-weight:400; }
	.run-choices { display:flex; min-width:0; gap:2px; padding:4px; overflow-x:auto; scrollbar-width:thin; border-radius:999px; background:var(--bg-raised); }
	.run-choices button { flex:1 0 auto; min-width:70px; padding:6px 8px; border-radius:999px; color:var(--text-dim); font-size:12px; font-weight:400; }
	.run-choices button.active { background:var(--bg-raised-2); color:var(--text); }
	.run-meta { display:flex; align-items:center; gap:8px; margin:8px 4px 0; color:var(--text-dim); font-size:12px; }
	.run-meta > i { width:6px; height:6px; flex:none; border-radius:50%; background:var(--ok); }
	.run-meta > i.failed { background:var(--err); }
	.run-inspect { display:flex; align-items:center; gap:2px; margin-left:auto; color:var(--text-faint); font-size:11.5px; }
	.run-meta code { max-width:45%; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; margin-left:auto; color:var(--text-faint); font:11px var(--mono); }
	.steps { margin:0; padding:0; list-style:none; }
	.steps > li { position:relative; display:grid; min-width:0; grid-template-columns:20px minmax(0,1fr); gap:8px; padding-bottom:12px; }
	.steps > li:not(:last-child)::before { position:absolute; top:20px; bottom:0; left:9px; width:1px; background:var(--border); content:''; }
	.step-number { position:relative; z-index:1; display:grid; width:15px; height:15px; margin:3px auto 0; place-items:center; border-radius:50%; background:var(--bg-raised-2); color:var(--text-dim); font:9px var(--mono); }
	.error .step-number { background:color-mix(in srgb,var(--warn) 20%,transparent); color:var(--warn); }
	.step-copy { min-width:0; }
	.step-heading { display:flex; align-items:baseline; gap:8px; }
	.step-heading strong { flex:1; min-width:0; color:var(--text); font-size:12.5px; font-weight:400; }
	.step-heading small { color:var(--text-faint); font:11px var(--mono); }
	.step-copy p { margin:2px 0 0; color:var(--text-dim); font-size:12px; line-height:17px; overflow-wrap:anywhere; }
	details { margin-top:6px; }
	summary { display:flex; width:fit-content; align-items:center; gap:4px; list-style:none; color:var(--text-dim); font-size:11.5px; cursor:pointer; }
	summary::-webkit-details-marker { display:none; }
	details[open] summary :global(svg) { transform:rotate(90deg); }
	pre { max-width:100%; margin:6px 0 0; padding:10px 12px; overflow:auto; border:1px solid var(--border); border-radius:12px; background:var(--bg-inset); color:var(--chat-body); font:10.5px/16px var(--mono); white-space:pre-wrap; overflow-wrap:anywhere; }
	.empty, .error { margin:0 4px; color:var(--text-faint); font-size:12.5px; line-height:19px; }
	.error { color:var(--err); }
</style>

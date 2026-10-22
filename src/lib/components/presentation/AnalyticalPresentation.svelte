<script lang="ts">
	import { Renderer } from '@openuidev/svelte-lang';
	import { untrack } from 'svelte';
	import { analyticalLibrary } from '../../presentation/library';
	import { validatePresentation } from '../../presentation/validate';
	import { resolvePresentationState, sanitizeState, validateResults } from '../../presentation/results';
	import type { BoundResult, PresentationAction, PresentationState } from '../../presentation/types';
	import { providePresentation } from './context';
	import ResultTable from './ResultTable.svelte';
	let { program, results, initialState = {}, onstatechange, onaction }: {
		program: string;
		results: BoundResult[];
		initialState?: PresentationState;
		onstatechange?: (state: PresentationState) => void;
		onaction?: (action: PresentationAction) => void;
	} = $props();
	let state = $state<PresentationState>(untrack(() => sanitizeState(initialState)));
	let lastInitial = untrack(() => initialState);
	let lastProgram = untrack(() => program);
	$effect(() => {
		if (initialState !== lastInitial || program !== lastProgram) {
			lastInitial = initialState; lastProgram = program; state = sanitizeState(initialState);
		}
	});
	let validation = $derived(validatePresentation(program, results));
	let effectiveState = $derived(validation.root ? resolvePresentationState(validation.root, state, results) : {});
	let safeResults = $derived.by(() => { try { validateResults(results); return results; } catch { return []; } });
	providePresentation({
		get results() { return results; },
		get state() { return effectiveState; },
		get interactive() { return !!onaction; },
		update(key, value) { state = sanitizeState({ ...effectiveState, [key]: value }); onstatechange?.(JSON.parse(JSON.stringify(effectiveState))); },
		action(action) { onaction?.({ ...action, state: sanitizeState(action.state) }); }
	});
</script>

<div class="analytical-presentation" data-presentation-version="fella-openui-v1">
	{#if validation.valid}
		<Renderer response={program} library={analyticalLibrary} />
	{:else}
		<div class="fallback" role="status"><p>This presentation could not be displayed. The saved analysis has not changed.</p>
			<details><summary>Presentation details</summary><ul>{#each validation.issues as issue}<li>{issue.message}</li>{/each}</ul></details>
		</div>
		{#each safeResults as result}<section class="fallback-result"><h3>Saved result: {result.id}</h3><ResultTable {result} /></section>{/each}
	{/if}
</div>
<style>
	.analytical-presentation { container-type: inline-size; min-width: 0; color: var(--text); }
	.fallback { color: var(--text-dim); font-size: .85rem; line-height: 1.6; } .fallback p { margin: 0 0 .6rem; }
	summary { cursor: pointer; color: var(--text-faint); font-size: .75rem; } ul { padding-left: 1.2rem; }
	.fallback-result { margin-top: 1.2rem; } h3 { font-size: .8rem; font-weight: 500; }
</style>

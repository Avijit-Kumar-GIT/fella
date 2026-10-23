<script lang="ts">
	import { untrack } from 'svelte';
	import { CatalogRenderer } from '@json-render/svelte';
	import { validatePresentation } from '../../presentation/validate';
	import { evaluatePresentationAction } from '../../presentation/runtime';
	import { resolvePresentationState, sanitizeState, validateResults } from '../../presentation/results';
	import type { BoundResult, PresentationAction, PresentationSpec, PresentationState } from '../../presentation/types';
	import { presentationRegistry } from '../../presentation/library';
	import { providePresentation } from './context';
	import ResultTable from './ResultTable.svelte';
	let { spec, results, initialState = {}, onstatechange, onaction }: {
		spec: PresentationSpec;
		results: BoundResult[];
		initialState?: PresentationState;
		onstatechange?: (state: PresentationState) => void;
		onaction?: (action: PresentationAction) => void;
	} = $props();
	let presentationState = $state<PresentationState>({});
	presentationState = untrack(() => sanitizeState(initialState));
	let actionError = $state('');
	let previousSpec = untrack(() => JSON.stringify(spec));
	let safeResults = $derived.by(() => { try { validateResults(results); return results; } catch { return []; } });
	let validation = $derived(validatePresentation(spec, safeResults));
	let effectiveState = $derived(validation.valid && validation.spec ? resolvePresentationState(validation.spec, presentationState, safeResults) : sanitizeState(presentationState));

	$effect(() => {
		const next = JSON.stringify(spec);
		const incoming = resolvePresentationState(spec, sanitizeState(initialState), safeResults);
		if (next !== previousSpec) {
			previousSpec = next;
			presentationState = incoming;
		} else if (JSON.stringify(incoming) !== JSON.stringify(presentationState)) presentationState = incoming;
	});

	function commit(next: PresentationState): void {
		presentationState = sanitizeState(next);
		actionError = '';
		onstatechange?.(JSON.parse(JSON.stringify(presentationState)));
	}

	function update(key: string, value: unknown): void {
		commit({ ...effectiveState, [key]: value });
	}

	function runAction(action: unknown): void {
		const safe = evaluatePresentationAction(action);
		if (!safe) { actionError = 'This presentation action is unavailable.'; return; }
		if (safe.type === 'follow_up') {
			onaction?.({ type: 'continue', message: safe.message, state: effectiveState });
			return;
		}
		const canSet = Object.values(spec.elements).some((element) => ['Select', 'SegmentedControl', 'Checkbox'].includes(element.type) && element.props.stateKey === safe.stateKey);
		if (!canSet) { actionError = 'This action can only update a control in this presentation.'; return; }
		commit({ ...effectiveState, [`control:${safe.stateKey}`]: safe.value });
	}

	providePresentation({
		get results() { return safeResults; },
		get state() { return effectiveState; },
		get interactive() { return !!onaction; },
		update,
		runAction,
		action(action) { onaction?.({ ...action, state: sanitizeState({ ...effectiveState, ...action.state }) }); }
	});
</script>

<div class="analytical-presentation" data-presentation-version="fella-json-render-v1">
	{#if validation.valid && validation.spec}
		<CatalogRenderer spec={validation.spec} registry={presentationRegistry} state={effectiveState} />
		{#if actionError}<p class="action-error" role="status">{actionError}</p>{/if}
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
	.action-error { margin: .6rem 0 0; color: var(--danger, #bd5b5b); font-size: .78rem; }
</style>

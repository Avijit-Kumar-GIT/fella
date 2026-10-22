<script lang="ts">
	import { untrack } from 'svelte';
	import { validatePresentation } from '../../presentation/validate';
	import { createPresentationStore, evaluatePresentationAction } from '../../presentation/runtime';
	import { resolvePresentationState, sanitizeState, validateResults } from '../../presentation/results';
	import type { BoundResult, PresentationAction, PresentationState } from '../../presentation/types';
	import { providePresentation } from './context';
	import PresentationNode from './PresentationNode.svelte';
	import ResultTable from './ResultTable.svelte';
	let { program, results, initialState = {}, onstatechange, onaction }: {
		program: string;
		results: BoundResult[];
		initialState?: PresentationState;
		onstatechange?: (state: PresentationState) => void;
		onaction?: (action: PresentationAction) => void;
	} = $props();
	let presentationState: PresentationState = $state({});
	presentationState = untrack(() => sanitizeState(initialState));
	let actionError = $state('');
	let previousProgram = untrack(() => program);
	let safeResults = $derived.by(() => { try { validateResults(results); return results; } catch { return []; } });
	let validation = $derived(validatePresentation(program, safeResults, presentationState));
	let effectiveState = $derived(validation.root ? resolvePresentationState(validation.root, presentationState, safeResults, validation.stateDefaults) : sanitizeState(presentationState));

	$effect(() => {
		const nextProgram = program;
		const incoming = sanitizeState(initialState);
		if (nextProgram !== previousProgram) {
			previousProgram = nextProgram;
			presentationState = incoming;
		} else if (JSON.stringify(incoming) !== JSON.stringify(presentationState)) presentationState = incoming;
	});

	function commit(next: PresentationState): void {
		presentationState = sanitizeState(next);
		actionError = '';
		onstatechange?.(JSON.parse(JSON.stringify(presentationState)));
	}

	function update(key: string, value: unknown): void {
		commit({ ...presentationState, [key]: value });
	}

	function updateVariable(name: string, value: unknown): void {
		const defaults = validation.stateDefaults;
		if (!Object.hasOwn(defaults, name)) return;
		const fallback = defaults[name];
		if (!(typeof value === typeof fallback || value === null && fallback === null)) return;
		if (typeof value === 'string' && value.length > 256 || typeof value === 'number' && (!Number.isFinite(value) || Math.abs(value) > 1e12) || value !== null && !['string', 'number', 'boolean'].includes(typeof value)) return;
		commit({ ...presentationState, [name]: value });
	}

	function runAction(action: unknown): void {
		const defaults = validation.stateDefaults;
		const store = createPresentationStore(defaults, presentationState);
		try {
			const outcome = evaluatePresentationAction(action, defaults, store);
			const next = sanitizeState(store.getSnapshot());
			if (outcome.error) { actionError = outcome.error; return; }
			if (JSON.stringify(next) !== JSON.stringify(presentationState)) commit(next);
			if (outcome.followUp) onaction?.({ type: 'continue', message: outcome.followUp, state: next });
		} finally { store.dispose(); }
	}

	providePresentation({
		get results() { return safeResults; },
		get state() { return effectiveState; },
		get variables() { return effectiveState; },
		get interactive() { return !!onaction; },
		update,
		updateVariable,
		runAction,
		action(action) { onaction?.({ ...action, state: sanitizeState({ ...effectiveState, ...action.state }) }); }
	});
</script>

<div class="analytical-presentation" data-presentation-version="fella-openui-v2">
	{#if validation.valid && validation.root}
		<PresentationNode node={validation.root} />
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

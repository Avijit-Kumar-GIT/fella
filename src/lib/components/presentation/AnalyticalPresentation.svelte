<script lang="ts">
	import { untrack } from 'svelte';
	import { CatalogRenderer } from '@json-render/svelte';
	import { validatePresentation } from '../../presentation/validate';
	import { evaluatePresentationAction } from '../../presentation/runtime';
	import { resolvePresentationState, sanitizeState, validateResults, type FilterSelection, type RowFilterValue } from '../../presentation/results';
	import type { BoundResult, PresentationAction, PresentationSpec, PresentationState } from '../../presentation/types';
	import { presentationRegistry } from '../../presentation/library';
	import { providePresentation } from './context';
	import ResultTable from './ResultTable.svelte';
	import Icon from '../Icon.svelte';
	let { spec, results, initialState = {}, interactionScope, showActiveFilters = true, onstatechange, onaction }: {
		spec: PresentationSpec;
		results: BoundResult[];
		initialState?: PresentationState;
		interactionScope?: string;
		showActiveFilters?: boolean;
		onstatechange?: (state: PresentationState) => void;
		onaction?: (action: PresentationAction) => void;
	} = $props();
	let presentationState = $state<PresentationState>({});
	presentationState = untrack(() => sanitizeState(initialState));
	let actionError = $state('');
	let previousSpec = untrack(() => JSON.stringify(spec));
	let safeResults = $derived.by(() => { try { validateResults(results); return results; } catch { return []; } });
	let validation = $derived(validatePresentation(spec, safeResults));
	let effectiveState = $derived(validation.valid && validation.spec ? resolvePresentationState(validation.spec, presentationState, safeResults, interactionScope) : sanitizeState(presentationState));
	let activeFilters = $derived(Object.entries(effectiveState).flatMap(([key, value]) => {
		if (!key.startsWith('filter:') || !value || typeof value !== 'object') return [];
		const filter = value as FilterSelection;
		return filter.value === null ? [] : [{ key, filter }];
	}));

	$effect(() => {
		const next = JSON.stringify(spec);
		const incoming = resolvePresentationState(spec, sanitizeState(initialState), safeResults, interactionScope);
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

	function selectRows(resultId: string, interactionGroup: string | undefined, values: RowFilterValue[]): void {
		if (!values.length) return;
		const next = { ...effectiveState };
		const scope = interactionGroup ? `group:${interactionGroup}` : `result:${resultId}`;
		for (const { field, value, valueType } of values) {
			const key = `filter:selection:${scope}:${field}`;
			next[key] = { resultId, field, value, ...(interactionGroup ? { interactionGroup } : {}), ...(interactionScope ? { interactionScope } : {}), ...(valueType ? { valueType } : {}) };
		}
		commit(next);
	}

	function removeFilter(key: string): void {
		const next = { ...effectiveState };
		delete next[key];
		commit(next);
	}

	function clearFilters(): void {
		const next = Object.fromEntries(Object.entries(effectiveState).filter(([key]) => !key.startsWith('filter:')));
		commit(next);
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
		get interactionScope() { return interactionScope; },
		update,
		selectRows,
		runAction,
		action(action) { onaction?.({ ...action, state: sanitizeState({ ...effectiveState, ...action.state }) }); }
	});
</script>

<div class="analytical-presentation" data-presentation-version="fella-json-render-v1">
	{#if validation.valid && validation.spec}
		{#if showActiveFilters && activeFilters.length}<div class="active-filter-bar" aria-label="Active report filters"><span class="filter-summary">Filtered by</span>{#each activeFilters as item (item.key)}<button class="filter-chip" aria-label={`Remove filter ${item.filter.field}: ${item.filter.value}`} onclick={() => removeFilter(item.key)}>{item.filter.field}<strong>{item.filter.value}</strong><Icon name="x" size={12} /></button>{/each}<button class="filter-reset" onclick={clearFilters}>Reset</button></div>{/if}
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
	.active-filter-bar { display:flex; flex-wrap:wrap; align-items:center; gap:6px; min-width:0; margin:0 0 var(--space-3); }
	.filter-summary { color:var(--text-faint); font-size:var(--fs-xs); }
	.filter-chip { display:inline-flex; min-width:0; align-items:center; gap:6px; padding:4px 8px; border:1px solid var(--border); border-radius:var(--radius-chip); background:var(--bg-raised); color:var(--text-dim); font:inherit; font-size:var(--fs-xs); cursor:pointer; }
	.filter-chip strong { max-width:200px; overflow:hidden; color:var(--text); font-weight:500; text-overflow:ellipsis; white-space:nowrap; }
	.filter-reset { padding:4px 6px; border:0; background:transparent; color:var(--text-faint); font:inherit; font-size:var(--fs-xs); cursor:pointer; }
	.filter-chip:focus-visible, .filter-reset:focus-visible { outline:2px solid var(--brand); outline-offset:2px; }
</style>

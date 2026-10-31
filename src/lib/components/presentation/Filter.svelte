<script lang="ts">
	import type { BaseComponentProps } from '@json-render/svelte';
	import { exactValue, requireResult, resultFilterStateKey, type FilterSelection } from '../../presentation/results';
	import { usePresentation } from './context';
	let { props }: BaseComponentProps<{ resultId: string; field: string; name: string; title?: string; interactionGroup?: string }> = $props();
	const context = usePresentation();
	let menu: HTMLDetailsElement;
	let result = $derived(requireResult(context.sourceResults, props.resultId));
	let options = $derived([...new Map(result.rows.map((_, row) => {
		const column = result.columns.indexOf(props.field);
		const raw = result.rows[row][column];
		const value = exactValue(result, row, props.field);
		return [`${raw === null ? 'null' : typeof raw}\u0000${value}`, { value, label: raw === null ? '(empty)' : value, type: raw === null ? 'null' : typeof raw }];
	})).values()]);
	let stateKey = $derived(resultFilterStateKey(props.name, props.interactionGroup));
	let currentFilter = $derived(context.state[stateKey] as FilterSelection | undefined);
	let selected = $derived(currentFilter?.value ?? null);
	let selectedLabel = $derived(selected === null ? 'All saved values' : options.find(option => option.value === selected)?.label ?? selected);
	let selectedType = $derived(currentFilter?.valueType);
	function select(value: string | null, valueType?: string) {
		const column = result.columns.indexOf(props.field);
		const row = value === null ? -1 : result.rows.findIndex((_, rowIndex) => {
			const raw = result.rows[rowIndex][column];
			return exactValue(result, rowIndex, props.field) === value && (raw === null ? 'null' : typeof raw) === valueType;
		});
		const raw = row < 0 ? null : result.rows[row][column];
		const selectedType: FilterSelection['valueType'] = raw === null ? 'null' : typeof raw === 'string' ? 'string' : typeof raw === 'number' ? 'number' : typeof raw === 'boolean' ? 'boolean' : undefined;
		context.setFilter(stateKey, { resultId: props.resultId, field: props.field, value, ...(props.interactionGroup ? { interactionGroup: props.interactionGroup } : {}), ...(context.interactionScope ? { interactionScope: context.interactionScope } : {}), ...(selectedType ? { valueType: selectedType } : {}) });
		if (menu) menu.open = false;
	}
</script>
<div class="filter presentation-filter">
	<span class="label">{props.title ?? props.field}</span>
	<details bind:this={menu}><summary>{selectedLabel}<span aria-hidden="true">⌄</span></summary>
		<div class="options presentation-filter-options" role="group" aria-label={props.title ?? props.field}>
			<button aria-pressed={selected === null} onclick={() => select(null)}>All saved values</button>
			{#each options as option}<button aria-pressed={selected === option.value && selectedType === option.type} onclick={() => select(option.value, option.type)}>{option.label}</button>{/each}
		</div>
	</details>
	{#if !result.complete}<span class="scope">Choices come from the saved result sample. Compatible SQL outputs replay the selected value against the same source revision.</span>{/if}
</div>
<style>
	.filter { display: flex; min-width: 0; max-width: 100%; align-items: center; flex-wrap: wrap; gap: var(--space-2); font-size: var(--fs-sm); }
	.label, .scope { color: var(--text-dim); overflow-wrap: anywhere; }
	.scope { font-size: var(--fs-xs); }
	details { position: relative; min-width: 0; max-width: 100%; }
	summary { display: flex; min-height: 36px; max-width: 100%; align-items: center; justify-content: space-between; gap: var(--space-3); padding: 6px 12px; list-style: none; overflow-wrap: anywhere; cursor: pointer; }
	summary::-webkit-details-marker { display: none; }
	.options { position: absolute; z-index: 5; top: calc(100% + var(--space-1)); left: 0; max-height: 18rem; overflow-x: hidden; overflow-y: auto; padding: var(--space-1); }
	button { display: block; width: 100%; padding: 7px 9px; border: 0; border-radius: var(--radius-sm); background: transparent; color: var(--text); font-size: var(--fs-sm); text-align: left; overflow-wrap: anywhere; cursor: pointer; }
	button:hover, button[aria-pressed='true'] { background: var(--bg-inset); }
</style>

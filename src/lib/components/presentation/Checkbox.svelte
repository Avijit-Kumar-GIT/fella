<script lang="ts">
	import type { BaseComponentProps } from '@json-render/svelte';
	import { usePresentation } from './context';
	let { props }: BaseComponentProps<{ label: string; stateKey: string; defaultChecked?: boolean }> = $props();
	const context = usePresentation();
	let checked = $derived(Boolean(context.state[`control:${props.stateKey}`] ?? props.defaultChecked ?? false));
	function change(value: boolean) { context.update(`control:${props.stateKey}`, value); }
</script>
<label class="presentation-checkbox"><input type="checkbox" checked={checked} onchange={(event) => change(event.currentTarget.checked)} /><span>{props.label}</span></label>
<style>
	label { display: inline-flex; min-width: 0; align-items: flex-start; gap: var(--space-2); width: fit-content; max-width: 100%; color: var(--text-dim); font-size: var(--fs-sm); cursor: pointer; }
	input { width: 16px; height: 16px; flex: none; margin: 2px 0 0; accent-color: var(--brand); }
	span { overflow-wrap: anywhere; }
</style>

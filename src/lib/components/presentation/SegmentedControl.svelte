<script lang="ts">
	import type { BaseComponentProps } from '@json-render/svelte';
	import { usePresentation } from './context';
	type Option = { value: string; label: string };
	let { props }: BaseComponentProps<{ name: string; options: Option[]; stateKey: string; defaultValue?: string }> = $props();
	const context = usePresentation();
	let selected = $derived(String(context.state[`control:${props.stateKey}`] ?? props.defaultValue ?? props.options[0]?.value ?? ''));
	function choose(value: string) { context.update(`control:${props.stateKey}`, value); }
</script>
<fieldset><legend>{props.name}</legend><div class="segments presentation-segments" role="group" aria-label={props.name}>{#each props.options as option (option.value)}<button type="button" aria-pressed={selected === option.value} onclick={() => choose(option.value)}>{option.label}</button>{/each}</div></fieldset>
<style>
	fieldset { min-width: 0; max-width: 100%; padding: 0; margin: 0; border: 0; }
	legend { max-width: 100%; margin-bottom: var(--space-1); color: var(--text-dim); font-size: var(--fs-xs); overflow-wrap: anywhere; }
	.segments { width: fit-content; }
	button { min-width: 0; }
</style>

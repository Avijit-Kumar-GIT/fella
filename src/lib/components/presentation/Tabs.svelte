<script lang="ts">
	import type { ComponentRenderProps } from '@openuidev/svelte-lang';
	import { usePresentation } from './context';
	let { props, renderNode }: ComponentRenderProps<{ name: string; labels: string[]; children: unknown[] }> = $props();
	const context = usePresentation();
	const id = $props.id();
	let selected = $derived(Math.min(Number(context.state[`tab:${props.name}`] ?? 0), props.labels.length - 1));
	function select(index: number) { context.update(`tab:${props.name}`, index); }
	function keydown(event: KeyboardEvent, index: number) {
		let next = index;
		if (event.key === 'ArrowRight') next = (index + 1) % props.labels.length;
		else if (event.key === 'ArrowLeft') next = (index - 1 + props.labels.length) % props.labels.length;
		else if (event.key === 'Home') next = 0;
		else if (event.key === 'End') next = props.labels.length - 1;
		else return;
		event.preventDefault(); select(next); document.getElementById(`${id}-tab-${next}`)?.focus();
	}
</script>
<div class="tabs">
	<div class="tab-list presentation-tab-list" role="tablist" aria-label={props.name}>
		{#each props.labels as label, index}<button role="tab" id={`${id}-tab-${index}`} aria-selected={selected === index} aria-controls={`${id}-panel-${index}`} tabindex={selected === index ? 0 : -1} onclick={() => select(index)} onkeydown={(event) => keydown(event, index)}>{label}</button>{/each}
	</div>
	{#each props.children as child, index}{#if selected === index}<div class="presentation-tab-panel" role="tabpanel" id={`${id}-panel-${index}`} aria-labelledby={`${id}-tab-${index}`} tabindex="0">{@render renderNode(child)}</div>{/if}{/each}
</div>
<style>
	.tabs { min-width: 0; }
	.tab-list { margin-bottom: var(--space-3); }
	button { flex-shrink: 1; }
	[role='tabpanel'] { outline-offset: 3px; }
</style>

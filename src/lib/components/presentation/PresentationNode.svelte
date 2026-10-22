<script lang="ts">
	import type { Component } from 'svelte';
	import type { ElementNode } from '@openuidev/lang-core';
	import { presentationRenderers } from '../../presentation/library';
	import PresentationNode from './PresentationNode.svelte';
	let { node }: { node: ElementNode } = $props();
	let Current = $derived(presentationRenderers[node.typeName as keyof typeof presentationRenderers] as Component<any>);
</script>

{#snippet renderChildren(value: unknown)}
	{#if Array.isArray(value)}
		{#each value as child}
			{@render renderChildren(child)}
		{/each}
	{:else if value && typeof value === 'object' && (value as ElementNode).type === 'element'}
		<PresentationNode node={value as ElementNode} />
	{:else if typeof value === 'string' || typeof value === 'number' || typeof value === 'boolean'}
		{String(value)}
	{/if}
{/snippet}

<Current props={node.props} renderNode={renderChildren} />

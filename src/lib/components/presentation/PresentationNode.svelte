<script lang="ts">
	import type { Component } from 'svelte';
	import type { ElementNode } from '@openuidev/lang-core';
	import { presentationRenderers } from '../../presentation/library';
	import PresentationNode from './PresentationNode.svelte';
	let { node }: { node: ElementNode } = $props();
	let Current = $derived(presentationRenderers[node.typeName as keyof typeof presentationRenderers] as Component<any>);
</script>

{#snippet renderChildren(children: unknown[])}
	{#each children as child, index}
		{#if child && typeof child === 'object' && (child as ElementNode).type === 'element'}
			<PresentationNode node={child as ElementNode} />
		{/if}
	{/each}
{/snippet}

<Current props={node.props} renderNode={renderChildren} />

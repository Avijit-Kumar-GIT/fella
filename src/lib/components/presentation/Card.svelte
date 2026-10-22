<script lang="ts">
	import type { ComponentRenderProps } from '@openuidev/svelte-lang';
	let { props, renderNode }: ComponentRenderProps<{ children: unknown[]; title?: string; detail?: string; tone?: 'plain' | 'quiet' | 'accent' }> = $props();
</script>
<section class="card" data-tone={props.tone ?? 'plain'}>
	{#if props.title || props.detail}<header>{#if props.title}<h3>{props.title}</h3>{/if}{#if props.detail}<p>{props.detail}</p>{/if}</header>{/if}
	<div class="card-content">{@render renderNode(props.children)}</div>
</section>
<style>
	.card { min-width: 0; padding: 1rem; border-radius: var(--radius-lg, 14px); background: var(--bg-raised); }
	.card[data-tone='quiet'] { background: var(--bg-inset); }
	.card[data-tone='accent'] { background: color-mix(in srgb, var(--accent) 9%, var(--bg-raised)); }
	header { margin-bottom: .85rem; } h3 { margin: 0; font-size: .9rem; font-weight: 600; } header p { margin: .2rem 0 0; color: var(--text-dim); font-size: .78rem; line-height: 1.5; }
	.card-content { display: grid; gap: .9rem; min-width: 0; }
</style>

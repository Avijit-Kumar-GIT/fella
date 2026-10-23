<script lang="ts">
	import type { BaseComponentProps } from '@json-render/svelte';
	let { props, children }: BaseComponentProps<{ title?: string; detail?: string; tone?: 'plain' | 'quiet' | 'accent' }> = $props();
</script>
<section class="card presentation-card" data-tone={props.tone ?? 'plain'}>
	{#if props.title || props.detail}<header>{#if props.title}<h3>{props.title}</h3>{/if}{#if props.detail}<p>{props.detail}</p>{/if}</header>{/if}
	<div class="card-content presentation-card-content">{@render children?.()}</div>
</section>
<style>
	.card { min-width: 0; }
	.card[data-tone='quiet'] { background: var(--bg-inset); }
	.card[data-tone='accent'] { background: color-mix(in srgb, var(--brand) 7%, var(--bg-raised)); }
	header { margin-bottom: var(--space-3); }
	h3 { margin: 0; font-size: var(--fs-lg); font-weight: 620; }
	header p { margin: var(--space-1) 0 0; color: var(--text-dim); font-size: var(--fs-sm); line-height: 1.5; }
	.card-content { display: grid; gap: var(--space-3); }
</style>

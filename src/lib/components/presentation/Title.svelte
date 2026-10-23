<script lang="ts">
	import type { BaseComponentProps } from '@json-render/svelte';
	let { props }: BaseComponentProps<{ content: string; level?: number }> = $props();
</script>
{#if (props.level ?? 1) === 1}<h2 class="presentation-title">{props.content}</h2>{:else if props.level === 2}<h3 class="presentation-title">{props.content}</h3>{:else if props.level === 3}<h4 class="presentation-title">{props.content}</h4>{:else}<h5 class="presentation-title">{props.content}</h5>{/if}
<style>
	h2,h3,h4,h5 { margin: 0; color: var(--text); line-height: 1.28; letter-spacing: -.02em; overflow-wrap: anywhere; }
	h2 { font-size: clamp(1.35rem, 3.4cqi, 1.8rem); font-weight: 650; }
	h3 { font-size: var(--fs-xl); font-weight: 620; }
	h4 { font-size: var(--fs-lg); font-weight: 600; }
	h5 { font-size: var(--fs); font-weight: 600; }
</style>

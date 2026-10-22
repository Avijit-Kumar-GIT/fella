<script lang="ts">
	import type { ComponentRenderProps } from '@openuidev/svelte-lang';
	let { props }: ComponentRenderProps<{ content: string; tone?: 'neutral' | 'positive' | 'caution' | 'negative' | 'accent' }> = $props();
</script>
<span class="presentation-badge" data-tone={props.tone ?? 'neutral'}>{props.content}</span>
<style>
	span { display: inline-flex; align-items: center; max-width: 100%; width: fit-content; padding: 3px 9px; border-radius: 999px; color: var(--text-dim); background: var(--bg-inset); font-size: var(--fs-xs); line-height: 1.4; overflow-wrap: anywhere; }
	span[data-tone='positive'] { color: var(--ok); background: color-mix(in srgb, var(--ok) 12%, transparent); }
	span[data-tone='caution'] { color: var(--warn); background: color-mix(in srgb, var(--warn) 12%, transparent); }
	span[data-tone='negative'] { color: var(--err); background: color-mix(in srgb, var(--err) 12%, transparent); }
	span[data-tone='accent'] { color: var(--accent); background: color-mix(in srgb, var(--accent) 12%, transparent); }
</style>

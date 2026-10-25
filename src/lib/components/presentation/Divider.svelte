<script lang="ts">
	import type { BaseComponentProps } from '@json-render/svelte';
	let { props }: BaseComponentProps<{ label?: string }> = $props();
</script>
<div class="divider" aria-hidden="true">{#if props.label}<span>{props.label}</span>{/if}</div>
<style>
	.divider { height: 1px; margin: var(--space-2) 0; background: var(--border); }
	.divider span { position: relative; top: -.72rem; display: inline-block; max-width: 100%; padding-right: var(--space-2); background: var(--bg-raised); color: var(--text-faint); font-size: var(--fs-xs); overflow-wrap: anywhere; }
</style>

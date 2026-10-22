<script lang="ts">
	import type { ComponentRenderProps } from '@openuidev/svelte-lang';
	let { props }: ComponentRenderProps<{ label?: string }> = $props();
</script>
<div class="divider" aria-hidden="true">{#if props.label}<span>{props.label}</span>{/if}</div>
<style>
	.divider { height: 1px; margin: .25rem 0; background: var(--border); }
	.divider span { position: relative; top: -.68rem; display: inline-block; padding-right: .5rem; background: var(--bg); color: var(--text-faint); font-size: .7rem; }
</style>

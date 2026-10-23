<script lang="ts">
	import type { BaseComponentProps } from '@json-render/svelte';
	import { usePresentation } from './context';
	let { props }: BaseComponentProps<{ label: string; action?: { type: 'follow_up'; message: string } | { type: 'set_state'; stateKey: string; value: string | number | boolean | null } }> = $props();
	const context = usePresentation();
</script>
<button class="presentation-button" type="button" onclick={() => context.runAction(props.action)}>{props.label}<span aria-hidden="true">↗</span></button>
<style>
	button { font: inherit; cursor: pointer; }
	span { color: var(--text-faint); }
</style>

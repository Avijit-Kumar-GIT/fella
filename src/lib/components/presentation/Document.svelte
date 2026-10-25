<script lang="ts">
	import type { BaseComponentProps } from '@json-render/svelte';
	import './presentation-theme.css';
	let { children }: BaseComponentProps = $props();
</script>
<div class="analytical-document">{@render children?.()}</div>

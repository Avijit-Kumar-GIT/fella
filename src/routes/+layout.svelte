<script lang="ts">
	import '../app.css';
	import { Tooltip } from '$lib/components/ui/tooltip';

	let { children } = $props();
</script>

<Tooltip.Provider delayDuration={350} skipDelayDuration={250}>
	{@render children()}
</Tooltip.Provider>

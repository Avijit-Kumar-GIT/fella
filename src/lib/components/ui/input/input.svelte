<script lang="ts">
	import { cn } from '$lib/utils';
	import type { HTMLInputAttributes } from 'svelte/elements';

	type Props = Omit<HTMLInputAttributes, 'class' | 'value' | 'ref'> & {
		class?: string;
		value?: string | string[] | number | null;
		ref?: HTMLInputElement | null;
	};

	let {
		class: className,
		type = 'text',
		value = $bindable(),
		ref = $bindable(),
		...restProps
	}: Props = $props();
</script>

<input data-slot="input" class={cn('fella-ui-input', className)} {type} bind:value bind:this={ref} {...restProps} />

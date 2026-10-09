<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { HTMLSelectAttributes } from 'svelte/elements';
	import { cn } from '$lib/utils';

	type Props = Omit<HTMLSelectAttributes, 'class' | 'value' | 'ref'> & {
		class?: string;
		value?: string | string[] | number | null;
		children?: Snippet;
		ref?: HTMLSelectElement | null;
	};

	let { class: className, value = $bindable(), children, ref = $bindable(), ...restProps }: Props = $props();
</script>

<select data-slot="select" class={cn('fella-ui-select', className)} bind:value bind:this={ref} {...restProps}>
	{@render children?.()}
</select>

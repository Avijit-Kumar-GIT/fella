<script lang="ts">
	import { cn } from '$lib/utils';
	import type { HTMLTextareaAttributes } from 'svelte/elements';

	type Props = Omit<HTMLTextareaAttributes, 'class' | 'value' | 'ref'> & {
		class?: string;
		value?: string | string[] | number | null;
		ref?: HTMLTextAreaElement | null;
	};

	let { class: className, value = $bindable(), ref = $bindable(), ...restProps }: Props = $props();
</script>

<textarea data-slot="textarea" class={cn('fella-ui-textarea', className)} bind:value bind:this={ref} {...restProps}></textarea>

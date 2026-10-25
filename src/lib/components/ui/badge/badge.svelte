<script lang="ts">
	import { cn } from '$lib/utils';
	import type { Snippet } from 'svelte';
	import type { HTMLAttributes } from 'svelte/elements';

	type Variant = 'default' | 'success' | 'warning' | 'destructive';
	type Props = Omit<HTMLAttributes<HTMLSpanElement>, 'class' | 'children'> & {
		class?: string;
		variant?: Variant;
		children?: Snippet;
	};

	let { class: className, variant = 'default', children, ...restProps }: Props = $props();
</script>

<span data-slot="badge" class={cn('fella-ui-badge', `fella-ui-badge-${variant}`, className)} {...restProps}>
	{@render children?.()}
</span>

<style>
	.fella-ui-badge {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		max-width: 100%;
		padding: 2px 6px;
		border: 1px solid var(--border);
		border-radius: var(--radius-chip);
		background: var(--bg-inset);
		color: var(--text-dim);
		font-size: var(--fs-xs);
		font-weight: 600;
		line-height: 1.35;
		white-space: nowrap;
	}
	.fella-ui-badge-success {
		border-color: color-mix(in srgb, var(--ok) 30%, var(--border));
		background: color-mix(in srgb, var(--ok) 9%, var(--bg-inset));
		color: var(--ok);
	}
	.fella-ui-badge-warning {
		border-color: color-mix(in srgb, var(--warn) 34%, var(--border));
		background: color-mix(in srgb, var(--warn) 9%, var(--bg-inset));
		color: var(--warn);
	}
	.fella-ui-badge-destructive {
		border-color: color-mix(in srgb, var(--err) 34%, var(--border));
		background: color-mix(in srgb, var(--err) 9%, var(--bg-inset));
		color: var(--err);
	}
</style>

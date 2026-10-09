<script lang="ts">
	import { cn } from '$lib/utils';
	import type { Snippet } from 'svelte';
	import type { HTMLAttributes } from 'svelte/elements';

	type Variant = 'default' | 'info' | 'warning' | 'destructive';
	type Props = Omit<HTMLAttributes<HTMLDivElement>, 'class' | 'children' | 'role'> & {
		class?: string;
		variant?: Variant;
		role?: string;
		children?: Snippet;
	};

	let { class: className, variant = 'default', role, children, ...restProps }: Props = $props();
	const defaultRole = $derived(role ?? (variant === 'destructive' ? 'alert' : 'status'));
</script>

<div
	data-slot="alert"
	class={cn('fella-ui-alert', `fella-ui-alert-${variant}`, className)}
	role={defaultRole}
	{...restProps}
>
	{@render children?.()}
</div>

<style>
	.fella-ui-alert {
		display: flex;
		align-items: flex-start;
		gap: var(--space-2);
		min-width: 0;
		padding: var(--space-2) var(--space-3);
		border: 1px solid var(--border);
		border-left: 2px solid var(--border-strong);
		border-radius: var(--radius-sm);
		background: var(--bg-inset);
		color: var(--text-dim);
		font-size: var(--fs-sm);
		line-height: 1.45;
	}
	.fella-ui-alert-info { border-left-color: var(--brand); }
	.fella-ui-alert-warning { border-left-color: var(--warn); }
	.fella-ui-alert-destructive { border-left-color: var(--err); }
</style>

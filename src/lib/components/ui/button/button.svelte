<script lang="ts" module>
	import { tv, type VariantProps } from 'tailwind-variants';
	import type { Snippet } from 'svelte';
	import type { HTMLButtonAttributes } from 'svelte/elements';

	export const buttonVariants = tv({
		base: 'inline-flex shrink-0 items-center justify-center gap-2 whitespace-nowrap rounded-md font-sans text-sm font-medium transition-colors focus-visible:outline-none disabled:pointer-events-none disabled:cursor-not-allowed disabled:opacity-60 [&_svg]:pointer-events-none [&_svg]:size-4 [&_svg]:shrink-0',
		variants: {
			variant: {
				default:
					'bg-primary text-primary-foreground hover:brightness-95 disabled:bg-muted disabled:text-muted-foreground',
				secondary: 'bg-secondary text-secondary-foreground hover:bg-secondary/80',
				outline: 'border border-input bg-background hover:bg-accent hover:text-accent-foreground',
				ghost: 'text-foreground hover:bg-accent hover:text-accent-foreground',
				destructive: 'text-destructive hover:bg-destructive/10',
				link: 'h-auto text-[var(--link)] underline-offset-4 hover:underline'
			},
			size: {
				default: 'h-9 px-3.5 py-2',
				sm: 'h-8 rounded-sm px-3 text-xs',
				lg: 'h-10 rounded-md px-6',
				icon: 'size-9 p-0'
			}
		},
		defaultVariants: {
			variant: 'default',
			size: 'default'
		}
	});

	export type ButtonVariant = VariantProps<typeof buttonVariants>['variant'];
	export type ButtonSize = VariantProps<typeof buttonVariants>['size'];
	export type ButtonProps = Omit<HTMLButtonAttributes, 'class' | 'children'> & {
		variant?: ButtonVariant;
		size?: ButtonSize;
		class?: string;
		children?: Snippet;
	};
</script>

<script lang="ts">
	import { cn } from '$lib/utils';

	let {
		class: className,
		variant = 'default',
		size = 'default',
		type = 'button',
		children,
		...restProps
	}: import('./button.svelte').ButtonProps = $props();
</script>

<button
	data-slot="button"
	class={cn(buttonVariants({ variant, size }), className)}
	{type}
	{...restProps}
>
	{@render children?.()}
</button>

<script lang="ts">
	import { cn } from '$lib/utils';

	let { size = 22, label, class: className }: { size?: number; label?: string; class?: string } = $props();
</script>

<span
	data-slot="spinner"
	class={cn('fella-ui-spinner', className)}
	style={`--fella-spinner-size:${size}px`}
	role={label ? 'status' : undefined}
	aria-label={label}
	aria-hidden={label ? undefined : true}
>
	<span></span>
	<span></span>
	<span></span>
</span>

<style>
	.fella-ui-spinner {
		position: relative;
		display: inline-block;
		width: var(--fella-spinner-size);
		height: var(--fella-spinner-size);
		flex: none;
		color: var(--brand);
	}
	.fella-ui-spinner::before {
		position: absolute;
		inset: 2px;
		border: 1px solid color-mix(in srgb, var(--brand) 42%, transparent);
		border-radius: 50%;
		content: '';
		animation: fella-spinner-breathe 2.4s ease-in-out infinite;
	}
	.fella-ui-spinner > span {
		position: absolute;
		top: calc(50% - 2.5px);
		left: calc(50% - 2.5px);
		width: 5px;
		height: 5px;
		border-radius: 50%;
		background: currentColor;
		transform-origin: 2.5px 2.5px;
		animation: fella-spinner-orbit 1.8s linear infinite;
	}
	.fella-ui-spinner > span:nth-child(2) {
		color: var(--chart-cyan);
		animation-delay: -0.6s;
	}
	.fella-ui-spinner > span:nth-child(3) {
		color: var(--chart-violet);
		animation-delay: -1.2s;
	}
	@keyframes fella-spinner-orbit {
		from { transform: rotate(0deg) translateX(calc(var(--fella-spinner-size) * 0.36)); }
		to { transform: rotate(360deg) translateX(calc(var(--fella-spinner-size) * 0.36)); }
	}
	@keyframes fella-spinner-breathe {
		0%, 100% { opacity: 0.35; transform: scale(0.92); }
		50% { opacity: 0.85; transform: scale(1); }
	}
	@media (prefers-reduced-motion: reduce) {
		.fella-ui-spinner::before,
		.fella-ui-spinner > span { animation: none; }
	}
</style>

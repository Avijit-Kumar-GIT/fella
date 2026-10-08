<script lang="ts">
	let { size = 22 }: { size?: number } = $props();
</script>

<span class="data-loader" style={`--loader-size:${size}px`} aria-hidden="true">
	<span></span>
	<span></span>
	<span></span>
</span>

<style>
	.data-loader {
		position: relative;
		display: inline-block;
		width: var(--loader-size);
		height: var(--loader-size);
		flex: none;
		color: var(--brand);
	}
	.data-loader::before {
		content: '';
		position: absolute;
		inset: 2px;
		border: 1px solid color-mix(in srgb, var(--brand) 42%, transparent);
		border-radius: 50%;
		animation: loader-breathe 2.4s ease-in-out infinite;
	}
	.data-loader > span {
		position: absolute;
		top: calc(50% - 2.5px);
		left: calc(50% - 2.5px);
		width: 5px;
		height: 5px;
		border-radius: 50%;
		background: currentColor;
		transform-origin: 2.5px 2.5px;
		animation: loader-orbit 1.8s linear infinite;
	}
	.data-loader > span:nth-child(2) {
		color: var(--chart-cyan);
		animation-delay: -0.6s;
	}
	.data-loader > span:nth-child(3) {
		color: var(--chart-violet);
		animation-delay: -1.2s;
	}
	@keyframes loader-orbit {
		from {
			transform: rotate(0deg) translateX(calc(var(--loader-size) * 0.36));
		}
		to {
			transform: rotate(360deg) translateX(calc(var(--loader-size) * 0.36));
		}
	}
	@keyframes loader-breathe {
		0%,
		100% {
			opacity: 0.35;
			transform: scale(0.92);
		}
		50% {
			opacity: 0.85;
			transform: scale(1);
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.data-loader::before,
		.data-loader > span {
			animation: none;
		}
	}
</style>

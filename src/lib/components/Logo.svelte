<script lang="ts">
	// Keep the app mark aligned with the teal orb used on the Fella website.
	let { size = 18, active = false }: { size?: number; active?: boolean } = $props();
</script>

<svg class="logo" class:active viewBox="0 0 32 32" width={size} height={size} aria-hidden="true">
	<defs>
		<radialGradient id="fella-orb-base" cx="35%" cy="28%" r="100%">
			<stop offset="0" stop-color="#a5e5df" />
			<stop offset="0.37" stop-color="#42b8ad" />
			<stop offset="0.68" stop-color="#008080" />
			<stop offset="1" stop-color="#1c6179" />
		</radialGradient>
	</defs>

	<g class="orb-core">
		<circle cx="16" cy="16" r="15.25" fill="url(#fella-orb-base)" stroke="#004a52" stroke-opacity="0.16" stroke-width="1" />
	</g>
</svg>

<style>
	.logo {
		display: block;
		flex: none;
		overflow: visible;
		filter: drop-shadow(0 1px 2px rgba(0, 89, 99, 0.22));
	}
	.orb-core {
		transform-box: fill-box;
		transform-origin: center;
	}
	.logo.active .orb-core {
		animation: orb-breathe 2.6s ease-in-out infinite;
	}
	@keyframes orb-breathe {
		0%,
		100% {
			transform: scale(0.985);
		}
		50% {
			transform: scale(1.035);
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.logo.active .orb-core {
			animation: none;
		}
	}
</style>

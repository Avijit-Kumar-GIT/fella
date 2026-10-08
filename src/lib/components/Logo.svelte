<script lang="ts">
	// A small workspace orb: teal remains the brand anchor, while the violet
	// highlight and cyan bloom give it the dimensional, active quality of a
	// light source rather than another flat interface glyph.
	let { size = 18, active = false }: { size?: number; active?: boolean } = $props();
</script>

<svg class="logo" class:active viewBox="0 0 32 32" width={size} height={size} aria-hidden="true">
	<defs>
		<radialGradient id="fella-orb-base" cx="24%" cy="18%" r="88%">
			<stop offset="0" stop-color="#efedff" />
			<stop offset="0.2" stop-color="#b9a8ed" />
			<stop offset="0.47" stop-color="#5977bd" />
			<stop offset="0.73" stop-color="#0eb7bf" />
			<stop offset="1" stop-color="#006671" />
		</radialGradient>
		<radialGradient id="fella-orb-light" cx="82%" cy="51%" r="67%">
			<stop offset="0" stop-color="#73f5ef" stop-opacity="0.96" />
			<stop offset="0.35" stop-color="var(--brand)" stop-opacity="0.58" />
			<stop offset="1" stop-color="var(--brand)" stop-opacity="0" />
		</radialGradient>
		<radialGradient id="fella-orb-sheen" cx="18%" cy="12%" r="60%">
			<stop offset="0" stop-color="#ffffff" stop-opacity="0.82" />
			<stop offset="0.3" stop-color="#e4dcff" stop-opacity="0.34" />
			<stop offset="1" stop-color="#c9bcf0" stop-opacity="0" />
		</radialGradient>
		<linearGradient id="fella-orb-depth" x1="0" y1="1" x2="1" y2="0">
			<stop offset="0" stop-color="#32145f" stop-opacity="0.58" />
			<stop offset="0.42" stop-color="#174e83" stop-opacity="0.13" />
			<stop offset="1" stop-color="#0c223e" stop-opacity="0" />
		</linearGradient>
		<radialGradient id="fella-orb-rim" cx="30%" cy="18%" r="82%">
			<stop offset="0" stop-color="#ffffff" stop-opacity="0.44" />
			<stop offset="0.48" stop-color="#73e9e7" stop-opacity="0.2" />
			<stop offset="1" stop-color="#001e2b" stop-opacity="0.44" />
		</radialGradient>
	</defs>

	<circle class="halo" cx="16" cy="16" r="15.15" fill="none" stroke="#3ce0dc" stroke-width="1" opacity="0.18" />
	<g class="orb-core">
		<circle cx="16" cy="16" r="14.35" fill="url(#fella-orb-base)" />
		<circle cx="16" cy="16" r="14.35" fill="url(#fella-orb-light)" />
		<circle cx="16" cy="16" r="14.35" fill="url(#fella-orb-depth)" />
		<circle cx="16" cy="16" r="14.35" fill="url(#fella-orb-sheen)" />
		<circle cx="16" cy="16" r="14.35" fill="none" stroke="url(#fella-orb-rim)" stroke-width="0.85" />
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
	.logo.active .halo {
		animation: orb-halo 2.6s ease-in-out infinite;
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
	@keyframes orb-halo {
		0%,
		100% {
			opacity: 0.16;
		}
		50% {
			opacity: 0.52;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.logo.active .orb-core,
		.logo.active .halo {
			animation: none;
		}
	}
</style>

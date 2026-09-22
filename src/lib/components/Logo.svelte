<script lang="ts">
	// The fella mark: a blob in --brand teal with two eyes punched out to --bg,
	// so it reads correctly against either theme without a second color.
	let { size = 18 }: { size?: number } = $props();
</script>

<svg class="logo" viewBox="124 168 264 226" width={size} height={size} aria-hidden="true">
	<path
		d="M256 178 C 330 178 378 228 378 292 C 378 340 342 384 284 384 L 228 384 C 170 384 134 340 134 292 C 134 228 182 178 256 178 Z"
		fill="var(--brand)"
	/>
	<ellipse cx="211" cy="278" rx="20" ry="25" fill="var(--bg)" />
	<ellipse cx="301" cy="278" rx="20" ry="25" fill="var(--bg)" />
</svg>

<style>
	.logo {
		display: block;
		flex: none;
	}
</style>

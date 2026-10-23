<script lang="ts">
	let { size = 20, fontSize = size / 2, active = false }: { size?: number; fontSize?: number; active?: boolean } = $props();
</script>

<span class="fella-mark" class:active style={`--mark-size:${size}px;--mark-font:${fontSize}px`} role="img" aria-label="Fella">F</span>

<style>
	.fella-mark { display:inline-flex; width:var(--mark-size); height:var(--mark-size); flex:none; align-items:center; justify-content:center; border-radius:50%; background:var(--brand); color:var(--on-brand); font:600 var(--mark-font)/1 var(--sans); letter-spacing:0; }
	.active { animation:breathe 1.6s ease-in-out infinite; }
	@keyframes breathe { 50% { opacity:.6; } }
	@media (prefers-reduced-motion:reduce) { .active { animation:none; } }
</style>

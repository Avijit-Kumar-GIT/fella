<script lang="ts">
	import { isDesktop, win } from '$lib/ipc';
	const native = typeof navigator !== 'undefined' && /Mac/i.test(navigator.platform || navigator.userAgent) && isDesktop();
</script>

<div class="window-controls" class:native aria-label={native ? undefined : 'Window controls'}>
	{#if !native}
		<button class="close" aria-label="Close window" title="Close window" disabled={!isDesktop()} onclick={() => void win.close()}><span>×</span></button>
		<button class="minimize" aria-label="Minimize window" title="Minimize window" disabled={!isDesktop()} onclick={() => void win.minimize()}><span>−</span></button>
		<button class="maximize" aria-label="Maximize or restore window" title="Maximize or restore window" disabled={!isDesktop()} onclick={() => void win.toggleMaximize()}><span>+</span></button>
	{/if}
</div>

<style>
	.window-controls { display:flex; width:52px; height:32px; flex:none; align-items:center; gap:8px; -webkit-app-region:no-drag; }
	.window-controls button { display:grid; width:12px; height:12px; min-width:12px; place-items:center; padding:0; border:0; border-radius:50%; color:rgb(0 0 0 / .65); cursor:default; }
	.close { background:#ff5f57; } .minimize { background:#febc2e; } .maximize { background:#28c840; }
	.window-controls span { opacity:0; font:600 11px/1 var(--sans); }
	.window-controls:hover span { opacity:1; }
	.window-controls button:focus-visible { outline:2px solid var(--text); outline-offset:3px; }
	.native { -webkit-app-region:drag; }
</style>

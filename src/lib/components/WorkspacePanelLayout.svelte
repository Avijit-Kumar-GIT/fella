<script lang="ts">
	import { onMount, type Snippet } from 'svelte';

	let { open, expanded = $bindable(false), compact = $bindable(false), conversation, tools }: {
		open: boolean; expanded?: boolean; compact?: boolean; conversation: Snippet; tools: Snippet;
	} = $props();
	let host = $state<HTMLDivElement>();
	let width = $state(0);
	let toolWidth = $state(480);
	let dragging = $state(false);
	const storageKey = 'fella:tools-width';
	let solo = $derived(open && (compact || expanded));
	let maximum = $derived(Math.max(400, width - 380));
	let actualWidth = $derived(Math.min(maximum, Math.max(400, toolWidth)));

	onMount(() => {
		try { const saved = Number(localStorage.getItem(storageKey)); if (Number.isFinite(saved) && saved >= 400) toolWidth = Math.min(saved, 1200); } catch { /* Optional local preference. */ }
		const observer = new ResizeObserver(([entry]) => {
			width = entry.contentRect.width;
			compact = width < 900;
		});
		if (host) observer.observe(host);
		return () => observer.disconnect();
	});

	function rememberWidth(): void {
		try { localStorage.setItem(storageKey, String(Math.round(toolWidth))); } catch { /* Optional local preference. */ }
	}
	function move(event: PointerEvent): void {
		if (!dragging || !host) return;
		toolWidth = Math.max(400, Math.min(maximum, host.getBoundingClientRect().right - event.clientX));
	}
	function finish(): void { if (dragging) { dragging = false; rememberWidth(); } }
	function resizeKey(event: KeyboardEvent): void {
		if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
		event.preventDefault();
		toolWidth = event.key === 'Home' ? 400 : event.key === 'End' ? maximum : Math.max(400, Math.min(maximum, actualWidth + (event.key === 'ArrowLeft' ? 32 : -32)));
		rememberWidth();
	}
</script>

<svelte:window onpointermove={move} onpointerup={finish} onpointercancel={finish} onblur={finish} />

<div bind:this={host} class="workspace-chat-layout" class:dragging data-workspace-pane-open={open} data-panel-mode={solo ? 'expanded' : open ? 'split' : 'conversation'} style={`--tools-width:${actualWidth}px`}>
	<section class="workspace-conversation-pane" aria-label="Conversation" hidden={solo}>
		{@render conversation()}
	</section>
	{#if open && !solo}
		<!-- A focusable separator is the WAI-ARIA window-splitter pattern. -->
		<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
		<div class="workspace-divider" role="separator" aria-label="Resize workspace tools" aria-orientation="vertical" aria-valuemin={400} aria-valuemax={Math.round(maximum)} aria-valuenow={Math.round(actualWidth)} tabindex="0" onkeydown={resizeKey} onpointerdown={(event) => { event.preventDefault(); dragging = true; }} ondblclick={() => { toolWidth = 480; rememberWidth(); }}></div>
	{/if}
	<aside class="workspace-tools-pane" id="workspace-tools-pane" aria-label="Workspace tools" hidden={!open}>
		{@render tools()}
	</aside>
</div>

<style>
	.workspace-chat-layout { position: relative; display: grid; flex: 1; min-width: 0; min-height: 0; grid-template-columns: minmax(0, 1fr); grid-template-rows: minmax(0, 1fr); overflow: hidden; background: var(--workspace-canvas); }
	.workspace-chat-layout[data-panel-mode='split'] { grid-template-columns: minmax(0, 1fr) 1px var(--tools-width); }
	.workspace-conversation-pane, .workspace-tools-pane { display: flex; min-width: 0; min-height: 0; overflow: hidden; }
	.workspace-conversation-pane[hidden], .workspace-tools-pane[hidden] { display: none; }
	.workspace-tools-pane { container-name: investigation-pane; container-type: inline-size; background: var(--workspace-canvas); }
	.workspace-divider { position: relative; z-index: 6; background: var(--border); cursor: col-resize; touch-action: none; }
	.workspace-divider::after { content: ''; position: absolute; inset: 0 -4px; }
	.workspace-divider:hover, .workspace-divider:focus-visible, .dragging .workspace-divider { background: var(--link); outline: none; }
	.dragging { cursor: col-resize; user-select: none; }
	.workspace-tools-pane :global(.workspace), .workspace-tools-pane :global(.companion) { flex: 1; width: 100%; min-width: 0; min-height: 0; border-left: 0; }
</style>

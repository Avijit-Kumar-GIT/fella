<script lang="ts">
	import { onMount, type Snippet } from 'svelte';

	let { open, notebook = false, primaryLabel = 'Chat', secondaryLabel = 'Tools', defaultToolsWidth = 380, layoutKey = 'default', expanded = $bindable(false), compact = $bindable(false), conversation, tools }: {
		open: boolean; notebook?: boolean; primaryLabel?: string; secondaryLabel?: string; defaultToolsWidth?: number; layoutKey?: string; expanded?: boolean; compact?: boolean; conversation: Snippet; tools: Snippet;
	} = $props();
	let host = $state<HTMLDivElement>();
	let width = $state(0);
	let toolWidth = $state(480);
	let notebookToolsWidth = $state<number | null>(null);
	let mobilePane = $state<'tools' | 'conversation'>('conversation');
	let dragging = $state(false);
	const storageKey = 'fella:tools-width';
	const notebookStorageKey = 'fella:notebook-tools-width-v2';
	let narrow = $derived(width > 0 && width < 760);
	let solo = $derived(open && (expanded || (!notebook && compact)));
	let mobileTabs = $derived(open && notebook && narrow && !expanded);
	let notebookMaximum = $derived(Math.max(320, width - 400));
	let notebookDefaultWidth = $derived(Math.min(defaultToolsWidth, Math.max(320, Math.round(width * 0.36))));
	let notebookActualWidth = $derived(Math.min(notebookMaximum, Math.max(320, notebookToolsWidth ?? notebookDefaultWidth)));
	let maximum = $derived(Math.max(400, width - 380));
	let actualWidth = $derived(Math.min(maximum, Math.max(400, toolWidth)));
	let activeWidth = $derived(notebook ? notebookActualWidth : actualWidth);
	let activeMaximum = $derived(notebook ? notebookMaximum : maximum);
	let conversationVisible = $derived(!solo && (!mobileTabs || mobilePane === 'conversation'));
	let toolsVisible = $derived(open && (!mobileTabs || mobilePane === 'tools'));
	let mode = $derived(mobileTabs ? 'mobile' : solo ? 'expanded' : open ? 'split' : 'conversation');

	onMount(() => {
		try {
			const saved = Number(localStorage.getItem(storageKey));
			if (Number.isFinite(saved) && saved >= 400) toolWidth = Math.min(saved, 1200);
			const savedNotebook = Number(localStorage.getItem(`${notebookStorageKey}:${layoutKey}`));
			if (Number.isFinite(savedNotebook) && savedNotebook >= 320) notebookToolsWidth = Math.min(savedNotebook, 1200);
		} catch { /* Optional local preference. */ }
		const observer = new ResizeObserver(([entry]) => {
			width = entry.contentRect.width;
			compact = width < 900;
		});
		if (host) observer.observe(host);
		return () => observer.disconnect();
	});
	$effect(() => {
		const key = layoutKey;
		if (!notebook) return;
		try {
			const saved = Number(localStorage.getItem(`${notebookStorageKey}:${key}`));
			notebookToolsWidth = Number.isFinite(saved) && saved >= 320 ? Math.min(saved, 1200) : null;
		} catch { notebookToolsWidth = null; }
	});

	function rememberWidth(): void {
		try {
			localStorage.setItem(notebook ? `${notebookStorageKey}:${layoutKey}` : storageKey, String(Math.round(notebook ? notebookActualWidth : toolWidth)));
		} catch { /* Optional local preference. */ }
	}

	function setActiveWidth(value: number): void {
		const minimum = notebook ? 320 : 400;
		const upper = notebook ? notebookMaximum : maximum;
		const next = Math.max(minimum, Math.min(upper, value));
		if (notebook) notebookToolsWidth = next;
		else toolWidth = next;
	}

	function move(event: PointerEvent): void {
		if (!dragging || !host) return;
		const rect = host.getBoundingClientRect();
		setActiveWidth(rect.right - event.clientX);
	}

	function finish(): void { if (dragging) { dragging = false; rememberWidth(); } }

	function resizeKey(event: KeyboardEvent): void {
		if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
		event.preventDefault();
		const delta = event.key === 'ArrowLeft' ? 32 : -32;
		const minimum = notebook ? 320 : 400;
		setActiveWidth(event.key === 'Home' ? minimum : event.key === 'End' ? activeMaximum : activeWidth + delta);
		rememberWidth();
	}

	function selectMobilePane(pane: 'tools' | 'conversation', focus = false): void {
		mobilePane = pane;
		if (focus) document.getElementById(pane === 'tools' ? 'workspace-tools-tab' : 'workspace-conversation-tab')?.focus();
	}

	function tabKeydown(event: KeyboardEvent): void {
		if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') {
			event.preventDefault();
			selectMobilePane(mobilePane === 'conversation' ? 'tools' : 'conversation', true);
		} else if (event.key === 'Home') {
			event.preventDefault();
			selectMobilePane('conversation', true);
		} else if (event.key === 'End') {
			event.preventDefault();
			selectMobilePane('tools', true);
		}
	}
</script>

<svelte:window onpointermove={move} onpointerup={finish} onpointercancel={finish} onblur={finish} />

<div
	bind:this={host}
	class="workspace-chat-layout"
	class:dragging
	data-workspace-pane-open={open}
	data-notebook={notebook}
	data-panel-mode={mode}
	data-mobile-pane={mobilePane}
	style={`--tools-width:${actualWidth}px;--notebook-tools-width:${notebookActualWidth}px`}
>
	<div class="workspace-mobile-tabs fella-pill-nav" role="tablist" aria-label="Workspace panes" hidden={!mobileTabs}>
		<button
			id="workspace-conversation-tab"
			class="workspace-mobile-tab fella-pill-nav-item"
			class:active={mobilePane === 'conversation'}
			type="button"
			role="tab"
			aria-selected={mobilePane === 'conversation'}
			aria-controls="workspace-conversation-pane"
			tabindex={mobilePane === 'conversation' ? 0 : -1}
			onclick={() => selectMobilePane('conversation')}
			onkeydown={tabKeydown}
		>{primaryLabel}</button>
		<button
			id="workspace-tools-tab"
			class="workspace-mobile-tab fella-pill-nav-item"
			class:active={mobilePane === 'tools'}
			type="button"
			role="tab"
			aria-selected={mobilePane === 'tools'}
			aria-controls="workspace-tools-pane"
			tabindex={mobilePane === 'tools' ? 0 : -1}
			onclick={() => selectMobilePane('tools')}
			onkeydown={tabKeydown}
		>{secondaryLabel}</button>
	</div>

	<section
		class="workspace-conversation-pane"
		id="workspace-conversation-pane"
		aria-label={primaryLabel}
		role={mobileTabs ? 'tabpanel' : undefined}
		aria-labelledby={mobileTabs ? 'workspace-conversation-tab' : undefined}
		hidden={!conversationVisible}
	>
		{@render conversation()}
	</section>
	{#if open && !solo && !mobileTabs}
		<!-- A focusable separator is the WAI-ARIA window-splitter pattern. -->
		<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
		<div
			class="workspace-divider"
			role="separator"
			aria-label={notebook ? `Resize ${secondaryLabel.toLocaleLowerCase()} pane` : 'Resize workspace tools'}
			aria-orientation="vertical"
			aria-valuemin={notebook ? 320 : 400}
			aria-valuemax={Math.round(activeMaximum)}
			aria-valuenow={Math.round(activeWidth)}
			tabindex="0"
			onkeydown={resizeKey}
			onpointerdown={(event) => { event.preventDefault(); dragging = true; }}
			ondblclick={() => { if (notebook) notebookToolsWidth = notebookDefaultWidth; else toolWidth = 480; rememberWidth(); }}
		></div>
	{/if}
	<aside
		class="workspace-tools-pane"
		id="workspace-tools-pane"
		aria-label={secondaryLabel}
		role={mobileTabs ? 'tabpanel' : undefined}
		aria-labelledby={mobileTabs ? 'workspace-tools-tab' : undefined}
		hidden={!toolsVisible}
	>
		{@render tools()}
	</aside>
</div>

<style>
	.workspace-chat-layout { position: relative; display: grid; flex: 1; min-width: 0; min-height: 0; grid-template-columns: minmax(0, 1fr); grid-template-rows: minmax(0, 1fr); overflow: hidden; background: var(--workspace-canvas); }
	.workspace-chat-layout[data-panel-mode='split'] { grid-template-columns: minmax(0, 1fr) 1px var(--tools-width); }
	.workspace-chat-layout[data-panel-mode='split'][data-notebook='true'] { grid-template-columns: minmax(400px, 1fr) 1px minmax(320px, var(--notebook-tools-width)); }
	.workspace-chat-layout[data-panel-mode='split'][data-notebook='true'] .workspace-tools-pane { grid-column: 3; grid-row: 1; }
	.workspace-chat-layout[data-panel-mode='split'][data-notebook='true'] .workspace-divider { grid-column: 2; grid-row: 1; }
	.workspace-chat-layout[data-panel-mode='split'][data-notebook='true'] .workspace-conversation-pane { grid-column: 1; grid-row: 1; }
	.workspace-chat-layout[data-panel-mode='mobile'] { grid-template-columns: minmax(0, 1fr); grid-template-rows: auto minmax(0, 1fr); }
	.workspace-chat-layout[data-panel-mode='mobile'] .workspace-mobile-tabs { grid-column: 1; grid-row: 1; }
	.workspace-chat-layout[data-panel-mode='mobile'] .workspace-conversation-pane,
	.workspace-chat-layout[data-panel-mode='mobile'] .workspace-tools-pane { grid-column: 1; grid-row: 2; }
	.workspace-conversation-pane, .workspace-tools-pane { display: flex; min-width: 0; min-height: 0; overflow: hidden; }
	.workspace-conversation-pane[hidden], .workspace-tools-pane[hidden], .workspace-mobile-tabs[hidden] { display: none; }
	.workspace-tools-pane { container-name: investigation-pane; container-type: inline-size; background: var(--workspace-canvas); }
	.workspace-mobile-tabs { display: flex; width: 100%; min-width: 0; justify-content: stretch; margin: 0; border-radius: var(--radius-window); }
	.workspace-mobile-tab { flex: 1 1 0; min-width: 0; min-height: 34px; padding-inline: 10px; }
	.workspace-mobile-tab.active { background: var(--bg-raised); color: var(--text); box-shadow: var(--shadow-sm); }
	.workspace-divider { position: relative; z-index: 6; background: var(--border); cursor: col-resize; touch-action: none; }
	.workspace-divider::after { content: ''; position: absolute; inset: 0 -4px; }
	.workspace-divider:hover, .workspace-divider:focus-visible, .dragging .workspace-divider { background: var(--link); outline: none; }
	.dragging { cursor: col-resize; user-select: none; }
	.workspace-tools-pane :global(.workspace), .workspace-tools-pane :global(.companion) { flex: 1; width: 100%; min-width: 0; min-height: 0; border-left: 0; }
</style>

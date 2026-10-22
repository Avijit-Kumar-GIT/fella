<script lang="ts">
	import { onMount, type Snippet } from 'svelte';

	let { open, notebook = false, expanded = $bindable(false), compact = $bindable(false), conversation, tools }: {
		open: boolean; notebook?: boolean; expanded?: boolean; compact?: boolean; conversation: Snippet; tools: Snippet;
	} = $props();
	let host = $state<HTMLDivElement>();
	let width = $state(0);
	let toolWidth = $state(480);
	let notebookDocumentWidth = $state<number | null>(null);
	let mobilePane = $state<'document' | 'conversation'>('document');
	let dragging = $state(false);
	const storageKey = 'fella:tools-width';
	const notebookStorageKey = 'fella:notebook-document-width';
	let narrow = $derived(width > 0 && width < 760);
	let solo = $derived(open && (expanded || (!notebook && compact)));
	let mobileTabs = $derived(open && notebook && narrow && !expanded);
	let notebookMaximum = $derived(Math.max(400, width - 321));
	let notebookDefaultWidth = $derived(Math.round(width * 0.62));
	let notebookActualWidth = $derived(Math.min(notebookMaximum, Math.max(400, notebookDocumentWidth ?? notebookDefaultWidth)));
	let maximum = $derived(Math.max(400, width - 380));
	let actualWidth = $derived(Math.min(maximum, Math.max(400, toolWidth)));
	let activeWidth = $derived(notebook ? notebookActualWidth : actualWidth);
	let activeMaximum = $derived(notebook ? notebookMaximum : maximum);
	let conversationVisible = $derived(!solo && (!mobileTabs || mobilePane === 'conversation'));
	let toolsVisible = $derived(open && (!mobileTabs || mobilePane === 'document'));
	let mode = $derived(mobileTabs ? 'mobile' : solo ? 'expanded' : open ? 'split' : 'conversation');

	onMount(() => {
		try {
			const saved = Number(localStorage.getItem(storageKey));
			if (Number.isFinite(saved) && saved >= 400) toolWidth = Math.min(saved, 1200);
			const savedNotebook = Number(localStorage.getItem(notebookStorageKey));
			if (Number.isFinite(savedNotebook) && savedNotebook >= 400) notebookDocumentWidth = Math.min(savedNotebook, 1200);
		} catch { /* Optional local preference. */ }
		const observer = new ResizeObserver(([entry]) => {
			width = entry.contentRect.width;
			compact = width < 900;
		});
		if (host) observer.observe(host);
		return () => observer.disconnect();
	});

	function rememberWidth(): void {
		try {
			localStorage.setItem(notebook ? notebookStorageKey : storageKey, String(Math.round(notebook ? notebookActualWidth : toolWidth)));
		} catch { /* Optional local preference. */ }
	}

	function setActiveWidth(value: number): void {
		const minimum = notebook ? 400 : 400;
		const upper = notebook ? notebookMaximum : maximum;
		const next = Math.max(minimum, Math.min(upper, value));
		if (notebook) notebookDocumentWidth = next;
		else toolWidth = next;
	}

	function move(event: PointerEvent): void {
		if (!dragging || !host) return;
		const rect = host.getBoundingClientRect();
		setActiveWidth(notebook ? event.clientX - rect.left : rect.right - event.clientX);
	}

	function finish(): void { if (dragging) { dragging = false; rememberWidth(); } }

	function resizeKey(event: KeyboardEvent): void {
		if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
		event.preventDefault();
		const delta = notebook
			? (event.key === 'ArrowRight' ? 32 : -32)
			: (event.key === 'ArrowLeft' ? 32 : -32);
		setActiveWidth(event.key === 'Home' ? 400 : event.key === 'End' ? activeMaximum : activeWidth + delta);
		rememberWidth();
	}

	function selectMobilePane(pane: 'document' | 'conversation', focus = false): void {
		mobilePane = pane;
		if (focus) document.getElementById(pane === 'document' ? 'workspace-document-tab' : 'workspace-conversation-tab')?.focus();
	}

	function tabKeydown(event: KeyboardEvent): void {
		if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') {
			event.preventDefault();
			selectMobilePane(mobilePane === 'document' ? 'conversation' : 'document', true);
		} else if (event.key === 'Home') {
			event.preventDefault();
			selectMobilePane('document', true);
		} else if (event.key === 'End') {
			event.preventDefault();
			selectMobilePane('conversation', true);
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
	style={`--tools-width:${actualWidth}px;--document-width:${notebookActualWidth}px`}
>
	<div class="workspace-mobile-tabs fella-pill-nav" role="tablist" aria-label="Workspace panes" hidden={!mobileTabs}>
		<button
			id="workspace-document-tab"
			class="workspace-mobile-tab fella-pill-nav-item"
			class:active={mobilePane === 'document'}
			type="button"
			role="tab"
			aria-selected={mobilePane === 'document'}
			aria-controls="workspace-tools-pane"
			tabindex={mobilePane === 'document' ? 0 : -1}
			onclick={() => selectMobilePane('document')}
			onkeydown={tabKeydown}
		>Document</button>
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
		>Chat</button>
	</div>

	<section
		class="workspace-conversation-pane"
		id="workspace-conversation-pane"
		aria-label="Conversation"
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
			aria-label={notebook ? 'Resize document pane' : 'Resize workspace tools'}
			aria-orientation="vertical"
			aria-valuemin={400}
			aria-valuemax={Math.round(activeMaximum)}
			aria-valuenow={Math.round(activeWidth)}
			tabindex="0"
			onkeydown={resizeKey}
			onpointerdown={(event) => { event.preventDefault(); dragging = true; }}
			ondblclick={() => { if (notebook) notebookDocumentWidth = notebookDefaultWidth; else toolWidth = 480; rememberWidth(); }}
		></div>
	{/if}
	<aside
		class="workspace-tools-pane"
		id="workspace-tools-pane"
		aria-label="Workspace tools"
		role={mobileTabs ? 'tabpanel' : undefined}
		aria-labelledby={mobileTabs ? 'workspace-document-tab' : undefined}
		hidden={!toolsVisible}
	>
		{@render tools()}
	</aside>
</div>

<style>
	.workspace-chat-layout { position: relative; display: grid; flex: 1; min-width: 0; min-height: 0; grid-template-columns: minmax(0, 1fr); grid-template-rows: minmax(0, 1fr); overflow: hidden; background: var(--workspace-canvas); }
	.workspace-chat-layout[data-panel-mode='split'] { grid-template-columns: minmax(0, 1fr) 1px var(--tools-width); }
	.workspace-chat-layout[data-panel-mode='split'][data-notebook='true'] { grid-template-columns: minmax(400px, var(--document-width)) 1px minmax(320px, 1fr); }
	.workspace-chat-layout[data-panel-mode='split'][data-notebook='true'] .workspace-tools-pane { grid-column: 1; grid-row: 1; }
	.workspace-chat-layout[data-panel-mode='split'][data-notebook='true'] .workspace-divider { grid-column: 2; grid-row: 1; }
	.workspace-chat-layout[data-panel-mode='split'][data-notebook='true'] .workspace-conversation-pane { grid-column: 3; grid-row: 1; }
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

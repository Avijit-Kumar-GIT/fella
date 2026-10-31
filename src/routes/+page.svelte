<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import CommandPalette from '$lib/components/CommandPalette.svelte';
	import CompanionPane from '$lib/components/CompanionPane.svelte';
	import Composer from '$lib/components/Composer.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import Logo from '$lib/components/Logo.svelte';
	import Sidebar from '$lib/components/Sidebar.svelte';
	import SettingsView from '$lib/components/SettingsView.svelte';
	import Titlebar from '$lib/components/Titlebar.svelte';
	import Transcript from '$lib/components/Transcript.svelte';
	import WorkspaceView from '$lib/components/WorkspaceView.svelte';
	import WorkspaceFilesList from '$lib/components/WorkspaceFilesList.svelte';
	import WorkspaceSourceView from '$lib/components/WorkspaceSourceView.svelte';
	import StudioDetailsView from '$lib/components/StudioDetailsView.svelte';
	import { APP_WORKSPACE_ID } from '$lib/analytics-artifacts';
	import WorkspacePanelLayout from '$lib/components/WorkspacePanelLayout.svelte';
	import { dispatch, loadStartupCatalog, openContext, openFolder, openRepository, stop } from '$lib/commands';
	import { ipc, isDesktop } from '$lib/ipc';
	import { fadeQuick } from '$lib/motion';
	import { prefs } from '$lib/prefs.svelte';
	import { GENERAL_WORKSPACE_ID, session } from '$lib/session.svelte';
	import type { SourceInfo } from '$lib/types';

	let transcript = $state<Transcript | undefined>();
	let composer = $state<Composer | undefined>();
	let paletteOpen = $state(false);
	let dragging = $state(false);
	let toolsExpanded = $state(false);
	let toolsCompact = $state(false);
	let selectedSource = $state<SourceInfo | null>(null);
	let selectedFolder = $state('all');
	let fileInspectorOpen = $state(true);
	let reportInspectorOpen = $state(true);
	let pendingSourcePrompt = $state('');
	let windowWidth = $state(1440);
	let narrowNavigation = $derived(windowWidth < ((session.workspacePaneOpen || session.workspaceSection !== 'chat') ? 980 : 700));
	$effect(() => {
		if (narrowNavigation) untrack(() => { session.sidebarCollapsed = true; });
	});

	let activeView = $derived(session.workspaceView);
	let showRightPane = $derived.by(() => {
		if (session.workspaceSection === 'files') return Boolean(session.catalog.workspace) && fileInspectorOpen;
		if (session.workspaceSection === 'reports') return reportInspectorOpen;
		return session.workspacePaneOpen || Boolean(session.activeChat.companionPane) ||
			(session.contentLayout === 'notebook' && !session.catalog.workspace);
	});
	let scopedWorkspaceActive = $derived(
		Boolean(session.activeChat.workspaceScope && session.catalog.workspace === session.activeChat.workspaceScope)
	);
	$effect(() => {
		const sources = session.catalog.sources;
		const selected = selectedSource;
		if (selected && sources.some((source) => source.path === selected.path && source.name === selected.name)) return;
		selectedSource = sources[0] ?? null;
	});
	$effect(() => {
		const workspace = session.catalog.workspace;
		void workspace;
		untrack(() => { selectedFolder = 'all'; });
	});
	$effect(() => {
		const mode = session.workspaceSection;
		if (mode === 'files') untrack(() => { fileInspectorOpen = true; });
		if (mode === 'reports') untrack(() => { reportInspectorOpen = true; });
	});
	$effect(() => {
		const prompt = pendingSourcePrompt;
		if (session.workspaceSection !== 'chat' || !prompt || !composer) return;
		queueMicrotask(() => {
			if (!composer || pendingSourcePrompt !== prompt) return;
			composer.setText(prompt);
			composer.focus();
			pendingSourcePrompt = '';
		});
	});

	function relativeSourcePath(path: string): string {
		const root = session.catalog.workspace?.replace(/[/\\]+$/, '') ?? '';
		const normalizedPath = path.replace(/\\/g, '/');
		const normalizedRoot = root.replace(/\\/g, '/').replace(/\/$/, '');
		return normalizedRoot && normalizedPath.toLocaleLowerCase().startsWith(`${normalizedRoot.toLocaleLowerCase()}/`)
			? normalizedPath.slice(normalizedRoot.length + 1)
			: normalizedPath;
	}

	function selectFolder(folder: string): void {
		selectedFolder = folder;
		const next = session.catalog.sources.find((source) => {
			const segments = relativeSourcePath(source.path).split('/').filter(Boolean);
			const id = segments.length > 1 ? segments[0] : '__root';
			return folder === 'all' || id === folder;
		});
		selectedSource = next ?? null;
		fileInspectorOpen = true;
	}

	function askAboutSource(source: SourceInfo): void {
		session.workspaceSection = 'chat';
		fileInspectorOpen = true;
		pendingSourcePrompt = `What should I know about ${source.name}?`;
	}
	$effect(() => {
		const pane = session.activeChat.companionPane;
		if (!pane || !scopedWorkspaceActive) return;
		untrack(() => {
			if (session.openPreviewTool(pane)) {
				session.activeChat.companionPane = null;
				session.persist();
			}
		});
	});

	function mountStatusLabel(progress: NonNullable<typeof session.mountProgress>): string {
		const count = (value: number) => new Intl.NumberFormat().format(value);
		if (progress.phase === 'scanning') {
			return progress.visited_files
				? `Scanning folder · ${count(progress.visited_files)} files checked`
				: 'Scanning folder…';
		}
		if (progress.phase === 'waiting') return 'Finishing the current analysis…';
		if (progress.ingest) {
			const fileName = progress.ingest.path.split(/[\\/]/).pop() || progress.ingest.path;
			const percent = Math.floor(
				(progress.ingest.bytes_read / Math.max(1, progress.ingest.total_bytes)) * 100
			);
			const action = progress.ingest.stage === 'profiling' ? 'Profiling' : 'Loading';
			return `${action} ${fileName} · ${count(Math.min(100, percent))}%`;
		}
		return `Preparing data · ${count(progress.prepared_files)} of ${count(progress.total_supported_files ?? progress.supported_files)} sources`;
	}

	function mountProgressValue(progress: NonNullable<typeof session.mountProgress>): number {
		const total = progress.total_supported_files ?? progress.supported_files;
		const ingest = progress.ingest;
		if (!ingest || total <= 0 || ingest.total_bytes <= 0) return progress.prepared_files;

		const fileFraction = Math.min(1, ingest.bytes_read / ingest.total_bytes);
		const passFraction = ingest.stage === 'loading' ? 0.5 + fileFraction * 0.5 : fileFraction * 0.5;
		return Math.min(total, progress.prepared_files + passFraction);
	}

	async function refreshHealth() {
		if (!isDesktop()) return;
		try {
			session.health = await ipc.providerHealth();
		} catch {
			/* keep the last value; the next tick retries */
		}
	}

	onMount(() => {
		void session.rollOver();
		composer?.focus();
		window.fella?.onAppCommand?.((command) => {
			switch (command) {
				case 'new-conversation': startConversation(); break;
				case 'open-folder': void openFolder(); break;
				case 'settings': session.setWorkspaceView('settings'); break;
				case 'search': paletteOpen = true; break;
				case 'toggle-sidebar': session.toggleSidebar(); break;
				case 'show-chat': session.setContentLayout('chat'); session.setWorkspaceView('ask'); break;
				case 'open-notebook':
					session.setWorkspaceView('ask');
					session.setContentLayout('notebook');
					break;
			}
		});
		if (!isDesktop()) return;

		void ipc
			.appReady()
			.then((ms) => console.info('fella interactive in', ms, 'ms'))
			.catch(() => {});
		void ipc.getSettings().then((s) => { session.settings = s; }).catch(() => {});
		void loadStartupCatalog();
		void ipc.listProviders().then((p) => { session.providers = p; }).catch(() => {});

		// Poll quickly while disconnected so a just-fixed key is picked up within
		// seconds; back off once healthy.
		let timer: ReturnType<typeof setTimeout>;
		const tick = () => {
			void refreshHealth().finally(() => {
				timer = setTimeout(tick, session.health?.reachable ? 20000 : 4000);
			});
		};
		tick();

		const onVisible = () => {
			if (document.visibilityState === 'visible') void refreshHealth();
		};
		document.addEventListener('visibilitychange', onVisible);
		window.addEventListener('focus', onVisible);

		return () => {
			clearTimeout(timer);
			document.removeEventListener('visibilitychange', onVisible);
			window.removeEventListener('focus', onVisible);
		};
	});

	function startConversation(): void {
		session.newConversation();
		composer?.focus();
	}

	async function selectWorkspace(id: string): Promise<void> {
		session.setWorkspaceView('ask');
		if (id === GENERAL_WORKSPACE_ID) {
			session.focusWorkspace(GENERAL_WORKSPACE_ID);
			return;
		}
		if (session.workspaceAt(id) && !session.workspaceAt(id)?.historyOnly) {
			session.focusWorkspace(id);
			return;
		}
		const opened = await openRepository(id);
		if (!opened && session.historyOnlyRepositoryPaths.includes(id)) {
			session.openHistoryOnlyWorkspace(id);
		}
	}

	function onKey(e: KeyboardEvent) {
		if (e.defaultPrevented) return;
		const commandKey = e.ctrlKey || e.metaKey;
		const key = e.key.toLowerCase();
		// Electron's native application menu owns these accelerators. Keep this
		// handler for the browser preview, but don't perform an action twice in
		// the desktop app when the native menu dispatches its command.
		if (isDesktop() && commandKey && !e.shiftKey && ['n', 'o', ',', 'k', 'b'].includes(key)) {
			e.preventDefault();
			return;
		}
		if (key === 'escape' && narrowNavigation && !session.sidebarCollapsed && !paletteOpen) { session.sidebarCollapsed = true; return; }
		if (commandKey && key === 'k') {
			e.preventDefault();
			paletteOpen = !paletteOpen;
			return;
		}
		if (commandKey && e.shiftKey && key === 'p') {
			e.preventDefault();
			paletteOpen = !paletteOpen;
			return;
		}
		if (paletteOpen) return;
		if (commandKey && (key === '[' || key === ']')) {
			e.preventDefault();
			const workspaces = [
				GENERAL_WORKSPACE_ID,
				...session.repositoryPaths.filter((path) => !session.hiddenRepositoryPaths.includes(path))
			];
			const count = workspaces.length;
			const direction = key === ']' ? 1 : -1;
			const current = session.activeWorkspaceId ?? GENERAL_WORKSPACE_ID;
			const index = workspaces.indexOf(current);
			void selectWorkspace(workspaces[(Math.max(0, index) + direction + count) % count]);
		} else if (commandKey && /^[1-9]$/.test(key)) {
			e.preventDefault();
			const workspaces = [
				GENERAL_WORKSPACE_ID,
				...session.repositoryPaths.filter((path) => !session.hiddenRepositoryPaths.includes(path))
			];
			const workspace = workspaces[Number(key) - 1];
			if (workspace) void selectWorkspace(workspace);
		} else if (commandKey && e.shiftKey && key === 'a') {
			e.preventDefault();
			startConversation();
		} else if (commandKey && e.shiftKey && key === 's') {
			e.preventDefault();
			session.setWorkspacePane('investigation');
		} else if (commandKey && e.shiftKey && key === 'c') {
			e.preventDefault();
			void openContext();
		} else if (commandKey && key === ',') {
			e.preventDefault();
			session.setWorkspaceView('settings');
		} else if (commandKey && key === 'o') {
			e.preventDefault();
			void openFolder();
		} else if (commandKey && key === 'n') {
			e.preventDefault();
			startConversation();
		} else if (commandKey && key === 'l') {
			e.preventDefault();
			void session.clear();
		} else if (commandKey && key === 'b') {
			e.preventDefault();
			session.toggleSidebar();
		} else if (commandKey && e.shiftKey && key === 'f') {
			e.preventDefault();
			session.focus = !session.focus;
		} else if (e.key === 'Escape' && !paletteOpen) {
			if (session.pendingKey) {
				session.pendingKey = null;
				session.addSystem('Cancelled.');
			} else if (session.busy) void stop();
			else transcript?.collapseAll();
		}
	}

	function onDragOver(e: DragEvent): void {
		if (e.dataTransfer?.types.includes('Files')) {
			e.preventDefault();
			dragging = true;
		}
	}

	function onDragLeave(e: DragEvent): void {
		if (e.relatedTarget === null) dragging = false;
	}

	function onDrop(e: DragEvent): void {
		e.preventDefault();
		dragging = false;
		const file = e.dataTransfer?.files?.[0];
		if (!file) return;
		const path = window.fella?.pathForFile(file) ?? (file as File & { path?: string }).path;
		if (path) void dispatch(`/open ${path}`);
	}

	// Persist the conversation transcript as it changes.
	$effect(() => {
		session.conversations.length;
		for (const conversation of session.conversations) {
			conversation.messages.length;
			conversation.messages.at(-1)?.text;
			conversation.messages.at(-1)?.pending;
		}
		session.persist();
	});

	// Apply the selected appearance to <html>.
	$effect(() => {
		prefs.appearance;
		prefs.apply();
	});

	function pickCommand(cmd: string) {
		const noArg = [
			'/files',
			'/help',
			'/clear',
			'/model',
			'/auth',
			'/history',
			'/new',
			'/plain',
			'/context'
		];
		session.setWorkspaceView('ask');
		const text = noArg.includes(cmd) ? cmd : cmd + ' ';
		queueMicrotask(() => {
			composer?.setText(text);
			composer?.focus();
		});
	}

	$effect(() => {
		activeView;
		if (activeView === 'ask') queueMicrotask(() => composer?.focus());
	});

	// Keep the no-folder Ask onboarding as the canonical fallback if the
	// workspace disappears while a catalog-dependent pane is open.
	$effect(() => {
		const view = session.workspaceView;
		const mounted = session.catalog.workspace;
		if (!mounted && view === 'workspace') {
			session.setWorkspaceView('ask');
		}
	});

	// A screen reader gets nothing during a run otherwise (the answer streams
	// into a div it isn't watching). Announce what Fella is doing, and that the
	// answer has landed.
	let live = $derived.by(() => {
		if (session.mountProgress?.phase === 'scanning') return 'Scanning the mounted folder';
		if (session.mountProgress?.phase === 'preparing') return 'Preparing workspace data';
		if (session.mountProgress?.phase === 'waiting') return 'Waiting for the current analysis to finish';
		if (session.mountProgress?.phase === 'ready') return 'Workspace ready';
		if (session.busy) return session.activity || 'working…';
		const last = session.messages.at(-1);
		return last?.role === 'assistant' && last.text.trim() ? 'answer ready' : '';
	});
</script>

<svelte:window bind:innerWidth={windowWidth} onkeydown={onKey} ondragover={onDragOver} ondragleave={onDragLeave} ondrop={onDrop} />

	<div class="shell" class:narrow-navigation={narrowNavigation} class:native-titlebar={isDesktop()}>
		{#if isDesktop()}<Titlebar nativeOverlay onpalette={() => (paletteOpen = true)} onworkspaceselect={selectWorkspace} expanded={toolsExpanded} compact={toolsCompact} ontoggleexpand={() => (toolsExpanded = !toolsExpanded)} />{/if}
		{#if narrowNavigation && !session.focus && !session.sidebarCollapsed}<button class="navigation-backdrop" aria-label="Close navigation" onclick={() => session.toggleSidebar()}></button>{/if}
		{#if !session.focus && !session.sidebarCollapsed}
			<Sidebar
				bind:mode={session.workspaceSection}
				onworkspaceselect={selectWorkspace}
				onsearch={() => (paletteOpen = true)}
				onsectionselect={(section) => { if (section === 'files') fileInspectorOpen = true; if (section === 'reports') reportInspectorOpen = true; }}
				selectedFolder={selectedFolder}
				onselectfolder={selectFolder}
			/>
		{/if}
	<div class="app" class:focus={session.focus}>
		{#if session.sidebarCollapsed || session.focus}
			<div class="collapsed-navigation"><button aria-label="Expand sidebar" title="Expand sidebar" onclick={() => { session.focus = false; if (session.sidebarCollapsed) session.toggleSidebar(); }}><Icon name="panel" size={14} /></button></div>
		{/if}
		<section class="workspace-window" aria-label="Current workspace">
			{#if session.mountProgress && session.mountProgress.phase !== 'ready'}
				<div class="mount-status" aria-hidden="true">
					<span class="mount-orb"><Logo size={17} active /></span>
					<span class="mount-label">{mountStatusLabel(session.mountProgress)}</span>
					{#if session.mountProgress.phase === 'preparing' && session.mountProgress.total_supported_files}
						<progress
							value={mountProgressValue(session.mountProgress)}
							max={session.mountProgress.total_supported_files}
						></progress>
					{:else}
						<progress></progress>
					{/if}
				</div>
			{/if}
			<main>
				<div class="main-row">
					{#if activeView === 'settings'}
						<div class="settings-workspace"><SettingsView /></div>
					{:else}
						<WorkspacePanelLayout
							open={showRightPane}
							notebook={true}
							primaryLabel={session.workspaceSection === 'files' ? 'Files' : session.workspaceSection === 'reports' ? 'Reports' : 'Chat'}
							secondaryLabel={session.workspaceSection === 'files' ? 'Preview' : session.workspaceSection === 'reports' ? 'Report details' : 'Investigation'}
							defaultToolsWidth={session.workspaceSection === 'reports' ? 320 : 380}
							layoutKey={session.workspaceSection}
							bind:expanded={toolsExpanded}
							bind:compact={toolsCompact}
						>
							{#snippet conversation()}
								<div class="primary-workspace" class:reports={session.workspaceSection === 'reports'} class:navigation-collapsed={session.sidebarCollapsed}>
								{#if !isDesktop() && session.workspaceSection !== 'reports'}<Titlebar onpalette={() => (paletteOpen = true)} onworkspaceselect={selectWorkspace} expanded={toolsExpanded} compact={toolsCompact} ontoggleexpand={() => (toolsExpanded = !toolsExpanded)} />{/if}
								<div class="conversation-workspace" hidden={session.workspaceSection !== 'chat'}>
										<Transcript bind:this={transcript} />
										<div class="dock"><Composer bind:this={composer} onafterrun={refreshHealth} /></div>
									</div>
								{#if session.workspaceSection === 'files'}
									<WorkspaceFilesList folder={selectedFolder} selected={selectedSource} onselect={(source) => { selectedSource = source; fileInspectorOpen = true; }} />
								{:else if session.workspaceSection === 'reports'}
									<WorkspaceView hideToolBar expanded={toolsExpanded} onexpand={() => (toolsExpanded = !toolsExpanded)} onasksource={askAboutSource} />
								{/if}
								</div>
							{/snippet}
							{#snippet tools()}
								{#if session.workspaceSection === 'chat' && scopedWorkspaceActive}
									<WorkspaceView expanded={toolsExpanded} onexpand={() => (toolsExpanded = !toolsExpanded)} onasksource={askAboutSource} defaultInspector />
								{:else if session.workspaceSection === 'files' && session.catalog.workspace}
									<WorkspaceSourceView source={selectedSource} workspacePath={session.catalog.workspace} revision={session.catalog.revision ?? null} onclose={() => (fileInspectorOpen = false)} onask={askAboutSource} />
								{:else if session.workspaceSection === 'reports'}
										<StudioDetailsView workspace={APP_WORKSPACE_ID} documentId={session.activeWorkspaceTool?.kind === 'studio' ? session.activeWorkspaceTool.documentId ?? null : null} onclose={() => (reportInspectorOpen = false)} />
								{/if}
								{#if session.contentLayout === 'notebook' && !scopedWorkspaceActive && session.workspaceSection === 'chat'}
									<div class="notebook-empty"><h1>Open a folder to explore your information</h1><p>Your conversation stays here. Notebook view organizes your files, Investigations and reports beside it.</p><button onclick={() => void openFolder()}>Open folder</button></div>
								{:else if session.workspaceSection === 'chat' && showRightPane && !session.workspacePaneOpen}
									<CompanionPane />
								{/if}
							{/snippet}
						</WorkspacePanelLayout>
					{/if}
				</div>
			</main>
		</section>
	</div>
</div>

<div class="sr-only" role="status" aria-live="polite">{live}</div>

{#if dragging}
	<div class="dropzone" transition:fadeQuick aria-hidden="true">
		<div class="dropcard">Drop a folder or workspace to open it</div>
	</div>
{/if}

<CommandPalette bind:open={paletteOpen} onpick={pickCommand} />

<style>
	.navigation-backdrop { position: absolute; inset: 0; z-index: 39; border: 0; background: rgb(0 0 0 / .25); }
	.narrow-navigation :global(.sidebar) { position: absolute; inset: 0 auto 0 0; z-index: 40; max-width: calc(100% - 48px); box-shadow: var(--shadow-pop); }
	.notebook-empty { display: flex; flex: 1; min-width: 0; flex-direction: column; align-items: flex-start; justify-content: center; gap: 16px; padding: clamp(20px, 6vw, 64px); }
	.notebook-empty h1 { font-size: var(--fs-xl); margin: 0; }
	.notebook-empty p { max-width: 44ch; color: var(--text-dim); line-height: 1.6; }
	.notebook-empty button { padding: 8px 16px; border: 1px solid var(--border); border-radius: 999px; background: var(--bg-raised); color: var(--text); cursor: pointer; }
	.shell {
		position: relative;
		display: flex;
		height: 100%;
		min-width:0; overflow:hidden; border:1px solid var(--border-strong); border-radius:8px;
	}
	.shell.native-titlebar { padding-top:32px; border:0; border-radius:0; }
	.app {
		position:relative;
		display: flex;
		flex-direction: column;
		flex: 1;
		min-width: 0;
		background: var(--app-chrome);
	}
	.workspace-window {
		position: relative;
		display: flex;
		flex: 1;
		flex-direction: column;
		min-width: 0;
		min-height: 0;
		margin: 0;
		overflow: hidden;
		border: 0;
		border-radius: 0;
		background: var(--workspace-surface);
		box-shadow: none;
	}
	main {
		flex: 1;
		min-height: 0;
		display: flex;
		flex-direction: column;
		min-width: 0;
		background: var(--workspace-canvas);
	}
	.mount-status {
		flex: none;
		display: flex;
		align-items: center;
		gap: var(--space-2);
		min-height: 32px;
		padding: 0 var(--pad);
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.mount-orb {
		flex: none;
		display: grid;
		place-items: center;
		width: 18px;
		height: 18px;
	}
	.mount-label {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.mount-status progress {
		flex: none;
		width: clamp(64px, 12vw, 140px);
		height: 3px;
		border: 0;
		border-radius: 99px;
		background: var(--bg-raised);
		accent-color: var(--accent);
	}
	.mount-status progress::-webkit-progress-bar {
		border-radius: 99px;
		background: var(--bg-raised);
	}
	.mount-status progress::-webkit-progress-value {
		border-radius: 99px;
		background: var(--accent);
	}
	.main-row {
		position: relative;
		flex: 1;
		min-width: 0;
		min-height: 0;
		display: flex;
		container-name: workspace-area;
		container-type: inline-size;
		background: var(--workspace-canvas);
	}
	.primary-workspace { position:relative; display:flex; flex:1; min-width:0; min-height:0; flex-direction:column; overflow:hidden; container-name:primary-pane; container-type:inline-size; }
	.primary-workspace.reports.navigation-collapsed { padding-top:52px; }
	.settings-workspace { display:flex; flex:1; min-height:0; padding-top:56px; }
	.collapsed-navigation { position:absolute; top:16px; left:16px; z-index:25; display:flex; align-items:center; -webkit-app-region:drag; }
	.collapsed-navigation > button { display:grid; width:28px; height:28px; place-items:center; border-radius:50%; color:var(--text-dim); background:var(--bg-raised); -webkit-app-region:no-drag; }
	.conversation-workspace[hidden] { display:none; }
	.conversation-workspace {
		display: flex;
		flex: 1;
		flex-direction: column;
		min-width: 0;
		min-height: 0;
		overflow: hidden;
	}
	.conversation-workspace > :global(.transcript) {
		flex: 1;
		min-width: 0;
		min-height: 0;
	}
	.dock {
		flex: none;
		min-width: 0;
		background: var(--workspace-canvas);
	}
	.dropzone {
		position: fixed;
		inset: 0;
		z-index: 40;
		display: grid;
		place-items: center;
		background: color-mix(in srgb, var(--bg) 68%, transparent);
		outline: 2px dashed var(--border-strong);
		outline-offset: -12px;
	}
	.dropcard {
		display: inline-flex;
		align-items: center;
		padding: var(--space-3) var(--space-4);
		background: var(--bg-raised);
		border: 1px solid var(--border);
		border-radius: var(--radius);
		box-shadow: var(--shadow-sm);
		color: var(--text-dim);
		font-size: var(--fs-sm);
	}
</style>

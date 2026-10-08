<script lang="ts">
	import { onMount } from 'svelte';
	import CommandPalette from '$lib/components/CommandPalette.svelte';
	import CompanionPane from '$lib/components/CompanionPane.svelte';
	import Composer from '$lib/components/Composer.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import Logo from '$lib/components/Logo.svelte';
	import ProjectDialog from '$lib/components/ProjectDialog.svelte';
	import ProjectView from '$lib/components/ProjectView.svelte';
	import Sidebar from '$lib/components/Sidebar.svelte';
	import SettingsView from '$lib/components/SettingsView.svelte';
	import Titlebar from '$lib/components/Titlebar.svelte';
	import Transcript from '$lib/components/Transcript.svelte';
	import WorkspaceView from '$lib/components/WorkspaceView.svelte';
	import { dispatch, loadStartupCatalog, openFolder, stop } from '$lib/commands';
	import { ipc, isDesktop } from '$lib/ipc';
	import { fadeQuick } from '$lib/motion';
	import { prefs } from '$lib/prefs.svelte';
	import { session } from '$lib/session.svelte';

	let transcript = $state<Transcript | undefined>();
	let composer = $state<Composer | undefined>();
	let paletteOpen = $state(false);
	let projectDialogOpen = $state(false);
	let dragging = $state(false);

	let activeView = $derived(session.workspaceView);

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

	function moveTab(delta: number): void {
		if (session.tabs.length < 2) return;
		const next = (session.active + delta + session.tabs.length) % session.tabs.length;
		session.activateTab(next);
		composer?.focus();
	}

	function onKey(e: KeyboardEvent) {
		if (e.defaultPrevented) return;
		const commandKey = e.ctrlKey || e.metaKey;
		const key = e.key.toLowerCase();
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
		if (commandKey && e.shiftKey && key === 'a') {
			e.preventDefault();
			session.setWorkspaceView('ask');
			session.newTab();
			composer?.focus();
		} else if (commandKey && e.shiftKey && key === 's') {
			e.preventDefault();
			session.setWorkspacePane('sources');
		} else if (commandKey && e.shiftKey && key === 'c') {
			e.preventDefault();
			session.setWorkspacePane('context');
		} else if (commandKey && key === ',') {
			e.preventDefault();
			session.setWorkspaceView('settings');
		} else if (commandKey && key === 'o') {
			e.preventDefault();
			void openFolder();
		} else if (commandKey && key === 'n') {
			e.preventDefault();
			session.setWorkspaceView('ask');
			session.newTab();
			composer?.focus();
		} else if (commandKey && e.key === '[') {
			e.preventDefault();
			moveTab(-1);
		} else if (commandKey && e.key === ']') {
			e.preventDefault();
			moveTab(1);
		} else if (commandKey && key === 'l') {
			e.preventDefault();
			void session.clear();
		} else if (commandKey && key === 'b') {
			e.preventDefault();
			session.toggleSidebar();
		} else if (commandKey && key === 't') {
			e.preventDefault();
			session.setWorkspaceView('ask');
			session.newTab();
			composer?.focus();
		} else if (commandKey && key === 'w') {
			e.preventDefault();
			void session.closeTab(session.active);
			composer?.focus();
		} else if (commandKey && e.key >= '1' && e.key <= '9') {
			const i = Number(e.key) - 1;
			if (i < session.tabs.length) {
				e.preventDefault();
				session.activateTab(i);
				composer?.focus();
			}
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
		session.tabs.length;
		for (const t of session.tabs) {
			if (t.kind !== 'chat') continue;
			t.messages.length;
			t.messages.at(-1)?.text;
			t.messages.at(-1)?.pending;
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
			'/tab',
			'/focus',
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

<svelte:window onkeydown={onKey} ondragover={onDragOver} ondragleave={onDragLeave} ondrop={onDrop} />

	<div class="shell">
		{#if !session.focus && !session.sidebarCollapsed}
			<Sidebar onsearch={() => (paletteOpen = true)} onnewproject={() => (projectDialogOpen = true)} />
		{/if}
	<div class="app" class:focus={session.focus}>
		<Titlebar onpalette={() => (paletteOpen = true)} />
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
					{#if activeView === 'workspace'}
						<WorkspaceView />
					{:else if activeView === 'project'}
						<ProjectView />
					{:else if activeView === 'settings'}
						<SettingsView />
					{:else}
						<div class="ask-workbench">
							<Transcript bind:this={transcript} />
							{#if session.activeChat.companionPane}
								<CompanionPane />
							{/if}
						</div>
					{/if}
				</div>
			</main>
			<div class="dock">
				{#if activeView === 'ask'}
					<Composer bind:this={composer} onafterrun={refreshHealth} />
				{/if}
			</div>
		</section>
	</div>
</div>

<div class="sr-only" role="status" aria-live="polite">{live}</div>

{#if dragging}
	<div class="dropzone" transition:fadeQuick aria-hidden="true">
		<div class="dropcard"><Icon name="folder" size={20} /> Drop a folder to open it</div>
	</div>
{/if}

<CommandPalette bind:open={paletteOpen} onpick={pickCommand} />
<ProjectDialog bind:open={projectDialogOpen} />

<style>
	.shell {
		position: relative;
		display: flex;
		height: 100%;
	}
	.app {
		display: flex;
		flex-direction: column;
		flex: 1;
		min-width: 0;
		background: var(--bg);
	}
	.workspace-window {
		position: relative;
		display: flex;
		flex: 1;
		flex-direction: column;
		min-width: 0;
		min-height: 0;
		margin: 8px 10px 10px;
		overflow: hidden;
		border: 1px solid var(--border);
		border-radius: var(--radius-window);
		background: var(--bg-raised);
	}
	main {
		flex: 1;
		min-height: 0;
		display: flex;
		flex-direction: column;
		min-width: 0;
		background: transparent;
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
	}
	.ask-workbench {
		position: relative;
		display: flex;
		flex: 1;
		min-width: 0;
		min-height: 0;
	}
	.ask-workbench > :global(.transcript) { flex: 1; min-width: 0; }
	/* Keep the composer inside the framed workspace, aligned to its content. */
	.dock {
		flex: none;
		padding: 0 var(--space-2) var(--space-3);
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
		gap: var(--space-2);
		padding: var(--space-3) var(--space-4);
		background: var(--bg-raised);
		border: 1px solid var(--border);
		border-radius: var(--radius);
		box-shadow: var(--shadow-sm);
		color: var(--text-dim);
		font-size: var(--fs-sm);
	}
</style>

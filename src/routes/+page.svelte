<script lang="ts">
	import { onMount } from 'svelte';
	import CommandPalette from '$lib/components/CommandPalette.svelte';
	import Composer from '$lib/components/Composer.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import StatusBar from '$lib/components/StatusBar.svelte';
	import Titlebar from '$lib/components/Titlebar.svelte';
	import Transcript from '$lib/components/Transcript.svelte';
	import AugmentView from '$lib/components/AugmentView.svelte';
	import { dispatch, loadStartupCatalog, reconcileModel, stop } from '$lib/commands';
	import { ipc, isTauri } from '$lib/ipc';
	import { fadeQuick } from '$lib/motion';
	import { prefs } from '$lib/prefs.svelte';
	import { session } from '$lib/session.svelte';

	let transcript = $state<Transcript | undefined>();
	let composer = $state<Composer | undefined>();
	let paletteOpen = $state(false);
	let dragging = $state(false);

	let activeTab = $derived(session.activeTab);

	async function refreshHealth() {
		if (!isTauri()) return;
		try {
			session.health = await ipc.ollamaHealth();
			await reconcileModel();
		} catch {
			/* keep the last value; the next tick retries */
		}
		// Also look for a local Ollama regardless of the current provider, so
		// "you just installed it" gets noticed. Skip when we're already on a
		// reachable Ollama that check would be redundant.
		if (!(session.settings?.provider === 'ollama' && session.health?.reachable)) {
			void ipc.probeOllama().then((h) => { session.ollamaLocal = h; }).catch(() => {});
		} else {
			session.ollamaLocal = session.health;
		}
	}

	onMount(() => {
		void session.rollOver();
		composer?.focus();
		if (!isTauri()) return;

		void ipc
			.appReady()
			.then((ms) => console.info('fella interactive in', ms, 'ms'))
			.catch(() => {});
		void ipc.getSettings().then((s) => { session.settings = s; }).catch(() => {});
		void loadStartupCatalog();
		void ipc.listProviders().then((p) => { session.providers = p; }).catch(() => {});
		void ipc.packsList().then((p) => { session.packs = p; }).catch(() => {});
		void ipc.augmentCapabilities().then((c) => { session.augmentCapabilities = c; }).catch(() => {});
		void prefs.load();

		// Poll quickly while disconnected so a freshly-started Ollama or a
		// just-fixed key is picked up within seconds; back off once healthy.
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

		// Native folder drop -> /open, with a full-window drop target while a
		// drag is over the window.
		let unlisten: (() => void) | undefined;
		void import('@tauri-apps/api/webview')
			.then(({ getCurrentWebview }) =>
				getCurrentWebview().onDragDropEvent((e) => {
					const t = e.payload.type;
					dragging = t === 'enter' || t === 'over';
					if (t === 'drop' && e.payload.paths.length) {
						void dispatch(`/open ${e.payload.paths[0]}`);
					}
				})
			)
			.then((u) => (unlisten = u))
			.catch(() => {});

		return () => {
			clearTimeout(timer);
			document.removeEventListener('visibilitychange', onVisible);
			window.removeEventListener('focus', onVisible);
			unlisten?.();
		};
	});

	function onKey(e: KeyboardEvent) {
		if (e.ctrlKey && e.key === 'l') {
			e.preventDefault();
			void session.clear();
		} else if (e.ctrlKey && e.key === 'k') {
			e.preventDefault();
			paletteOpen = !paletteOpen;
		} else if (e.ctrlKey && (e.key === 't' || e.key === 'T')) {
			e.preventDefault();
			session.newTab();
			composer?.focus();
		} else if (e.ctrlKey && (e.key === 'w' || e.key === 'W')) {
			e.preventDefault();
			void session.closeTab(session.active);
			composer?.focus();
		} else if (e.ctrlKey && e.key >= '1' && e.key <= '9') {
			const i = Number(e.key) - 1;
			if (i < session.tabs.length) {
				e.preventDefault();
				session.active = i;
				composer?.focus();
			}
		} else if (e.ctrlKey && e.shiftKey && (e.key === 'f' || e.key === 'F')) {
			e.preventDefault();
			session.focus = !session.focus;
		} else if (e.key === 'Escape' && !paletteOpen) {
			if (session.pendingKey || session.pendingConnect) {
				session.pendingKey = null;
				session.pendingConnect = null;
				session.addSystem('Cancelled.');
			} else if (session.busy) void stop();
			else transcript?.collapseAll();
		}
	}

	// Persist every conversation tab's transcript as it changes. Augment tabs
	// have no transcript and aren't persisted.
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

	// Apply the active theme pack's CSS tokens to <html>.
	$effect(() => {
		prefs.themeTokens;
		prefs.apply();
	});

	function pickCommand(cmd: string) {
		const noArg = ['/files', '/help', '/clear', '/model', '/auth', '/history', '/tab', '/focus'];
		composer?.setText(noArg.includes(cmd) ? cmd : cmd + ' ');
		composer?.focus();
	}

	// A screen reader gets nothing during a run otherwise (the answer streams
	// into a div it isn't watching). Announce what Fella is doing, and that the
	// answer has landed.
	let live = $derived.by(() => {
		if (session.busy) return session.activity || 'working…';
		const last = session.messages.at(-1);
		return last?.role === 'assistant' && last.text.trim() ? 'answer ready' : '';
	});
</script>

<svelte:window onkeydown={onKey} />

<div class="app" class:focus={session.focus}>
	<Titlebar onpalette={() => (paletteOpen = true)} />
	<main>
		{#if activeTab.kind === 'augment'}
			{#key activeTab.id}
				<AugmentView tab={activeTab} />
			{/key}
		{:else}
			<Transcript bind:this={transcript} />
		{/if}
	</main>
	<div class="dock">
		{#if !session.focus}
			<StatusBar />
		{/if}
		{#if activeTab.kind !== 'augment'}
			<Composer bind:this={composer} onafterrun={refreshHealth} />
		{/if}
	</div>
</div>

<div class="sr-only" role="status" aria-live="polite">{live}</div>

{#if dragging}
	<div class="dropzone" transition:fadeQuick aria-hidden="true">
		<div class="dropcard"><Icon name="folder" size={20} /> Drop a folder to open it</div>
	</div>
{/if}

<CommandPalette bind:open={paletteOpen} onpick={pickCommand} />

<style>
	.app {
		display: flex;
		flex-direction: column;
		height: 100%;
		background: var(--bg);
	}
	main {
		flex: 1;
		min-height: 0;
		display: flex;
		flex-direction: column;
		background: var(--bg-raised);
		border-top: 1px solid var(--border);
	}
	/* Status line + composer read as one calm footer zone, continuous with the
	   transcript surface above it no rule, no colour change. */
	.dock {
		flex: none;
		background: var(--bg-raised);
		padding-bottom: var(--space-2);
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

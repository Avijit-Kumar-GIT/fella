<script lang="ts">
	import { onMount } from 'svelte';
	import CommandPalette from '$lib/components/CommandPalette.svelte';
	import Composer from '$lib/components/Composer.svelte';
	import Header from '$lib/components/Header.svelte';
	import StatusBar from '$lib/components/StatusBar.svelte';
	import TabBar from '$lib/components/TabBar.svelte';
	import Transcript from '$lib/components/Transcript.svelte';
	import { dispatch, reconcileModel, stop } from '$lib/commands';
	import { ipc, isTauri } from '$lib/ipc';
	import { prefs } from '$lib/prefs.svelte';
	import { session } from '$lib/session.svelte';

	let transcript: Transcript;
	let composer: Composer;
	let paletteOpen = $state(false);

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
		void ipc.getCatalog().then((c) => { session.catalog = c; }).catch(() => {});
		void ipc.listProviders().then((p) => { session.providers = p; }).catch(() => {});
		void ipc.packsList().then((p) => { session.packs = p; }).catch(() => {});
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

		// Native folder drop -> /open
		let unlisten: (() => void) | undefined;
		void import('@tauri-apps/api/webview')
			.then(({ getCurrentWebview }) =>
				getCurrentWebview().onDragDropEvent((e) => {
					if (e.payload.type === 'drop' && e.payload.paths.length) {
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

	// Persist every tab's transcript as it changes.
	$effect(() => {
		session.tabs.length;
		for (const t of session.tabs) {
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

<div class="app">
	<!-- Always grabbable: moves the window even when the chrome is hidden. -->
	<div class="draghandle" data-tauri-drag-region></div>
	{#if !session.focus}
		<TabBar />
		{#if session.catalog.workspace}
			<Header />
		{/if}
	{/if}
	<main>
		<Transcript bind:this={transcript} />
	</main>
	<Composer bind:this={composer} onafterrun={refreshHealth} />
	<StatusBar />
</div>

<div class="sr-only" role="status" aria-live="polite">{live}</div>

<CommandPalette bind:open={paletteOpen} onpick={pickCommand} />

<style>
	.app {
		display: flex;
		flex-direction: column;
		height: 100%;
		background: var(--bg);
	}
	/* A thin move strip under the OS title bar, so the window is draggable from
	   the app's own top edge (and still is in focus mode). */
	.draghandle {
		flex: none;
		height: 8px;
	}
	main {
		flex: 1;
		min-height: 0;
		display: flex;
		flex-direction: column;
		background: var(--bg-raised);
		border-top: 1px solid var(--border);
	}
</style>

<script lang="ts">
	import {
		carriesSecret,
		COMMAND_DESCRIPTIONS,
		completionsFor,
		dispatch,
		stop
	} from '$lib/commands';
	import { session } from '$lib/session.svelte';
	import { enterUp } from '$lib/motion';
	import Icon from './Icon.svelte';

	let { onafterrun }: { onafterrun?: () => void } = $props();

	let value = $state('');
	let ta: HTMLTextAreaElement;
	// ↑-recall history lives on the active conversation, so each tab has its own.
	let history = $derived(session.activeTab.history);
	let histIx = -1;

	// --- completion menu -------------------------------------------------
	const MAX_ITEMS = 8;
	let menuSel = $state(-1); // -1 = nothing highlighted; Enter still submits
	let menuOff = $state(false); // dismissed with Esc until the text changes

	let pendingInput = $derived(!!session.pendingKey || !!session.pendingConnect);
	let items = $derived(menuOff || pendingInput ? [] : completionsFor(value));
	let shown = $derived(items.slice(0, MAX_ITEMS));
	let menuOpen = $derived(shown.length > 0);

	function describe(item: string): string {
		if (item.startsWith('/')) return COMMAND_DESCRIPTIONS[item] ?? '';
		const p = session.providers.find((x) => x.id === item);
		if (p) return p.auth === 'none' ? 'runs on your machine' : 'sign in with an API key';
		return '';
	}

	function grow() {
		if (!ta) return;
		ta.style.height = 'auto';
		ta.style.height = Math.min(ta.scrollHeight, 200) + 'px';
	}

	function onInput() {
		grow();
		menuSel = -1;
		menuOff = false;
	}

	/** Swap the word being typed for a chosen completion and keep going. */
	function acceptItem(item: string | undefined) {
		if (!item) return;
		const parts = value.split(/\s+/);
		parts[parts.length - 1] = item;
		value = parts.join(' ') + ' ';
		menuSel = -1;
		queueMicrotask(() => {
			grow();
			ta?.focus();
			ta?.setSelectionRange(value.length, value.length);
		});
	}

	async function submit() {
		const text = value.trim();
		if (!text || session.busy) return;
		// Don't keep a pasted secret in the ↑-recall history.
		if (!pendingInput && !carriesSecret(text)) history.unshift(text);
		histIx = -1;
		value = '';
		menuSel = -1;
		queueMicrotask(grow);
		await dispatch(text);
		onafterrun?.();
	}

	function onKey(e: KeyboardEvent) {
		// While the menu is open it owns the arrows / Tab / Esc, and Enter too
		// but only once something is highlighted.
		if (menuOpen) {
			if (e.key === 'ArrowDown') {
				menuSel = menuSel + 1 >= shown.length ? -1 : menuSel + 1;
				e.preventDefault();
				return;
			}
			if (e.key === 'ArrowUp') {
				menuSel = menuSel <= -1 ? shown.length - 1 : menuSel - 1;
				e.preventDefault();
				return;
			}
			if (e.key === 'Tab') {
				e.preventDefault();
				acceptItem(shown[menuSel >= 0 ? menuSel : 0]);
				return;
			}
			if (e.key === 'Escape') {
				menuOff = true;
				menuSel = -1;
				e.preventDefault();
				e.stopPropagation();
				return;
			}
			if (e.key === 'Enter' && !e.shiftKey) {
				const last = value.split(/\s+/).pop() ?? '';
				// Accept the highlighted row, or a lone candidate that only
				// extends what's typed; otherwise fall through and submit.
				if (menuSel >= 0) {
					e.preventDefault();
					acceptItem(shown[menuSel]);
					return;
				}
				if (shown.length === 1 && shown[0] !== last) {
					e.preventDefault();
					acceptItem(shown[0]);
					return;
				}
			}
		}

		if (e.key === 'Enter' && !e.shiftKey) {
			e.preventDefault();
			void submit();
			return;
		}
		if (e.key === 'ArrowUp' && (value === '' || histIx >= 0)) {
			if (histIx + 1 < history.length) {
				histIx++;
				value = history[histIx];
				queueMicrotask(grow);
			}
			e.preventDefault();
			return;
		}
		if (e.key === 'ArrowDown' && histIx >= 0) {
			histIx--;
			value = histIx >= 0 ? history[histIx] : '';
			queueMicrotask(grow);
			e.preventDefault();
			return;
		}
		if (e.key === 'Tab' && value.startsWith('/')) {
			// A slash command with no menu still never let Tab leave the box.
			e.preventDefault();
		}
	}

	export function focus() {
		ta?.focus();
	}

	export function setText(t: string) {
		value = t;
		menuSel = -1;
		menuOff = false;
		queueMicrotask(() => {
			grow();
			ta?.focus();
			ta?.setSelectionRange(t.length, t.length);
		});
	}
</script>

<div class="wrap">
	{#if menuOpen}
		<ul
			class="menu"
			id="composer-completions"
			role="listbox"
			aria-label="completions"
			transition:enterUp
		>
			{#each shown as item, i (item)}
				<li>
					<button
						class="rowbtn"
						type="button"
						tabindex="-1"
						role="option"
						id={'composer-opt-' + i}
						aria-selected={i === menuSel}
						class:sel={i === menuSel}
						onmousedown={(e) => {
							e.preventDefault();
							acceptItem(item);
						}}
						onmouseenter={() => (menuSel = i)}
					>
						<span class="val">{item}</span>
						{#if describe(item)}<span class="desc">{describe(item)}</span>{/if}
					</button>
				</li>
			{/each}
			{#if items.length > shown.length}
				<li class="more">+{items.length - shown.length} more keep typing</li>
			{/if}
		</ul>
	{/if}

	<div class="field" class:secret={pendingInput}>
		<textarea
			bind:this={ta}
			bind:value
			rows="1"
			spellcheck="false"
			autocapitalize="off"
			autocomplete="off"
			role="combobox"
			aria-expanded={menuOpen}
			aria-controls="composer-completions"
			aria-autocomplete="list"
			aria-activedescendant={menuOpen && menuSel >= 0 ? 'composer-opt-' + menuSel : undefined}
			aria-label={session.catalog.workspace ? 'Ask about your files' : 'Ask a question'}
			placeholder={session.pendingKey
				? `Paste your ${session.pendingKey.display} API key…`
				: session.pendingConnect
					? `Paste the ${session.pendingConnect.id} key…`
					: session.catalog.workspace
						? 'Ask about your files…'
						: 'Ask a question, or type /help'}
			oninput={onInput}
			onkeydown={onKey}
			onfocus={() => (menuOff = false)}
		></textarea>
		{#if session.busy}
			<button class="act stop" title="Stop (Esc)" aria-label="Stop" onclick={() => stop()}>
				<Icon name="stop" fill size={13} />
			</button>
		{:else if value.trim()}
			<button class="act send" aria-label="Send" onclick={() => void submit()}>
				<Icon name="corner-down-left" size={15} />
			</button>
		{/if}
	</div>
</div>

<style>
	.wrap {
		position: relative;
		flex: none;
		background: var(--bg);
		padding: var(--space-3) var(--pad) var(--space-4);
	}
	/* The composer is a distinct raised surface a real input, not a bare line. */
	.field {
		display: flex;
		align-items: flex-end;
		gap: var(--space-2);
		background: var(--bg-raised);
		border: 1px solid var(--border-strong);
		border-radius: var(--radius);
		box-shadow: var(--shadow-sm);
		padding: var(--space-2) var(--space-2) var(--space-2) var(--space-4);
		transition:
			border-color var(--dur-fast) var(--ease),
			box-shadow var(--dur-fast) var(--ease);
	}
	.field:focus-within {
		border-color: var(--link);
		box-shadow: var(--shadow-sm), var(--focus-ring);
	}
	textarea {
		flex: 1;
		min-width: 0;
		resize: none;
		border: none;
		outline: none;
		background: transparent;
		color: var(--text);
		font: inherit;
		line-height: var(--lh);
		max-height: 200px;
		overflow-y: auto;
		padding: var(--space-1) 0;
	}
	textarea:focus-visible {
		box-shadow: none;
	}
	textarea::placeholder {
		color: var(--text-faint);
	}
	/* API-key entry: mask the characters (WebKit Tauri's engine). */
	.field.secret textarea {
		-webkit-text-security: disc;
		font-family: var(--mono);
	}
	/* Trailing action send when there's text, stop while a run is live. */
	.act {
		flex: none;
		display: grid;
		place-items: center;
		width: 26px;
		height: 26px;
		border-radius: var(--radius-sm);
		color: var(--text-faint);
		transition:
			background var(--dur-fast) var(--ease),
			color var(--dur-fast) var(--ease);
	}
	.act.send:hover {
		color: var(--text);
		background: var(--bg-inset);
	}
	.act.stop {
		color: var(--err);
	}
	.act.stop:hover {
		background: color-mix(in srgb, var(--err) 12%, transparent);
	}

	/* completion menu drops up, since the composer sits at the bottom */
	.menu {
		position: absolute;
		bottom: 100%;
		left: var(--pad);
		right: var(--pad);
		margin-bottom: var(--space-2);
		list-style: none;
		padding: var(--space-1);
		background: var(--bg-raised);
		border: 1px solid var(--border);
		border-radius: var(--radius);
		box-shadow: var(--shadow-pop);
		max-height: 246px;
		overflow-y: auto;
		z-index: 20;
	}
	.menu li {
		list-style: none;
	}
	/* rows use the global .rowbtn primitive; only the inner spans are local */
	.menu .val {
		font-family: var(--mono);
		font-size: var(--fs-sm);
	}
	.menu .desc {
		font-size: var(--fs-sm);
		color: var(--text-faint);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.menu .more {
		padding: 4px 8px;
		font-size: var(--fs-xs);
		color: var(--text-faint);
	}
</style>

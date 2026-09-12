<script lang="ts">
	import {
		carriesSecret,
		COMMAND_DESCRIPTIONS,
		completionsFor,
		dispatch,
		openFolder,
		resumeLastFolder,
		steerRun,
		stop
	} from '$lib/commands';
	import { session } from '$lib/session.svelte';
	import { enterUp } from '$lib/motion';
	import Icon from './Icon.svelte';

	let { onafterrun }: { onafterrun?: () => void } = $props();

	let value = $state('');
	let ta: HTMLTextAreaElement;
	// ↑-recall history lives on the active conversation, so each tab has its own.
	// (The composer is hidden on an augment tab, so activeChat is the focused tab.)
	let history = $derived(session.activeChat?.history ?? []);
	let histIx = -1;

	let folderName = $derived(
		session.catalog.workspace?.replace(/[/\\]+$/, '').replace(/^.*[/\\]/, '') ?? ''
	);
	let placeholder = $derived(
		session.pendingKey
			? `Paste your ${session.pendingKey.display} API key…`
			: session.pendingConnect
				? `Paste the ${session.pendingConnect.id} key…`
				: folderName
					? `Ask about ${folderName}…`
					: 'Open a folder to ask, or type /help'
	);

	// --- live-state chips (moved from the retired StatusBar) -------------
	let up = $derived(session.health?.reachable ?? null);
	let rejected = $derived(session.health?.rejected === true);
	let providerId = $derived(session.settings?.provider ?? 'ollama');
	let providerName = $derived(
		session.providers.find((p) => p.id === providerId)?.display ?? providerId
	);
	let hasFolder = $derived(!!session.catalog.workspace);
	let fileCount = $derived(session.catalog.sources.length);
	// provider/model, only once the provider has actually answered. Shows the
	// active tab's model (each tab can pick its own).
	let modelLabel = $derived(up === true && session.model ? `${providerName}/${session.model}` : '');
	// The health chip's text when there's nothing to show yet, or something
	// needs doing. Points at the fix.
	let healthState = $derived.by(() => {
		if (up === null) return 'connecting…';
		if (rejected) return 'key refused — /login';
		if (up === false) return 'offline';
		if (up === true && !session.model) return 'pick a model — /model';
		return '';
	});
	let activityNote = $derived.by(() => {
		if (session.activity) return session.activity;
		if (session.busy) return 'working…';
		if ((up === false || rejected) && providerId !== 'ollama') return providerName;
		if (session.focus) return 'focus mode · /focus to exit';
		return null;
	});

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
		if (!text) return;
		// Mid-run: a plain line (not a command, not a key paste) steers the live
		// answer — cancel and re-ask with it appended. A command or key still
		// waits for the run to end.
		if (session.busy) {
			if (pendingInput || text.startsWith('/')) return;
			if (!carriesSecret(text)) history.unshift(text);
			histIx = -1;
			value = '';
			menuSel = -1;
			queueMicrotask(grow);
			await steerRun(session.ensureChat(), text);
			onafterrun?.();
			return;
		}
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
		// While the menu is open it owns the arrows / Tab / Esc, and Enter
		// only when a row is highlighted; otherwise Enter submits.
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
			// Enter accepts a row only if you arrowed to one; with nothing
			// highlighted it submits what's typed (Tab is for completing).
			// Otherwise `/login xai` with `key` still in the menu could never
			// be sent it kept completing to `/login xai key`.
			if (e.key === 'Enter' && !e.shiftKey && menuSel >= 0) {
				e.preventDefault();
				acceptItem(shown[menuSel]);
				return;
			}
		}

		if (e.key === 'Enter' && !e.shiftKey) {
			e.preventDefault();
			// On the welcome screen (no folder, nothing typed) Enter reopens the
			// last folder if there is one otherwise it's the normal submit.
			if (!value.trim() && !session.catalog.workspace && session.lastFolder) {
				void resumeLastFolder();
				return;
			}
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
		<div class="input-row">
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
				aria-label={folderName ? `Ask about ${folderName}` : 'Ask a question'}
				{placeholder}
				oninput={onInput}
				onkeydown={onKey}
				onfocus={() => (menuOff = false)}
			></textarea>
			{#if session.busy && value.trim() && !pendingInput && !value.startsWith('/')}
				<button
					class="act send"
					title="Cancel and re-ask with this (Enter)"
					aria-label="Cancel and re-ask with this"
					onclick={() => void submit()}
				>
					<Icon name="corner-down-left" size={15} />
				</button>
			{:else if session.busy}
				<button class="act stop" title="Stop (Esc)" aria-label="Stop" onclick={() => stop()}>
					<Icon name="stop" fill size={13} />
				</button>
			{:else if value.trim()}
				<button class="act send" aria-label="Send" onclick={() => void submit()}>
					<Icon name="corner-down-left" size={15} />
				</button>
			{/if}
		</div>
		{#if !session.focus}
			<div class="chips">
				<span class="pill ghost chip">
					{#if up === true}
						<Icon name="asterisk" size={11} />
					{:else}
						<span class="dot" class:down={up === false} aria-hidden="true"></span>
					{/if}
					{modelLabel || healthState}
				</span>
				{#if activityNote}
					<span class="pill ghost chip">
						{#if session.busy}<span class="thinking" aria-hidden="true"></span>{/if}
						{activityNote}
					</span>
				{/if}
			</div>
		{/if}
	</div>
	{#if !session.focus}
		<div class="below">
			<button class="below-btn" type="button" onclick={() => void openFolder()}>
				<Icon name="folder" size={12} />
				{hasFolder ? `${folderName} · ${fileCount} file${fileCount === 1 ? '' : 's'}` : 'choose a folder'}
			</button>
		</div>
	{/if}
</div>

<style>
	.wrap {
		position: relative;
		flex: none;
		padding: var(--space-1) var(--pad) var(--space-2);
	}
	/* Live session state, moved here from the retired StatusBar so it reads
	   as part of the composer instead of a separate strip. */
	.chips {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2);
		margin-top: var(--space-1);
		padding-top: var(--space-1);
		border-top: 1px solid var(--border);
	}
	.chip {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 5px 10px;
		font-size: var(--fs-sm);
		white-space: nowrap;
	}
	.chip .dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--text-faint);
		flex: none;
		box-shadow: 0 0 0 3px color-mix(in srgb, var(--text-faint) 18%, transparent);
	}
	.chip .dot.down {
		background: var(--err);
		box-shadow: 0 0 0 3px color-mix(in srgb, var(--err) 20%, transparent);
	}
	/* Outlined but unfilled it sits on the footer surface, no card colour.
	   Softly rounded; stays sane when the textarea grows tall. */
	.field {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		background: var(--bg-raised);
		border: 1px solid var(--border);
		border-radius: 18px;
		box-shadow: var(--shadow-sm);
		padding: var(--space-2) var(--space-2) var(--space-2) var(--space-4);
		transition:
			border-color var(--dur-fast) var(--ease),
			box-shadow var(--dur-fast) var(--ease);
	}
	.field:focus-within {
		border-color: var(--link);
		box-shadow: var(--focus-ring);
	}
	.input-row {
		display: flex;
		align-items: flex-end;
		gap: var(--space-2);
	}
	.below {
		display: flex;
		margin-top: var(--space-2);
	}
	.below-btn {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		color: var(--text-faint);
		font-size: var(--fs-xs);
		padding: 3px 6px;
		border-radius: var(--radius-chip);
		transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
	}
	.below-btn:hover {
		background: var(--bg-inset);
		color: var(--text-dim);
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
		width: 28px;
		height: 28px;
		border-radius: 50%;
		color: var(--text-faint);
		transition:
			background var(--dur-fast) var(--ease),
			color var(--dur-fast) var(--ease);
	}
	.act.send {
		background: var(--bg-inset);
		color: var(--text-dim);
	}
	.act.send:hover {
		color: var(--bg-raised);
		background: var(--text);
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

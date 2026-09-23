<script lang="ts">
	import {
		carriesSecret,
		COMMAND_DESCRIPTIONS,
		completionsFor,
		dispatch,
		resumeLastFolder,
		selectModel,
		steerRun,
		stop
	} from '$lib/commands';
	import { session } from '$lib/session.svelte';
	import { enterUp } from '$lib/motion';
	import type { ContextReference, SourceInfo } from '$lib/types';
	import Icon from './Icon.svelte';
	import ProviderIcon from './ProviderIcon.svelte';

	let { onafterrun }: { onafterrun?: () => void } = $props();

	let value = $state('');
	let ta: HTMLTextAreaElement;
	let contextInput = $state<HTMLInputElement>();
	let modelInput = $state<HTMLInputElement>();
	let wrapEl = $state<HTMLDivElement>();
	// ↑-recall history lives on the active conversation, so each tab has its own.
	let history = $derived(session.activeChat?.history ?? []);
	let histIx = -1;

	let folderName = $derived(
		session.catalog.workspace?.replace(/[/\\]+$/, '').replace(/^.*[/\\]/, '') ?? ''
	);
	let placeholder = $derived(
		session.pendingKey
			? `Paste your ${session.pendingKey.display} API key…`
			: folderName
				? `Ask about ${folderName}…`
				: 'Choose a folder to ask about…'
	);

	// --- completion menu -------------------------------------------------
	const MAX_ITEMS = 8;
	let menuSel = $state(-1); // -1 = nothing highlighted; Enter still submits
	let menuOff = $state(false); // dismissed with Esc until the text changes
	let contextOpen = $state(false);
	let modeOpen = $state(false);
	let modelOpen = $state(false);
	let contextQuery = $state('');
	let modelQuery = $state('');
	let mode = $derived(session.activeChat?.mode ?? 'ask');
	let contextRefs = $derived(session.activeChat?.contextRefs ?? []);
	let providerId = $derived(session.settings?.provider ?? '');
	let providerName = $derived(
		(session.providers.find((provider) => provider.id === providerId)?.display ?? providerId) || 'Provider'
	);
	let currentModel = $derived(session.model.trim());
	let modelOptions = $derived.by(() => {
		const available = new Set(session.health?.models ?? []);
		if (currentModel) available.add(currentModel);
		const query = modelQuery.trim().toLowerCase();
		return [...available]
			.filter((model) => !query || model.toLowerCase().includes(query))
			.sort((a, b) => a.localeCompare(b));
	});
	let contextSources = $derived.by((): SourceInfo[] => {
		const q = contextQuery.trim().toLowerCase();
		return session.catalog.sources
			.filter((source) => !q || `${source.name} ${source.path} ${source.kind} ${source.synopsis ?? ''}`.toLowerCase().includes(q))
			.slice(0, 8);
	});
	let contextColumns = $derived.by(() => {
		const q = contextQuery.trim().toLowerCase();
		return session.catalog.sources
			.flatMap((source) => (source.columns ?? []).map((column) => ({ source, column })))
			.filter(({ source, column }) =>
				!q || `${source.name} ${column.name} ${column.type}`.toLowerCase().includes(q)
			)
			.slice(0, 8);
	});
	$effect(() => {
		if (contextOpen) queueMicrotask(() => contextInput?.focus());
		if (modelOpen) queueMicrotask(() => modelInput?.focus());
	});

	let pendingInput = $derived(!!session.pendingKey);
	// `session.busy` alone isn't specific enough to mean "an answer is
	// streaming, steering it makes sense" -- it's also true while a folder
	// is still loading (openFolder reuses it for progress feedback), which
	// has no answer in flight to steer. Only ask()'s pending assistant
	// placeholder means there's actually something to steer.
	let answering = $derived(session.activeChat?.messages.at(-1)?.pending === true);
	let items = $derived(menuOff || pendingInput ? [] : completionsFor(value));
	let shown = $derived(items.slice(0, MAX_ITEMS));
	let menuOpen = $derived(shown.length > 0);

	function describe(item: string): string {
		if (item.startsWith('/')) return COMMAND_DESCRIPTIONS[item] ?? '';
		const p = session.providers.find((x) => x.id === item);
		if (p) return 'sign in with an API key';
		return '';
	}

	function grow() {
		if (!ta) return;
		ta.style.height = 'auto';
		ta.style.height = Math.min(ta.scrollHeight, 200) + 'px';
	}

	function onInput() {
		if (!pendingInput && value.endsWith('@')) {
			value = value.slice(0, -1);
			contextOpen = true;
			modeOpen = false;
			contextQuery = '';
		}
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
		contextOpen = false;
		modeOpen = false;
		// Mid-run: a plain line (not a command, not a key paste) steers the live
		// answer — cancel and re-ask with it appended. A command or key still
		// waits for the run to end.
		if (session.busy) {
			// Busy but nothing is actually answering (e.g. a folder is still
			// being read after auto-mounting a reopened conversation) -- there's
			// no run to steer and no workspace ready yet either; ignore the
			// send rather than firing early against a folder that hasn't
			// finished opening.
			if (!answering) return;
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
		// A real question with no folder open otherwise goes to the model with
		// no tools at all, and a small model fills the gap by guessing or
		// referencing the conversation as if a folder were still open, instead
		// of saying so. If we know the last folder, reopen it first rather
		// than making the user type /open themselves; a slash command still
		// goes straight through (e.g. /open <a different folder>).
		if (!session.catalog.workspace && !text.startsWith('/') && session.lastFolder) {
			await resumeLastFolder();
		}
		await dispatch(text);
		onafterrun?.();
	}

	function onKey(e: KeyboardEvent) {
		if (contextOpen && e.key === 'Escape') {
			contextOpen = false;
			contextQuery = '';
			e.preventDefault();
			e.stopPropagation();
			return;
		}
		if (modeOpen && e.key === 'Escape') {
			modeOpen = false;
			e.preventDefault();
			e.stopPropagation();
			return;
		}
		if (modelOpen && e.key === 'Escape') {
			modelOpen = false;
			modelQuery = '';
			e.preventDefault();
			e.stopPropagation();
			return;
		}
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

	function sourceDetail(source: SourceInfo): string {
		const root = session.catalog.workspace?.replace(/[/\\]+$/, '');
		const path = root && (source.path.startsWith(root + '/') || source.path.startsWith(root + '\\'))
			? source.path.slice(root.length + 1).replace(/\\/g, '/')
			: source.path;
		return source.view ? `${path} · ${source.view}` : path;
	}

	function addSource(source: SourceInfo): void {
		session.addContextReference({ kind: 'source', key: source.path, label: source.name, detail: sourceDetail(source) });
		contextOpen = false;
		contextQuery = '';
	}

	function addColumn(source: SourceInfo, name: string, type: string): void {
		session.addContextReference({
			kind: 'column',
			key: `${source.path}:${name}`,
			label: `${source.name}.${name}`,
			detail: `${type} field`
		});
		contextOpen = false;
		contextQuery = '';
	}

	function inspectSource(source: SourceInfo): void {
		contextOpen = false;
		session.openInspector({ kind: 'source', path: source.path });
	}

	function removeReference(ref: ContextReference): void {
		session.removeContextReference(ref.kind, ref.key);
	}

	function chooseMode(next: 'ask' | 'inspect'): void {
		session.setAskMode(next);
		modeOpen = false;
	}

	async function chooseModel(next: string): Promise<void> {
		if (!(await selectModel(next))) return;
		modelOpen = false;
		modelQuery = '';
		ta?.focus();
	}

	function onWindowClick(e: MouseEvent): void {
		if (!wrapEl?.contains(e.target as Node)) {
			contextOpen = false;
			modeOpen = false;
			modelOpen = false;
			modelQuery = '';
		}
	}
</script>

<svelte:window onclick={onWindowClick} />

<div class="wrap" bind:this={wrapEl}>
	{#if contextOpen}
		<div class="context-menu" transition:enterUp>
			<div class="context-search">
				<Icon name="search" size={16} />
				<input bind:this={contextInput} bind:value={contextQuery} placeholder="Find a source or field…" spellcheck="false" />
				<button type="button" aria-label="Close context picker" onclick={() => (contextOpen = false)}><Icon name="x" size={16} /></button>
			</div>
			{#if contextSources.length}
				<p class="context-heading">Sources</p>
				<div class="context-list">
					{#each contextSources as source (source.path)}
						<div class="context-item">
							<button class="context-main" type="button" onclick={() => addSource(source)}>
								<span class="context-icon"><Icon name={source.view ? 'table' : 'file'} size={16} /></span>
								<span class="context-copy"><strong>{source.name}</strong><small>{sourceDetail(source)}</small></span>
							</button>
							<button class="context-inspect" type="button" aria-label={`View details for ${source.name}`} title="View source details" onclick={() => inspectSource(source)}>
								<Icon name="info" size={16} />
							</button>
						</div>
					{/each}
				</div>
			{/if}
			{#if contextColumns.length}
				<p class="context-heading">Fields</p>
				<div class="context-list compact-list">
					{#each contextColumns as item (item.source.path + ':' + item.column.name)}
						<button class="context-field" type="button" onclick={() => addColumn(item.source, item.column.name, item.column.type)}>
							<span class="context-icon"><Icon name="table" size={16} /></span>
							<span class="context-copy"><strong>{item.source.name}.{item.column.name}</strong><small>{item.column.type}</small></span>
						</button>
					{/each}
				</div>
			{/if}
			{#if !contextSources.length && !contextColumns.length}
				<p class="context-empty">No matching sources or fields.</p>
			{/if}
			<p class="context-hint">Add a source or field to guide your next question.</p>
		</div>
	{/if}
	{#if menuOpen && !contextOpen && !modeOpen && !modelOpen}
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
		{#if !pendingInput}
			<div class="context-row">
				{#each contextRefs as ref (ref.kind + ':' + ref.key)}
					<span class="ref-pill" title={ref.detail ?? ref.label}>
							<Icon name={ref.kind === 'source' ? 'file' : 'table'} size={12} />
						<span>{ref.label}</span>
						<button type="button" aria-label={`Remove ${ref.label} from this question`} onclick={() => removeReference(ref)}><Icon name="x" size={12} /></button>
					</span>
				{/each}
				<button
					class="context-add"
					type="button"
					aria-label="Add a source or field to this question"
					title="Add a source or field to this question"
					aria-expanded={contextOpen}
					onclick={() => { contextOpen = !contextOpen; modeOpen = false; modelOpen = false; modelQuery = ''; }}
				>
					<Icon name="plus" size={16} /> Add source{#if contextRefs.length} · {contextRefs.length}{/if}
				</button>
			</div>
		{/if}
		<textarea
			bind:this={ta}
			bind:value
			rows="1"
			spellcheck="false"
			autocapitalize="off"
			autocomplete="off"
			role="combobox"
			aria-expanded={menuOpen && !contextOpen && !modeOpen && !modelOpen}
			aria-controls="composer-completions"
			aria-autocomplete="list"
			aria-activedescendant={menuOpen && !contextOpen && !modeOpen && !modelOpen && menuSel >= 0 ? 'composer-opt-' + menuSel : undefined}
			aria-label={folderName ? `Ask about ${folderName}` : 'Ask a question'}
			{placeholder}
			oninput={onInput}
			onkeydown={onKey}
			onfocus={() => (menuOff = false)}
		></textarea>
		<div class="bottom-row">
			{#if !session.focus}
				<div class="mode-wrap">
					<button class="mode-trigger" type="button" aria-expanded={modeOpen} onclick={() => { modeOpen = !modeOpen; contextOpen = false; modelOpen = false; modelQuery = ''; }}>
						<span class="mode-mark" class:inspect={mode === 'inspect'}></span>
						{mode === 'inspect' ? 'Check data' : 'Ask'}
						<Icon name="chevron-right" size={12} />
					</button>
					{#if modeOpen}
						<div class="mode-menu">
							<button class:chosen={mode === 'ask'} type="button" onclick={() => chooseMode('ask')}>
								<span class="mode-mark"></span><span><strong>Ask</strong><small>Answer from the workspace.</small></span>
							</button>
							<button class:chosen={mode === 'inspect'} type="button" onclick={() => chooseMode('inspect')}>
								<span class="mode-mark inspect"></span><span><strong>Check data</strong><small>Start with the files and show the checks.</small></span>
							</button>
						</div>
					{/if}
				</div>
				{#if !pendingInput}
					<div class="model-wrap">
						<button
							class="model-trigger"
							type="button"
							aria-expanded={modelOpen}
							aria-haspopup="dialog"
							title={currentModel ? `Model: ${currentModel}` : 'Choose a model'}
							onclick={() => { modelOpen = !modelOpen; contextOpen = false; modeOpen = false; if (!modelOpen) modelQuery = ''; }}
						>
							{#if providerId}<ProviderIcon providerId={providerId} size={14} />{/if}
							<span class="model-name" class:empty={!currentModel}>{currentModel || 'Choose model'}</span>
							<Icon name="chevron-right" size={12} />
						</button>
						{#if modelOpen}
							<div class="model-menu" role="dialog" aria-label="Choose a model" transition:enterUp>
								<div class="model-menu-head">
									<strong>Model</strong>
									<small>{providerName}</small>
								</div>
								<label class="model-search">
									<Icon name="search" size={14} />
									<span class="sr-only">Find a model</span>
									<input
										bind:this={modelInput}
										bind:value={modelQuery}
										placeholder="Find a model…"
										spellcheck="false"
										onkeydown={(event) => {
											if (event.key === 'Escape') {
												modelOpen = false;
												modelQuery = '';
												event.preventDefault();
											}
										}}
									/>
								</label>
								{#if modelOptions.length}
									<div class="model-list" role="listbox" aria-label="Available models">
										{#each modelOptions as modelOption (modelOption)}
											<button
												class="model-option"
												class:selected={modelOption === currentModel}
												type="button"
												role="option"
												aria-selected={modelOption === currentModel}
												onclick={() => void chooseModel(modelOption)}
											>
												<span>{modelOption}</span>
												{#if modelOption === currentModel}<Icon name="check" size={14} />{/if}
											</button>
										{/each}
									</div>
								{:else}
									<p class="model-empty">No models available from {providerName}. Refresh the connection in Settings.</p>
								{/if}
								<button class="model-settings" type="button" onclick={() => { modelOpen = false; session.setWorkspaceView('settings'); }}>
									<Icon name="settings" size={14} /> Provider settings
								</button>
							</div>
						{/if}
					</div>
				{/if}
			{/if}
			{#if answering && value.trim() && !pendingInput && !value.startsWith('/')}
				<button
					class="act send"
					title="Cancel and re-ask with this (Enter)"
					aria-label="Cancel and re-ask with this"
					onclick={() => void submit()}
				>
						<Icon name="corner-down-left" size={16} />
				</button>
			{:else if session.busy}
				<button class="act stop" title="Stop (Esc)" aria-label="Stop" onclick={() => stop()}>
					<Icon name="stop" fill size={16} />
				</button>
			{:else if value.trim()}
				<button class="act send" aria-label="Send" onclick={() => void submit()}>
					<Icon name="corner-down-left" size={16} />
				</button>
			{/if}
		</div>
	</div>
</div>

<style>
	.wrap {
		position: relative;
		flex: 0 1 var(--content-max);
		width: 100%;
		max-width: var(--content-max);
		margin-inline: auto;
		padding: var(--space-1) var(--pad) var(--space-2);
	}
	.context-row {
		display: flex;
		align-items: center;
		gap: 6px;
		min-width: 0;
		min-height: 18px;
		overflow-x: auto;
		scrollbar-width: none;
	}
	.context-row::-webkit-scrollbar {
		display: none;
	}
	.ref-pill {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		max-width: 210px;
		padding: 4px 5px 4px 8px;
		border: 1px solid var(--border-strong);
		border-radius: var(--radius-chip);
		background: var(--bg-inset);
		color: var(--text-dim);
		font-size: var(--fs-xs);
		white-space: nowrap;
	}
	.ref-pill > span {
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.ref-pill :global(svg) {
		color: var(--text-faint);
	}
	.ref-pill button {
		display: grid;
		place-items: center;
		width: 16px;
		height: 16px;
		border-radius: 3px;
		color: var(--text-faint);
	}
	.ref-pill button:hover {
		background: var(--bg-inset);
		color: var(--text);
	}
	.context-add {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		flex: none;
		padding: 3px 6px;
		border-radius: var(--radius-chip);
		color: var(--text-faint);
		font-size: var(--fs-xs);
		white-space: nowrap;
	}
	.context-add:hover,
	.context-add[aria-expanded='true'] {
		background: var(--bg-inset);
		color: var(--text-dim);
	}
	.mode-wrap {
		position: relative;
		flex: none;
	}
	.mode-trigger {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 4px 6px;
		border-radius: var(--radius-chip);
		color: var(--text-dim);
		font-size: var(--fs-xs);
		font-weight: 600;
		white-space: nowrap;
	}
	.mode-trigger:hover,
	.mode-trigger[aria-expanded='true'] {
		background: var(--bg-inset);
		color: var(--text);
	}
	.mode-trigger :global(svg) {
		transform: rotate(90deg);
		color: var(--text-faint);
	}
	.mode-mark {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--text-faint);
	}
	.mode-mark.inspect {
		background: var(--brand);
	}
	.mode-menu {
		position: absolute;
		bottom: calc(100% + 8px);
		left: -6px;
		z-index: 22;
		width: 224px;
		padding: 5px;
		border: 1px solid var(--border);
		border-radius: var(--radius);
		background: var(--bg-raised);
		box-shadow: var(--shadow-pop);
	}
	.mode-menu button {
		display: flex;
		align-items: flex-start;
		gap: 9px;
		width: 100%;
		padding: 8px;
		border-radius: var(--radius-chip);
		text-align: left;
	}
	.mode-menu button:hover,
	.mode-menu button.chosen {
		background: var(--bg-inset);
	}
	.mode-menu button > span:last-child {
		display: flex;
		flex-direction: column;
		gap: 1px;
	}
	.mode-menu strong {
		font-size: var(--fs-xs);
		font-weight: 600;
	}
	.mode-menu small {
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.model-wrap {
		position: relative;
		min-width: 0;
		flex: none;
	}
	.model-trigger {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		min-width: 0;
		max-width: min(260px, 32vw);
		padding: 4px 6px;
		border-radius: var(--radius-chip);
		color: var(--text-dim);
		font-size: var(--fs-xs);
		font-weight: 500;
		white-space: nowrap;
	}
	.model-trigger:hover,
	.model-trigger[aria-expanded='true'] {
		background: var(--bg-inset);
		color: var(--text);
	}
	.model-trigger > :global(svg) {
		transform: rotate(90deg);
		color: var(--text-faint);
	}
	.model-name {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.model-name.empty {
		color: var(--text-faint);
	}
	.model-menu {
		position: absolute;
		bottom: calc(100% + 8px);
		left: -6px;
		z-index: 22;
		width: min(300px, calc(100vw - 48px));
		padding: var(--space-2);
		border: 1px solid var(--border);
		border-radius: var(--radius);
		background: var(--bg-raised);
		box-shadow: var(--shadow-pop);
	}
	.model-menu-head {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: var(--space-2);
		padding: 2px var(--space-2) var(--space-2);
	}
	.model-menu-head strong {
		font-size: var(--fs-sm);
		font-weight: 600;
	}
	.model-menu-head small {
		max-width: 16ch;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.model-search {
		display: flex;
		align-items: center;
		gap: 7px;
		margin-bottom: var(--space-2);
		padding: 6px 8px;
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		color: var(--text-faint);
	}
	.model-search:focus-within {
		border-color: var(--link);
		box-shadow: var(--focus-ring);
	}
	.model-search input {
		min-width: 0;
		width: 100%;
		border: 0;
		outline: 0;
		background: transparent;
		color: var(--text);
		font: inherit;
		font-size: var(--fs-sm);
	}
	.model-search input::placeholder {
		color: var(--text-faint);
	}
	.model-list {
		max-height: min(280px, 38vh);
		overflow: auto;
	}
	.model-option {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-2);
		width: 100%;
		padding: 7px 8px;
		border-radius: var(--radius-chip);
		text-align: left;
		font-family: var(--mono);
		font-size: var(--fs-xs);
	}
	.model-option:hover,
	.model-option.selected {
		background: var(--bg-inset);
	}
	.model-option span {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.model-option :global(svg) {
		flex: none;
		color: var(--brand);
	}
	.model-empty {
		margin: var(--space-3) var(--space-2);
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.model-settings {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		width: 100%;
		margin-top: var(--space-2);
		padding: var(--space-2);
		border-top: 1px solid var(--border);
		color: var(--text-faint);
		font-size: var(--fs-xs);
		text-align: left;
	}
	.model-settings:hover {
		color: var(--text);
	}
	.context-menu {
		position: absolute;
		bottom: calc(100% + 8px);
		left: var(--pad);
		z-index: 21;
		width: min(420px, calc(100% - (var(--pad) * 2)));
		padding: var(--space-2);
		max-height: min(70vh, 520px);
		overflow: auto;
		border: 1px solid var(--border);
		border-radius: var(--radius);
		background: var(--bg-raised);
		box-shadow: var(--shadow-pop);
	}
	.context-search {
		display: flex;
		align-items: center;
		gap: 7px;
		padding: 7px 8px;
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		color: var(--text-faint);
	}
	.context-search:focus-within {
		border-color: var(--link);
		box-shadow: var(--focus-ring);
	}
	.context-search input {
		min-width: 0;
		width: 100%;
		border: 0;
		outline: 0;
		background: transparent;
		color: var(--text);
		font: inherit;
		font-size: var(--fs-sm);
	}
	.context-search input::placeholder {
		color: var(--text-faint);
	}
	.context-search > button {
		display: grid;
		place-items: center;
		width: 20px;
		height: 20px;
		flex: none;
		color: var(--text-faint);
	}
	.context-heading {
		margin: var(--space-3) var(--space-2) var(--space-1);
		color: var(--text-faint);
		font-size: var(--fs-xs);
		font-weight: 650;
		letter-spacing: 0.01em;
	}
	.context-list {
		max-height: 220px;
		overflow: auto;
	}
	.compact-list {
		max-height: 150px;
	}
	.context-item {
		display: flex;
		align-items: center;
		gap: 2px;
		border-radius: var(--radius-chip);
	}
	.context-item:hover,
	.context-item:focus-within {
		background: var(--bg-inset);
	}
	.context-main {
		min-width: 0;
		display: flex;
		align-items: center;
		gap: 9px;
		flex: 1;
		padding: 7px 6px 7px 8px;
		text-align: left;
	}
	.context-field {
		width: 100%;
		display: flex;
		align-items: center;
		gap: 9px;
		padding: 6px 8px;
		border-radius: var(--radius-chip);
		text-align: left;
	}
	.context-field:hover {
		background: var(--bg-inset);
	}
	.context-icon {
		display: grid;
		place-items: center;
		width: 24px;
		height: 24px;
		flex: none;
		border-radius: var(--radius-chip);
		background: var(--bg-inset);
		color: var(--text-faint);
	}
	.context-copy {
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 1px;
	}
	.context-copy strong,
	.context-copy small {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.context-copy strong {
		font-size: var(--fs-xs);
		font-weight: 600;
	}
	.context-copy small {
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.context-inspect {
		display: grid;
		place-items: center;
		width: 26px;
		height: 26px;
		margin-right: 4px;
		border-radius: var(--radius-chip);
		color: var(--text-faint);
	}
	.context-inspect:hover {
		background: var(--bg-raised);
		color: var(--text);
	}
	.context-empty,
	.context-hint {
		margin: var(--space-3) var(--space-2) var(--space-2);
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.context-hint {
		padding-top: var(--space-2);
		border-top: 1px solid var(--border);
	}
	/* Outlined but unfilled it sits on the footer surface, no card colour.
	   Softly rounded; stays sane when the textarea grows tall. */
	.field {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		background: var(--bg-raised);
		border: 1px solid var(--border);
		border-radius: 18px;
		box-shadow: var(--shadow-sm);
		padding: var(--space-3) var(--space-3) var(--space-2) var(--space-4);
		transition:
			border-color var(--dur-fast) var(--ease),
			box-shadow var(--dur-fast) var(--ease);
	}
	.field:focus-within {
		border-color: var(--link);
		box-shadow: var(--focus-ring);
	}
	.bottom-row {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		min-height: 28px;
	}
	textarea {
		width: 100%;
		resize: none;
		border: none;
		outline: none;
		background: transparent;
		color: var(--text);
		font: inherit;
		font-size: var(--fs-lg);
		line-height: 1.5;
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
		margin-left: auto;
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
		background: var(--brand);
		color: var(--on-brand);
	}
	.act.send:hover {
		color: var(--on-brand);
		background: var(--brand);
		filter: brightness(0.9);
	}
	.act.stop {
		color: var(--err);
	}
	.act.stop:hover {
		background: var(--bg-inset);
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

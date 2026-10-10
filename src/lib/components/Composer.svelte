<script lang="ts">
	import {
		carriesSecret,
		COMMAND_DESCRIPTIONS,
		completionsFor,
		dispatch,
		openRepository,
		selectModel,
		steerRun,
		stop
	} from '$lib/commands';
	import { Button } from '$lib/components/ui/button';
	import { DropdownMenu } from '$lib/components/ui/dropdown-menu';
	import { Input } from '$lib/components/ui/input';
	import { Textarea } from '$lib/components/ui/textarea';
	import { session } from '$lib/session.svelte';
	import { enterUp } from '$lib/motion';
	import type { ContextReference, SourceInfo } from '$lib/types';
	import Icon from './Icon.svelte';
	import Logo from './Logo.svelte';
	import ProviderIcon from './ProviderIcon.svelte';

	let { onafterrun }: { onafterrun?: () => void } = $props();

	let value = $state('');
	let ta = $state<HTMLTextAreaElement>();
	let contextInput = $state<HTMLInputElement>();
	let modelInput = $state<HTMLInputElement>();
	let wrapEl = $state<HTMLDivElement>();
	// ↑-recall history belongs to the active conversation.
	let history = $derived(session.activeChat?.history ?? []);
	let histIx = -1;

	let folderName = $derived(
		session.catalog.workspace?.replace(/[/\\]+$/, '').replace(/^.*[/\\]/, '') ?? ''
	);
	let unavailableWorkspacePath = $derived.by(() => {
		const path = session.activeChat?.workspaceScope;
		const workspace = path ? session.workspaceAt(path) : null;
		return path && (!workspace || workspace.historyOnly)
			? path
			: null;
	});
	let unavailableWorkspaceName = $derived(
		unavailableWorkspacePath?.replace(/[/\\]+$/, '').replace(/^.*[/\\]/, '') ?? ''
	);
	let placeholder = $derived(
		session.pendingKey
			? `Paste your ${session.pendingKey.display} API key…`
			: unavailableWorkspacePath
				? 'Reopen this workspace to continue analysis…'
				: folderName
					? 'Ask a question…'
					: 'Ask Fella a question…'
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
	let pendingClarification = $derived.by(() => {
		const latest = session.activeChat?.messages.at(-1);
		if (latest?.role !== 'assistant' || latest.pending) return null;
		const request = latest.answer?.clarification;
		const turnId = latest.answer?.turn_id;
		return request && turnId ? { request, turnId } : null;
	});
	let clarificationOther = $state('');

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
	// Mount progress is separate from conversation activity. Only ask()'s
	// pending assistant placeholder means there is an answer to steer.
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
		// Do not start or steer an analysis across a workspace snapshot change.
		if (session.mountProgress) return;
		contextOpen = false;
		modeOpen = false;
		// Mid-run: a plain line (not a command, not a key paste) steers the live
		// answer — cancel and re-ask with it appended. A command or key still
		// waits for the run to end.
		if (session.busy) {
			// Busy but nothing is actually answering -- ignore the send.
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
		// A typed question is never a request to reopen a previous repository.
		// The empty-composer Enter shortcut and explicit Reopen button remain
		// the deliberate ways to restore the last folder.
		await dispatch(text);
		onafterrun?.();
	}

	async function submitClarificationResponse(raw: string) {
		const pending = pendingClarification;
		const response = raw.trim();
		if (!pending || !response || session.busy || session.mountProgress) return;
		if (!carriesSecret(response)) history.unshift(response);
		histIx = -1;
		clarificationOther = '';
		await dispatch(response, pending.turnId);
		onafterrun?.();
	}

	function submitOtherClarification(event: SubmitEvent) {
		event.preventDefault();
		void submitClarificationResponse(clarificationOther);
	}

	function onKey(e: KeyboardEvent) {
		if (contextOpen && e.key === 'Escape') {
			contextOpen = false;
			contextQuery = '';
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

	function removeReference(ref: ContextReference): void {
		session.removeContextReference(ref.kind, ref.key);
	}

	function chooseMode(next: 'ask' | 'inspect'): void {
		session.setAskMode(next);
		modeOpen = false;
	}

	function setModeMenuOpen(open: boolean): void {
		modeOpen = open;
		if (!open) return;
		contextOpen = false;
		modelOpen = false;
		modelQuery = '';
	}

	function selectMode(value: string): void {
		if (value === 'ask' || value === 'inspect') chooseMode(value);
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
				<Input
					class="context-search-input"
					bind:ref={contextInput}
					bind:value={contextQuery}
					aria-label="Find a source or field"
					placeholder="Find a source or field…"
					spellcheck="false"
				/>
				<Button variant="ghost" size="icon" class="context-close" aria-label="Close context picker" onclick={() => (contextOpen = false)}>
					<Icon name="x" size={16} />
				</Button>
			</div>
			{#if contextSources.length}
				<p class="context-heading">Sources</p>
				<div class="context-list">
					{#each contextSources as source (source.path)}
						<div class="context-item">
							<Button variant="ghost" class="context-main" type="button" onclick={() => addSource(source)}>
								<span class="context-icon"><Icon name={source.view ? 'table' : 'file'} size={16} /></span>
								<span class="context-copy"><strong>{source.name}</strong><small>{sourceDetail(source)}</small></span>
							</Button>
						</div>
					{/each}
				</div>
			{/if}
			{#if contextColumns.length}
				<p class="context-heading">Fields</p>
				<div class="context-list compact-list">
					{#each contextColumns as item (item.source.path + ':' + item.column.name)}
						<Button variant="ghost" class="context-field" type="button" onclick={() => addColumn(item.source, item.column.name, item.column.type)}>
							<span class="context-icon"><Icon name="table" size={16} /></span>
							<span class="context-copy"><strong>{item.source.name}.{item.column.name}</strong><small>{item.column.type}</small></span>
						</Button>
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
					<Button
						variant="ghost"
						class="completion-option"
						type="button"
						tabindex={-1}
						role="option"
						id={'composer-opt-' + i}
						aria-selected={i === menuSel}
						onmousedown={(e) => {
							e.preventDefault();
							acceptItem(item);
						}}
						onmouseenter={() => (menuSel = i)}
					>
						<span class="val">{item}</span>
						{#if describe(item)}<span class="desc">{describe(item)}</span>{/if}
					</Button>
				</li>
			{/each}
			{#if items.length > shown.length}
				<li class="more">+{items.length - shown.length} more keep typing</li>
			{/if}
		</ul>
	{/if}

	{#if pendingClarification && !pendingInput}
		<div class="field clarification-field" role="region" aria-label="Clarification response">
			<h2 class="clarification-question">{pendingClarification.request.question}</h2>
			{#if pendingClarification.request.reason}
				<p class="clarification-reason">{pendingClarification.request.reason}</p>
			{/if}
			{#if pendingClarification.request.options.length}
				<div class="clarification-options" role="group" aria-label="Suggested answers">
					{#each pendingClarification.request.options as option, index (option)}
						<Button
							variant="ghost"
							class="clarification-option"
							type="button"
							disabled={session.busy}
							onclick={() => void submitClarificationResponse(option)}
						>
							<span class="clarification-index">{index + 1}</span>
							<span>{option}</span>
						</Button>
					{/each}
				</div>
			{/if}
			<form class="clarification-other" onsubmit={submitOtherClarification}>
				<label for="clarification-other-input">Other</label>
				<div class="clarification-other-row">
					<Textarea
						class="clarification-input"
						id="clarification-other-input"
						bind:value={clarificationOther}
						aria-label="Other interpretation"
						placeholder="Describe what you mean…"
						maxlength={500}
						rows={2}
						disabled={session.busy}
					></Textarea>
					<Button
						variant="default"
						size="icon"
						class="clarification-send"
						type="submit"
						disabled={!clarificationOther.trim() || session.busy}
						aria-label="Continue with this interpretation"
					>
						<Icon name="corner-down-left" size={16} />
					</Button>
				</div>
			</form>
		</div>
	{:else}
	<div class="field" class:secret={pendingInput}>
		{#if unavailableWorkspacePath && !pendingInput}
			<div class="workspace-unavailable-note" role="status">
				<span>
					{session.historyOnlyRepositoryPaths.includes(unavailableWorkspacePath)
						? `The original folder for this conversation is unavailable. Reopen ${unavailableWorkspaceName} to continue.`
						: `This workspace is closed. Reopen ${unavailableWorkspaceName} to continue.`}
				</span>
				<Button variant="ghost" size="sm" class="workspace-reopen" onclick={() => void openRepository(unavailableWorkspacePath!, { reportFailure: true })}>Open workspace</Button>
			</div>
		{/if}
		{#if !pendingInput}
			<div class="context-row">
				{#each contextRefs as ref (ref.kind + ':' + ref.key)}
					<span class="ref-pill" title={ref.detail ?? ref.label}>
							<Icon name={ref.kind === 'source' ? 'file' : 'table'} size={12} />
						<span>{ref.label}</span>
						<Button variant="ghost" size="icon" class="remove-ref" aria-label={`Remove ${ref.label} from this question`} onclick={() => removeReference(ref)}>
							<Icon name="x" size={12} />
						</Button>
					</span>
				{/each}
				<Button
					variant="ghost"
					size="sm"
					class="context-add"
					aria-label="Add a source or field to this question"
					title="Add a source or field to this question"
					aria-expanded={contextOpen}
					onclick={() => { contextOpen = !contextOpen; modeOpen = false; modelOpen = false; modelQuery = ''; }}
				>
					<Icon name="plus" size={16} /> Add source{#if contextRefs.length} · {contextRefs.length}{/if}
				</Button>
			</div>
		{/if}
		<Textarea
			class="composer-main-textarea"
			bind:ref={ta}
			bind:value
			rows={1}
			spellcheck="false"
			autocapitalize="off"
			autocomplete="off"
			role="combobox"
			aria-expanded={menuOpen && !contextOpen && !modeOpen && !modelOpen}
			aria-controls="composer-completions"
			aria-autocomplete="list"
			aria-activedescendant={menuOpen && !contextOpen && !modeOpen && !modelOpen && menuSel >= 0 ? 'composer-opt-' + menuSel : undefined}
			aria-label={
				unavailableWorkspacePath
					? `Workspace unavailable: ${unavailableWorkspaceName}`
					: folderName
						? `Ask about ${folderName}`
						: 'Ask a question'
			}
			disabled={!!unavailableWorkspacePath}
			{placeholder}
			oninput={onInput}
			onkeydown={onKey}
			onfocus={() => (menuOff = false)}
		></Textarea>
		<div class="bottom-row">
			{#if !session.focus}
				<div class="mode-wrap">
					<DropdownMenu.Root open={modeOpen} onOpenChange={setModeMenuOpen}>
						<DropdownMenu.Trigger class="mode-trigger" type="button">
							<span class="mode-mark" class:inspect={mode === 'inspect'}></span>
							{mode === 'inspect' ? 'Check data' : 'Ask'}
							<Icon name="chevron-right" size={12} />
						</DropdownMenu.Trigger>
						<DropdownMenu.Portal>
							<DropdownMenu.Content class="fella-ui-menu-content mode-menu" side="top" align="start" sideOffset={8}>
								<DropdownMenu.RadioGroup value={mode} onValueChange={selectMode}>
									<DropdownMenu.RadioItem class="fella-ui-menu-item mode-option" value="ask">
										<span class="mode-mark"></span>
										<span class="mode-copy"><strong>Ask</strong><small>Answer from the workspace.</small></span>
									</DropdownMenu.RadioItem>
									<DropdownMenu.RadioItem class="fella-ui-menu-item mode-option" value="inspect">
										<span class="mode-mark inspect"></span>
										<span class="mode-copy"><strong>Check data</strong><small>Start with the files and show the checks.</small></span>
									</DropdownMenu.RadioItem>
								</DropdownMenu.RadioGroup>
							</DropdownMenu.Content>
						</DropdownMenu.Portal>
					</DropdownMenu.Root>
				</div>
				{#if !pendingInput}
					<div class="model-wrap">
						<Button
							variant="ghost"
							size="sm"
							class="model-trigger"
							aria-expanded={modelOpen}
							aria-haspopup="dialog"
							title={currentModel ? `Model: ${currentModel}` : 'Choose a model'}
							onclick={() => { modelOpen = !modelOpen; contextOpen = false; modeOpen = false; if (!modelOpen) modelQuery = ''; }}
						>
							{#if providerId}<ProviderIcon providerId={providerId} size={14} />{/if}
							<span class="model-name" class:empty={!currentModel}>{currentModel || 'Choose model'}</span>
							<Icon name="chevron-right" size={12} />
						</Button>
						{#if modelOpen}
							<div class="model-menu" role="dialog" aria-label="Choose a model" transition:enterUp>
								<div class="model-menu-head">
									<strong>Model</strong>
									<small>{providerName}</small>
								</div>
								<label class="model-search">
									<Icon name="search" size={14} />
									<span class="sr-only">Find a model</span>
									<Input
										class="model-search-input"
										bind:ref={modelInput}
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
											<Button
												variant="ghost"
												size="sm"
												class={`model-option${modelOption === currentModel ? ' selected' : ''}`}
												role="option"
												aria-selected={modelOption === currentModel}
												onclick={() => void chooseModel(modelOption)}
											>
												<span>{modelOption}</span>
												{#if modelOption === currentModel}<Icon name="check" size={14} />{/if}
											</Button>
										{/each}
									</div>
								{:else}
									<p class="model-empty">No models available from {providerName}. Refresh the connection in Settings.</p>
								{/if}
									<Button
										variant="ghost"
										size="sm"
										class="model-settings"
										onclick={() => { modelOpen = false; session.setWorkspaceView('settings'); }}
									>
										<Icon name="settings" size={14} /> Provider settings
								</Button>
							</div>
						{/if}
					</div>
				{/if}
			{/if}
			{#if answering && value.trim() && !pendingInput && !value.startsWith('/') && !session.mountProgress}
				<Button
					variant="default"
					size="icon"
					class="act send"
					title="Cancel and re-ask with this (Enter)"
					aria-label="Cancel and re-ask with this"
					onclick={() => void submit()}
				>
					<Icon name="corner-down-left" size={16} />
				</Button>
			{:else if session.busy}
				<Button variant="destructive" size="icon" class="act stop" title="Stop (Esc)" aria-label="Stop" onclick={() => stop()}>
					<Icon name="stop" size={16} />
				</Button>
			{:else if session.mountProgress}
				<Button variant="ghost" size="icon" class="act mount-wait" disabled title="Preparing workspace" aria-label="Preparing workspace">
					<Logo size={18} active />
				</Button>
			{:else if value.trim()}
				<Button variant="default" size="icon" class="act send" aria-label="Send" onclick={() => void submit()}>
					<Icon name="corner-down-left" size={16} />
				</Button>
			{/if}
		</div>
	</div>
	{/if}
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
	.workspace-unavailable-note {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-2);
		padding: 2px 0 8px;
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.workspace-unavailable-note span {
		min-width: 0;
	}
	:global(.workspace-reopen) {
		height: auto;
		min-height: 0;
		flex: none;
		padding: 3px 8px;
		border-radius: var(--radius-chip);
		background: var(--bg-inset);
		color: var(--text-dim);
		font-size: var(--fs-xs);
	}
	:global(.workspace-reopen:hover) {
		background: var(--bg-inset);
		color: var(--text);
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
	.ref-pill :global(.remove-ref) {
		display: grid;
		place-items: center;
		padding: 0;
		width: 16px;
		min-width: 16px;
		height: 16px;
		min-height: 16px;
		border-radius: 3px;
		color: var(--text-faint);
	}
	.ref-pill :global(.remove-ref:hover) {
		background: var(--bg-inset);
		color: var(--text);
	}
	.ref-pill :global(.remove-ref svg) {
		width: 12px;
		height: 12px;
	}
	:global(.context-add) {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		flex: none;
		height: auto;
		min-height: 0;
		padding: 3px 6px;
		border-radius: var(--radius-chip);
		color: var(--text-faint);
		font-size: var(--fs-xs);
		white-space: nowrap;
	}
	:global(.context-add:hover),
	:global(.context-add[aria-expanded='true']) {
		background: var(--bg-inset);
		color: var(--text-dim);
	}
	.mode-wrap {
		position: relative;
		flex: none;
	}
	:global(.mode-trigger) {
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
	:global(.mode-trigger:hover),
	:global(.mode-trigger[aria-expanded='true']) {
		background: var(--bg-inset);
		color: var(--text);
	}
	:global(.mode-trigger svg) {
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
	:global(.mode-menu) { width: 224px; }
	:global(.mode-option) {
		align-items: flex-start;
		gap: 9px;
		min-height: 0;
		padding: 8px;
		border-radius: var(--radius-chip);
		text-align: left;
	}
	:global(.mode-option[data-state='checked']) {
		background: var(--bg-inset);
		color: var(--text);
	}
	:global(.mode-option) .mode-copy {
		display: flex;
		flex-direction: column;
		gap: 1px;
	}
	:global(.mode-option) strong {
		font-size: var(--fs-xs);
		font-weight: 600;
	}
	:global(.mode-option) small {
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.model-wrap {
		position: relative;
		min-width: 0;
		flex: none;
	}
	:global(.model-trigger) {
		display: inline-flex;
		align-items: center;
		justify-content: flex-start;
		gap: 6px;
		min-width: 0;
		min-height: 0;
		height: auto;
		max-width: min(260px, 32vw);
		padding: 4px 6px;
		border-radius: var(--radius-chip);
		background: transparent;
		color: var(--text-dim);
		font-size: var(--fs-xs);
		font-weight: 500;
		white-space: nowrap;
	}
	:global(.model-trigger:hover),
	:global(.model-trigger[aria-expanded='true']) {
		background: var(--bg-inset);
		color: var(--text);
	}
	:global(.model-trigger) > :global(svg) {
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
	.model-search :global(.model-search-input) {
		min-width: 0;
		width: 100%;
		min-height: 0;
		border: 0;
		border-radius: 0;
		outline: 0;
		padding: 0;
		background: transparent;
		color: var(--text);
		font: inherit;
		font-size: var(--fs-sm);
	}
	.model-search :global(.model-search-input:focus-visible) {
		border-color: transparent;
		outline: 0;
		box-shadow: none;
	}
	.model-search :global(.model-search-input::placeholder) {
		color: var(--text-faint);
	}
	.model-list {
		max-height: min(280px, 38vh);
		overflow: auto;
	}
	:global(.model-list) :global(.model-option) {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-2);
		width: 100%;
		min-height: 0;
		height: auto;
		padding: 7px 8px;
		border-radius: var(--radius-chip);
		background: transparent;
		color: var(--text-dim);
		text-align: left;
		font-family: var(--mono);
		font-size: var(--fs-xs);
	}
	:global(.model-list) :global(.model-option:hover),
	:global(.model-list) :global(.model-option.selected) {
		background: var(--bg-inset);
		color: var(--text);
	}
	:global(.model-option) span {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	:global(.model-option) :global(svg) {
		flex: none;
		color: var(--brand);
	}
	.model-empty {
		margin: var(--space-3) var(--space-2);
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	:global(.model-settings) {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		width: 100%;
		height: auto;
		min-height: 0;
		margin-top: var(--space-2);
		padding: var(--space-2);
		border-radius: 0;
		border-top: 1px solid var(--border);
		color: var(--text-faint);
		font-size: var(--fs-xs);
		font-weight: 500;
		text-align: left;
	}
	:global(.model-settings:hover) {
		background: var(--bg-inset);
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
	.context-search :global(.context-search-input) {
		min-width: 0;
		width: 100%;
		min-height: 0;
		border: 0;
		border-radius: 0;
		outline: 0;
		padding: 0;
		background: transparent;
		color: var(--text);
		font: inherit;
		font-size: var(--fs-sm);
	}
	.context-search :global(.context-search-input:focus-visible) {
		border-color: transparent;
		outline: 0;
		box-shadow: none;
	}
	.context-search :global(.context-search-input::placeholder) {
		color: var(--text-faint);
	}
	.context-search :global(.context-close) {
		display: grid;
		place-items: center;
		width: 20px;
		min-width: 20px;
		height: 20px;
		min-height: 20px;
		padding: 0;
		border-radius: var(--radius-sm);
		flex: none;
		color: var(--text-faint);
	}
	.context-search :global(.context-close:hover) {
		background: var(--bg-inset);
		color: var(--text);
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
	:global(.context-main) {
		min-width: 0;
		display: flex;
		align-items: center;
		justify-content: flex-start;
		gap: 9px;
		flex: 1;
		height: auto;
		min-height: 0;
		padding: 7px 6px 7px 8px;
		border-radius: var(--radius-chip);
		background: transparent;
		color: var(--text-dim);
		font-size: var(--fs-xs);
		white-space: normal;
		text-align: left;
	}
	:global(.context-main:hover) {
		background: transparent;
		color: var(--text);
	}
	:global(.context-field) {
		width: 100%;
		display: flex;
		align-items: center;
		justify-content: flex-start;
		gap: 9px;
		height: auto;
		min-height: 0;
		padding: 6px 8px;
		border-radius: var(--radius-chip);
		background: transparent;
		color: var(--text-dim);
		font-size: var(--fs-xs);
		white-space: normal;
		text-align: left;
	}
	:global(.context-field:hover) {
		background: var(--bg-inset);
		color: var(--text);
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
		background: var(--workspace-canvas);
		border: 1px solid var(--border-strong);
		border-radius: 12px;
		box-shadow: none;
		padding: var(--space-3) var(--space-3) var(--space-2) var(--space-4);
		transition:
			border-color var(--dur-fast) var(--ease),
			box-shadow var(--dur-fast) var(--ease);
	}
	.field:focus-within {
		border-color: var(--link);
		box-shadow: var(--focus-ring);
	}
	.clarification-field {
		gap: 0;
		max-height: min(68vh, 520px);
		overflow-y: auto;
		padding: var(--space-3) var(--space-4);
	}
	.clarification-question {
		margin: 0;
		color: var(--text);
		font-size: var(--fs-lg);
		font-weight: 600;
		line-height: 1.4;
	}
	.clarification-reason {
		margin: 6px 0 0;
		color: var(--text-faint);
		font-size: var(--fs-sm);
		line-height: 1.45;
	}
	.clarification-options {
		display: grid;
		gap: 0;
		margin-top: var(--space-2);
		max-height: 228px;
		overflow-y: auto;
		scrollbar-width: thin;
	}
	:global(.clarification-option) {
		display: flex;
		align-items: center;
		justify-content: flex-start;
		gap: 10px;
		width: 100%;
		height: auto;
		min-height: 44px;
		padding: 6px 8px;
		border-bottom: 1px solid var(--border);
		border-radius: var(--radius-sm);
		color: var(--text-dim);
		background: transparent;
		font-size: var(--fs-sm);
		white-space: normal;
		text-align: left;
		transition: color var(--dur-fast) var(--ease), background var(--dur-fast) var(--ease);
	}
	:global(.clarification-option:hover:not(:disabled)) {
		color: var(--text);
		background: var(--bg-inset);
	}
	:global(.clarification-option:disabled) {
		cursor: progress;
		opacity: 0.6;
	}
	.clarification-index {
		display: grid;
		place-items: center;
		flex: none;
		width: 26px;
		height: 26px;
		border-radius: 8px;
		color: var(--text-faint);
		background: var(--bg-inset);
		font-size: var(--fs-xs);
		font-variant-numeric: tabular-nums;
	}
	.clarification-other {
		margin-top: var(--space-2);
		padding-top: var(--space-2);
		border-top: 1px solid var(--border);
	}
	.clarification-other label {
		display: block;
		margin-bottom: 4px;
		color: var(--text-faint);
		font-size: var(--fs-xs);
		font-weight: 600;
	}
	.clarification-other-row {
		display: flex;
		align-items: flex-end;
		gap: var(--space-2);
	}
	.clarification-other-row :global(.clarification-input) {
		min-width: 0;
		min-height: 42px;
		max-height: 100px;
		width: 100%;
		padding: 7px 9px;
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		background: var(--bg-inset);
		color: var(--text);
		font: inherit;
		font-size: var(--fs-sm);
		line-height: 1.4;
		resize: vertical;
	}
	.clarification-other-row :global(.clarification-input:focus-visible) {
		border-color: var(--link);
		outline: 2px solid color-mix(in srgb, var(--link) 22%, transparent);
		outline-offset: 1px;
	}
	:global(.clarification-send) {
		display: grid;
		place-items: center;
		flex: none;
		width: 30px;
		min-width: 30px;
		height: 30px;
		min-height: 30px;
		padding: 0;
		border-radius: 50%;
		color: var(--on-brand);
		background: var(--brand);
		transition: filter var(--dur-fast) var(--ease), opacity var(--dur-fast) var(--ease);
	}
	:global(.clarification-send:hover:not(:disabled)) { filter: brightness(0.92); }
	:global(.clarification-send:disabled) { opacity: 0.45; cursor: default; }
	.bottom-row {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		min-height: 28px;
	}
	.field :global(.composer-main-textarea) {
		width: 100%;
		min-height: 0;
		resize: none;
		border: none;
		border-radius: 0;
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
	.field :global(.composer-main-textarea:focus-visible) {
		border-color: transparent;
		outline: none;
		box-shadow: none;
	}
	.field :global(.composer-main-textarea::placeholder) {
		color: var(--text-faint);
	}
	/* API-key entry: mask the characters in the Electron renderer. */
	.field.secret :global(.composer-main-textarea) {
		-webkit-text-security: disc;
		font-family: var(--mono);
	}
	/* Trailing action send when there's text, stop while a run is live. */
	:global(.bottom-row .act) {
		flex: none;
		margin-left: auto;
		display: grid;
		place-items: center;
		width: 28px;
		min-width: 28px;
		height: 28px;
		min-height: 28px;
		padding: 0;
		border-radius: 50%;
		color: var(--text-faint);
		background: transparent;
		transition:
			background var(--dur-fast) var(--ease),
			color var(--dur-fast) var(--ease);
	}
	:global(.bottom-row .act.send) {
		background: var(--brand);
		color: var(--on-brand);
	}
	:global(.bottom-row .act.send:hover) {
		color: var(--on-brand);
		background: var(--brand);
		filter: brightness(0.9);
	}
	:global(.bottom-row .act.stop) {
		color: var(--err);
	}
	:global(.bottom-row .act.stop:hover) {
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
	.menu :global(.completion-option) {
		min-width: 0;
		width: 100%;
		height: auto;
		justify-content: flex-start;
		align-items: baseline;
		padding: var(--space-2) var(--space-3);
		border-radius: var(--radius-chip);
		text-align: left;
		font: inherit;
		font-weight: 500;
		color: var(--text);
	}
	.menu :global(.completion-option:hover),
	.menu :global(.completion-option[aria-selected='true']) {
		background: var(--bg-inset);
	}
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

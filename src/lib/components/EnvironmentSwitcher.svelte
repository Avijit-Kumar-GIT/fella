<script lang="ts">
	import { session } from '$lib/session.svelte';
	import { DropdownMenu } from '$lib/components/ui/dropdown-menu';
	import { Dialog } from '$lib/components/ui/dialog';
	import { Button } from '$lib/components/ui/button';
	import Icon from './Icon.svelte';
	import Logo from './Logo.svelte';

	let {
		onselect,
		onnew,
		oncompose,
		onclose,
		compact = false
	}: {
		onselect: (id: string) => void | Promise<void>;
		onnew: () => void;
		oncompose: (name: string, workspaceIds: (string | null)[]) => void;
		onclose: (id: string) => void | Promise<void>;
		compact?: boolean;
	} = $props();

	const shortcutModifier =
		typeof navigator !== 'undefined' && /Mac/i.test(navigator.platform || navigator.userAgent) ? '⌘' : 'Ctrl';

	let menuOpen = $state(false);
	let menuContent = $state<HTMLElement | null>(null);
	let composeOpen = $state(false);
	let composeName = $state('');
	let selectedWorkspaceIds = $state<(string | null)[]>([]);
	let activeEnvironment = $derived(session.activeEnvironment);
	let activeLabel = $derived(activeEnvironment ? session.environmentLabel(activeEnvironment) : 'General');
	let workspaceChoices = $derived.by(() => {
		const paths = [...new Set([
			...session.workspaceWindows.map((workspace) => workspace.id),
			...session.repositoryPaths
		])].filter((path) => !session.hiddenRepositoryPaths.includes(path));
		return [
			{ id: null as string | null, label: 'General', detail: 'Conversations without a folder' },
			...paths.map((path) => {
				const workspace = session.workspaceAt(path);
				const label = (workspace?.path ?? path).replace(/[/\\]+$/, '').split(/[/\\]/).at(-1) || 'Workspace';
				return {
					id: path,
					label,
					detail: workspace?.historyOnly || session.historyOnlyRepositoryPaths.includes(path)
						? 'Saved conversations · folder unavailable'
						: path
				};
			})
		];
	});

	function environmentIsBusy(environment: (typeof session.environments)[number] | null): boolean {
		return !!environment?.panes.some((pane) =>
			session.conversations.find((conversation) => conversation.id === pane.conversationId)?.busy
		);
	}

	function environmentMenuLabel(environment: (typeof session.environments)[number]): string {
		const label = session.environmentLabel(environment);
		return environment.panes.length > 1 ? `${label} · ${environment.panes.length} workspaces` : label;
	}

	function selectEnvironment(id: string): void {
		if (id !== session.activeEnvironmentId) void onselect(id);
	}

	function closeCurrentEnvironment(): void {
		if (activeEnvironment && session.environments.length > 1) void onclose(activeEnvironment.id);
	}

	function openComposer(): void {
		selectedWorkspaceIds = activeEnvironment?.panes.map((pane) => pane.workspaceId) ?? [];
		if (!selectedWorkspaceIds.length) selectedWorkspaceIds = [session.activeWorkspaceId];
		composeName = activeEnvironment?.name ? `${activeEnvironment.name} copy` : '';
		composeOpen = true;
	}

	function toggleWorkspace(id: string | null, checked: boolean): void {
		if (checked) {
			if (selectedWorkspaceIds.length >= 4 || selectedWorkspaceIds.some((item) => item === id)) return;
			selectedWorkspaceIds = [...selectedWorkspaceIds, id];
		} else {
			selectedWorkspaceIds = selectedWorkspaceIds.filter((item) => item !== id);
		}
	}

	function createComposition(): void {
		const name = composeName.trim();
		if (!name || selectedWorkspaceIds.length < 2 || selectedWorkspaceIds.length > 4) return;
		oncompose(name, [...selectedWorkspaceIds]);
		composeOpen = false;
	}

	function setMenuOpen(open: boolean): void {
		menuOpen = open;
		if (!open) return;
		requestAnimationFrame(() => {
			if (!menuOpen) return;
			menuContent?.querySelector<HTMLElement>('[aria-checked="true"]')?.scrollIntoView({ block: 'nearest' });
		});
	}
</script>

<div class="environment-switcher" class:compact>
	<DropdownMenu.Root open={menuOpen} onOpenChange={setMenuOpen}>
		<DropdownMenu.Trigger
			class="environment-switcher-trigger"
			aria-label={`Switch environment. Current: ${activeLabel}${(activeEnvironment?.panes.length ?? 0) > 1 ? `, ${activeEnvironment?.panes.length} workspaces` : ''}`}
			title={`Switch environment (${shortcutModifier}+[ / ] or ${shortcutModifier}+1–9)`}
		>
			{#if (activeEnvironment?.panes.length ?? 0) > 1}
				<Icon name="panel" size={14} />
			{:else if activeEnvironment?.panes[0]?.workspaceId}
				<Icon name="repository" size={14} />
			{:else}
				<Logo size={16} active={environmentIsBusy(activeEnvironment)} />
			{/if}
			<span class="environment-switcher-label">{activeLabel}</span>
			{#if (activeEnvironment?.panes.length ?? 0) > 1}
				<span class="environment-switcher-count" aria-hidden="true">{activeEnvironment?.panes.length}</span>
			{/if}
			<Icon name="chevron-right" size={14} />
		</DropdownMenu.Trigger>
		<DropdownMenu.Portal>
			<DropdownMenu.Content
				class="fella-ui-menu-content environment-menu-content"
				side="bottom"
				align="start"
				sideOffset={6}
				bind:ref={menuContent}
			>
				<DropdownMenu.RadioGroup value={session.activeEnvironmentId} onValueChange={selectEnvironment}>
					{#each session.environments as environment (environment.id)}
						<DropdownMenu.RadioItem class="fella-ui-menu-item environment-option" value={environment.id}>
							<span class="environment-option-label">{environmentMenuLabel(environment)}</span>
							{#if environment.id === session.activeEnvironmentId}
								<Icon name="check" size={14} />
							{/if}
						</DropdownMenu.RadioItem>
					{/each}
				</DropdownMenu.RadioGroup>

				<div class="environment-menu-separator" role="separator"></div>
				{#if (activeEnvironment?.panes.length ?? 0) > 1}
					<DropdownMenu.Item class="fella-ui-menu-item" onSelect={() => session.setWorkspaceView('board')}>
						Open workspace overview
					</DropdownMenu.Item>
				{/if}
				<DropdownMenu.Item class="fella-ui-menu-item" onSelect={onnew}>
					New environment
				</DropdownMenu.Item>
				<DropdownMenu.Item class="fella-ui-menu-item" onSelect={openComposer}>
					Combine workspaces…
				</DropdownMenu.Item>
				{#if session.environments.length > 1}
					<DropdownMenu.Item
						class="fella-ui-menu-item close-environment"
						disabled={environmentIsBusy(activeEnvironment)}
						onSelect={closeCurrentEnvironment}
					>
						Close current environment
					</DropdownMenu.Item>
				{/if}
			</DropdownMenu.Content>
		</DropdownMenu.Portal>
	</DropdownMenu.Root>

	<Dialog.Root bind:open={composeOpen}>
		<Dialog.Portal>
			<Dialog.Overlay class="fella-ui-dialog-overlay" />
			<Dialog.Content class="fella-ui-dialog-content environment-composer-dialog">
				<header class="environment-composer-header">
					<div>
						<Dialog.Title class="environment-composer-title" level={2}>Combine workspaces</Dialog.Title>
						<Dialog.Description class="environment-composer-description">
							Create a named environment with up to four workspaces. Selecting a workspace elsewhere will not add it here.
						</Dialog.Description>
					</div>
					<Dialog.Close class="environment-composer-close" type="button" aria-label="Close workspace composer">
						<Icon name="x" size={16} />
					</Dialog.Close>
				</header>
				<label class="environment-name-field">
					<span>Environment name</span>
					<input bind:value={composeName} maxlength="64" placeholder="e.g. Weekly sales review" />
				</label>
				<fieldset class="environment-choice-list">
					<legend>Workspaces <span>{selectedWorkspaceIds.length} of 4 selected</span></legend>
					{#each workspaceChoices as choice (choice.id ?? 'general')}
						{@const checked = selectedWorkspaceIds.some((id) => id === choice.id)}
						<label class="environment-choice">
							<input
								type="checkbox"
								checked={checked}
								disabled={!checked && selectedWorkspaceIds.length >= 4}
								aria-label={`Include ${choice.label}`}
								onchange={(event) => toggleWorkspace(choice.id, event.currentTarget.checked)}
							/>
							<span class="environment-choice-copy"><strong>{choice.label}</strong><small>{choice.detail}</small></span>
						</label>
					{/each}
				</fieldset>
				<footer class="environment-composer-actions">
					<Button variant="ghost" onclick={() => (composeOpen = false)}>Cancel</Button>
					<Button disabled={!composeName.trim() || selectedWorkspaceIds.length < 2} onclick={createComposition}>
						Create environment
					</Button>
				</footer>
			</Dialog.Content>
		</Dialog.Portal>
	</Dialog.Root>
</div>

<style>
	.environment-switcher {
		flex: none;
		width: 100%;
		min-width: 0;
		padding: var(--space-1) var(--space-2) var(--space-2);
		-webkit-app-region: no-drag;
	}
	:global(.environment-switcher-trigger) {
		width: 100%;
		min-width: 0;
		min-height: 34px;
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 0 10px;
		border: 1px solid var(--pane-edge);
		border-radius: 999px;
		background: color-mix(in srgb, var(--text) 3%, var(--sidebar-surface));
		color: var(--text-dim);
		text-align: left;
		font-size: var(--fs-sm);
		transition: background var(--dur-fast) var(--ease), border-color var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
	}
	:global(.environment-switcher-trigger:hover),
	:global(.environment-switcher-trigger[data-state='open']) {
		border-color: var(--border-strong);
		background: var(--sidebar-hover);
		color: var(--text);
	}
	:global(.environment-switcher-trigger:focus-visible) {
		outline: 2px solid var(--brand);
		outline-offset: 2px;
	}
	:global(.environment-switcher-trigger > svg:first-child),
	:global(.environment-switcher-trigger > .logo) { flex: none; }
	.environment-switcher-label {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-weight: 550;
	}
	.environment-switcher-count {
		flex: none;
		color: var(--text-faint);
		font-size: var(--fs-xs);
		font-variant-numeric: tabular-nums;
	}
	:global(.environment-switcher-trigger > svg:last-child) {
		flex: none;
		color: var(--text-faint);
		transform: rotate(90deg);
		transition: transform var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
	}
	:global(.environment-switcher-trigger[data-state='open'] > svg:last-child) {
		color: var(--text-dim);
		transform: rotate(-90deg);
	}
	:global(.environment-menu-content) {
		width: 224px;
		max-height: min(420px, calc(100vh - 84px));
		overflow-y: auto;
	}
	:global(.environment-option) {
		gap: 8px;
	}
	.environment-option-label {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	:global(.environment-option > svg) {
		flex: none;
		margin-left: auto;
		color: var(--text-dim);
	}
	.environment-menu-separator {
		height: 1px;
		margin: 5px 5px;
		background: var(--border);
	}
	:global(.close-environment[aria-disabled='true']) {
		color: var(--text-faint);
		opacity: 0.55;
	}
	:global(.environment-composer-dialog) {
		width: min(500px, calc(100vw - 32px));
		max-height: min(620px, calc(100vh - 32px));
		display: grid;
		grid-template-rows: auto auto minmax(0, 1fr) auto;
		gap: 16px;
		padding: 22px;
	}
	.environment-composer-header { display: flex; justify-content: space-between; align-items: flex-start; gap: 16px; }
	:global(.environment-composer-title) { margin: 0; font-size: 18px; font-weight: 650; letter-spacing: -.02em; }
	:global(.environment-composer-description) { margin-top: 5px; color: var(--text-dim); font-size: var(--fs-sm); line-height: 1.5; }
	:global(.environment-composer-close) { display: grid; place-items: center; flex: none; width: 30px; height: 30px; border: 0; border-radius: var(--radius-sm); background: transparent; color: var(--text-faint); }
	:global(.environment-composer-close:hover) { background: var(--sidebar-hover); color: var(--text); }
	.environment-name-field { display: grid; gap: 6px; color: var(--text-dim); font-size: var(--fs-xs); font-weight: 600; }
	.environment-name-field input { width: 100%; min-height: 36px; padding: 7px 10px; border: 1px solid var(--border); border-radius: var(--radius-sm); outline: none; background: var(--bg); color: var(--text); font: inherit; font-size: var(--fs-sm); }
	.environment-name-field input:focus { border-color: var(--brand); box-shadow: 0 0 0 2px color-mix(in srgb, var(--brand) 18%, transparent); }
	.environment-choice-list { display: grid; align-content: start; gap: 3px; min-height: 0; overflow: auto; padding: 0; margin: 0; border: 0; }
	.environment-choice-list legend { width: 100%; display: flex; justify-content: space-between; margin-bottom: 6px; color: var(--text); font-size: var(--fs-sm); font-weight: 600; }
	.environment-choice-list legend span { color: var(--text-faint); font-size: var(--fs-xs); font-weight: 450; }
	.environment-choice { display: flex; align-items: flex-start; gap: 10px; padding: 9px 10px; border-radius: var(--radius-sm); cursor: pointer; }
	.environment-choice:hover { background: var(--sidebar-hover); }
	.environment-choice input { flex: none; margin: 3px 0 0; accent-color: var(--brand); }
	.environment-choice-copy { display: grid; min-width: 0; gap: 2px; }
	.environment-choice-copy strong { overflow: hidden; color: var(--text); font-size: var(--fs-sm); font-weight: 550; text-overflow: ellipsis; white-space: nowrap; }
	.environment-choice-copy small { overflow: hidden; color: var(--text-faint); font-size: var(--fs-xs); text-overflow: ellipsis; white-space: nowrap; }
	.environment-composer-actions { display: flex; justify-content: flex-end; gap: 8px; padding-top: 2px; }
	.environment-switcher.compact {
		width: min(190px, 24vw);
		padding: 0 0 0 var(--space-1);
	}
	:global(.environment-switcher.compact .environment-switcher-trigger) {
		min-height: 28px;
		padding-inline: 9px;
		background: transparent;
	}
	:global(.environment-switcher.compact .environment-switcher-trigger:hover),
	:global(.environment-switcher.compact .environment-switcher-trigger[data-state='open']) {
		background: var(--bg-inset);
	}
</style>

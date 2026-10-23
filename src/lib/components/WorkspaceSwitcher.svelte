<script lang="ts">
	import { session, GENERAL_WORKSPACE_ID } from '$lib/session.svelte';
	import { DropdownMenu } from '$lib/components/ui/dropdown-menu';
	import Icon from './Icon.svelte';
	import Logo from './Logo.svelte';
	import { openFolder } from '$lib/commands';

	let {
		onselect,
		compact = false,
		identity = false
	}: {
		onselect: (id: string) => void | Promise<void>;
		compact?: boolean;
		identity?: boolean;
	} = $props();

	const shortcutModifier =
		typeof navigator !== 'undefined' && /Mac/i.test(navigator.platform || navigator.userAgent) ? '⌘' : 'Ctrl';

	let menuOpen = $state(false);
	let menuContent = $state<HTMLElement | null>(null);
	let menuSelection = $state(GENERAL_WORKSPACE_ID);
	let activeLabel = $derived(session.workspaceLabel(session.activeWorkspaceId));
	let choices = $derived.by(() => {
		const paths = [...new Set([
			...session.workspaceWindows.map((workspace) => workspace.id),
			...session.repositoryPaths
		])].filter((path) => !session.hiddenRepositoryPaths.includes(path));
		return [
			{ id: GENERAL_WORKSPACE_ID, label: 'General', detail: 'Conversations without a folder' },
			...paths.map((path) => ({
				id: path,
				label: session.workspaceLabel(path),
				detail: session.historyOnlyRepositoryPaths.includes(path) || session.workspaceAt(path)?.historyOnly
					? 'Saved conversations · folder unavailable'
					: path
			}))
		];
	});

	function selectWorkspace(id: string): void {
		const scope = id === GENERAL_WORKSPACE_ID ? null : id;
		if (scope === session.activeWorkspaceId) return;
		void onselect(id);
	}

	function handleWorkspaceMenuKeydown(event: KeyboardEvent): void {
		if (!['Home', 'End'].includes(event.key)) return;
		const items = [...(menuContent?.querySelectorAll<HTMLElement>('[role="menuitemradio"]') ?? [])];
		if (!items.length) return;

		const nextIndex = event.key === 'Home' ? 0 : items.length - 1;
		const nextItem = items[nextIndex];
		if (!nextItem) return;

		event.preventDefault();
		event.stopPropagation();
		nextItem.focus();
	}

	function setMenuOpen(open: boolean): void {
		menuOpen = open;
		if (!open) return;
		menuSelection = session.activeWorkspaceId ?? GENERAL_WORKSPACE_ID;
		requestAnimationFrame(() => {
			if (!menuOpen) return;
			menuContent?.querySelector<HTMLElement>('[aria-checked="true"]')?.scrollIntoView({ block: 'nearest' });
		});
	}
</script>

<div class="workspace-switcher" class:compact class:identity>
	<DropdownMenu.Root open={menuOpen} onOpenChange={setMenuOpen}>
		<DropdownMenu.Trigger
			class="workspace-switcher-trigger"
			aria-label={`${identity ? 'Choose workspace from sidebar' : 'Switch workspace'}. Current: ${activeLabel}`}
			title={`Switch workspace (${shortcutModifier}+1–9)`}
		>
			{#if identity}
				<Logo size={36} active={session.busy} />
				<span class="workspace-identity-copy"><strong>Fella</strong><small>{activeLabel.replace(/^Fella\s+/i, '')}</small></span>
			{:else}
				<Logo size={20} active={session.busy} />
				<span class="workspace-switcher-label">{activeLabel}</span>
			{/if}
			<Icon name="chevron-down" size={12} />
		</DropdownMenu.Trigger>
		<DropdownMenu.Portal>
			<DropdownMenu.Content
				class="fella-ui-menu-content workspace-menu-content"
				side="bottom"
				align="start"
				sideOffset={6}
				bind:ref={menuContent}
			>
				<DropdownMenu.RadioGroup
					value={menuSelection}
					onValueChange={(id) => (menuSelection = id)}
				>
					{#each choices as choice (choice.id)}
						<DropdownMenu.RadioItem
							class="fella-ui-menu-item workspace-option"
							value={choice.id}
							onSelect={() => selectWorkspace(choice.id)}
						>
							<span class="workspace-option-copy">
								<strong>{choice.label}</strong>
								{#if choice.id !== GENERAL_WORKSPACE_ID}
									<small title={choice.detail}>{choice.detail}</small>
								{/if}
							</span>
							{#if choice.id === menuSelection}
								<Icon name="check" size={14} />
							{/if}
						</DropdownMenu.RadioItem>
					{/each}
				</DropdownMenu.RadioGroup>
				<DropdownMenu.Separator class="workspace-menu-separator" />
				<DropdownMenu.Item class="fella-ui-menu-item" onSelect={() => void openFolder()}><Icon name="plus" size={14} />Add workspace</DropdownMenu.Item>
			</DropdownMenu.Content>
		</DropdownMenu.Portal>
	</DropdownMenu.Root>
</div>

<style>
	.workspace-switcher {
		flex: none;
		width: 100%;
		min-width: 0;
		padding: var(--space-1) var(--space-2) var(--space-2);
		-webkit-app-region: no-drag;
	}
	:global(.workspace-switcher-trigger) {
		width: 100%;
		height: 36px;
		min-width: 0;
		display: flex;
		align-items: center;
		gap: 9px;
		padding: 6px 10px;
		border: 1px solid var(--border);
		border-radius: var(--radius);
		background: var(--surface);
		color: var(--text-dim);
		text-align: left;
		transition: background 120ms ease, border-color 120ms ease, color 120ms ease;
	}
	:global(.workspace-switcher-trigger:hover),
	:global(.workspace-switcher-trigger[data-state='open']) {
		border-color: var(--border-strong, var(--border));
		background: var(--sidebar-hover);
		color: var(--text);
	}
	:global(.workspace-switcher-trigger:focus-visible) {
		outline: 2px solid color-mix(in srgb, var(--brand) 52%, transparent);
		outline-offset: 2px;
	}
	:global(.workspace-switcher-trigger > svg:first-child),
	:global(.workspace-switcher-trigger > .logo) { flex: none; }
	.workspace-switcher-label {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		color: var(--text);
		font-size: var(--fs-sm);
		font-weight: 550;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	:global(.workspace-switcher-trigger > svg:last-child) {
		flex: none;
		transform: none;
		color: var(--text-faint);
		transition: transform 120ms ease;
	}
	:global(.workspace-switcher-trigger[data-state='open'] > svg:last-child) { transform: rotate(180deg); }
	:global(.workspace-menu-content) {
		min-width: 260px;
		max-width: min(360px, calc(100vw - 24px));
		max-height: min(480px, calc(100vh - 40px));
		overflow-y: auto;
		overscroll-behavior: contain;
	}
	:global(.workspace-option) { display: flex; align-items: center; justify-content: space-between; gap: 14px; }
	.workspace-option-copy { display: grid; min-width: 0; gap: 2px; }
	.workspace-option-copy strong {
		overflow: hidden;
		color: var(--text);
		font-size: var(--fs-sm);
		font-weight: 550;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.workspace-option-copy small {
		overflow: hidden;
		color: var(--text-faint);
		font-size: var(--fs-xs);
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.workspace-switcher.compact { width:max-content; max-width:100%; padding:0; }
	:global(.workspace-menu-separator) { height: 1px; margin: 6px 0; background: var(--border); }
	:global(.workspace-switcher.compact .workspace-switcher-trigger) { width:auto; max-width:100%; height:32px; min-height:32px; gap:8px; padding:6px 12px; border:0; border-radius:999px; background:var(--bg-raised); }
	:global(.workspace-switcher.compact .workspace-switcher-label) { font-size:13px; font-weight:500; }
	.workspace-switcher.identity { flex:1; width:auto; min-width:0; padding:0; }
	:global(.workspace-switcher.identity .workspace-switcher-trigger) { height:auto; min-height:36px; gap:12px; padding:0; border:0; background:transparent; }
	.workspace-identity-copy { display:grid; flex:1; min-width:0; gap:1px; }
	.workspace-identity-copy strong { color:var(--text); font-size:14px; font-weight:500; }
	.workspace-identity-copy small { overflow:hidden; color:var(--text-dim); font-size:12px; text-overflow:ellipsis; white-space:nowrap; }
	:global(.workspace-switcher.identity .workspace-switcher-trigger > svg:last-child) { opacity:0; }
	:global(.workspace-switcher.identity .workspace-switcher-trigger:hover > svg:last-child), :global(.workspace-switcher.identity .workspace-switcher-trigger:focus-visible > svg:last-child) { opacity:1; }
</style>

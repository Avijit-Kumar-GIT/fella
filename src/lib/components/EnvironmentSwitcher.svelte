<script lang="ts">
	import { session } from '$lib/session.svelte';
	import { DropdownMenu } from '$lib/components/ui/dropdown-menu';
	import Icon from './Icon.svelte';
	import Logo from './Logo.svelte';

	let {
		onselect,
		onnew,
		onclose,
		compact = false
	}: {
		onselect: (id: string) => void | Promise<void>;
		onnew: () => void;
		onclose: (id: string) => void | Promise<void>;
		compact?: boolean;
	} = $props();

	const shortcutModifier =
		typeof navigator !== 'undefined' && /Mac/i.test(navigator.platform || navigator.userAgent) ? '⌘' : 'Ctrl';

	let menuOpen = $state(false);
	let menuContent = $state<HTMLElement | null>(null);
	let activeEnvironment = $derived(session.activeEnvironment);
	let activeLabel = $derived(activeEnvironment ? session.environmentLabel(activeEnvironment) : 'General');

	function environmentIsBusy(environment: (typeof session.environments)[number] | null): boolean {
		return !!environment?.panes.some((pane) =>
			session.conversations.find((conversation) => conversation.id === pane.conversationId)?.busy
		);
	}

	function environmentMenuLabel(environment: (typeof session.environments)[number]): string {
		const locations = [
			...new Set(
				environment.panes.map((pane) => {
					if (!pane.workspaceId) return 'General';
					const workspace = session.workspaceAt(pane.workspaceId);
					return (
						(workspace?.path ?? pane.workspaceId).replace(/[/\\]+$/, '').split(/[/\\]/).at(-1) ||
						'Workspace'
					);
				})
			)
		];
		return locations.length > 1 ? locations.join(' · ') : session.environmentLabel(environment);
	}

	function selectEnvironment(id: string): void {
		if (id !== session.activeEnvironmentId) void onselect(id);
	}

	function closeCurrentEnvironment(): void {
		if (activeEnvironment && session.environments.length > 1) void onclose(activeEnvironment.id);
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
			aria-label={`Switch environment. Current: ${activeLabel}`}
			title={`Switch environment (${shortcutModifier}+[ / ] or ${shortcutModifier}+1–9)`}
		>
			{#if activeEnvironment?.panes[0]?.workspaceId}
				<Icon name="repository" size={14} />
			{:else if (activeEnvironment?.panes.length ?? 0) > 1}
				<Icon name="panel" size={14} />
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
				<DropdownMenu.Item class="fella-ui-menu-item" onSelect={onnew}>
					New environment
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

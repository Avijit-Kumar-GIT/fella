<script lang="ts">
	import { session } from '$lib/session.svelte';
	import { openContext } from '$lib/commands';
	import { DropdownMenu } from '$lib/components/ui/dropdown-menu';
	import WorkspaceSwitcher from './WorkspaceSwitcher.svelte';
	import Icon from './Icon.svelte';
	import Logo from './Logo.svelte';
	let { onpalette, onworkspaceselect, expanded = false, compact = false, nativeOverlay = false, ontoggleexpand }: {
		onpalette: () => void; onworkspaceselect: (id:string) => void | Promise<void>;
		expanded?: boolean; compact?: boolean; nativeOverlay?: boolean; ontoggleexpand?: () => void;
	} = $props();
	const nativePlatform = typeof window === 'undefined' ? null : window.fella?.platform ?? null;
	const showIntegratedMenus = nativePlatform !== null && nativePlatform !== 'darwin';
	function openNativeMenu(event: MouseEvent, name: 'file' | 'edit' | 'view' | 'help'): void {
		const button = event.currentTarget as HTMLElement;
		const rect = button.getBoundingClientRect();
		void window.fella?.popupMenu(name, rect.left, rect.bottom);
	}
</script>

<header class="titlebar" class:titlebar-overlay={nativeOverlay} class:integrated-menu={showIntegratedMenus} aria-label="Workspace navigation">
	{#if showIntegratedMenus}
		<div class="titlebar-leading">
			<span class="titlebar-logo"><Logo size={14} /></span>
			<nav class="titlebar-menus" aria-label="Application menu">
				{#each [{ id:'file', label:'File' }, { id:'edit', label:'Edit' }, { id:'view', label:'View' }, { id:'help', label:'Help' }] as menu (menu.id)}
					<button type="button" aria-haspopup="menu" onclick={(event) => openNativeMenu(event, menu.id as 'file' | 'edit' | 'view' | 'help')}>{menu.label}</button>
				{/each}
			</nav>
		</div>
	{/if}
	<div class="titlebar-workspace"><WorkspaceSwitcher compact onselect={onworkspaceselect} /></div>
	<div class="titlebar-actions">
		{#if session.activeChat.workspaceScope}
			<button class="chrome-action" aria-label={session.workspacePaneOpen ? 'Close the panel' : 'Open the panel'} title={session.workspacePaneOpen ? 'Close the panel' : 'Open the panel'} aria-expanded={session.workspacePaneOpen} onclick={() => session.toggleWorkspacePane()}><Icon name="panel-right" size={14} /></button>
		{/if}
		<DropdownMenu.Root>
			<DropdownMenu.Trigger class="chrome-action" aria-label="Workspace actions" title="Workspace actions"><Icon name="more-horizontal" size={16} /></DropdownMenu.Trigger>
			<DropdownMenu.Portal><DropdownMenu.Content class="fella-ui-menu-content" align="end" sideOffset={6}>
				<DropdownMenu.Item class="fella-ui-menu-item" onSelect={onpalette}><Icon name="search" size={14} />Search Fella</DropdownMenu.Item>
				{#if session.catalog.workspace}
					<DropdownMenu.Item class="fella-ui-menu-item" onSelect={() => void openContext()}><Icon name="notebook-pen" size={14} />Workspace context</DropdownMenu.Item>
					<DropdownMenu.Separator class="workspace-context-separator" />
				{/if}
				<DropdownMenu.Item class="fella-ui-menu-item" onSelect={() => { session.setContentLayout('chat'); if (expanded) ontoggleexpand?.(); }}>Chat view</DropdownMenu.Item>
				<DropdownMenu.Item class="fella-ui-menu-item" onSelect={() => session.setContentLayout('notebook')}>Notebook view</DropdownMenu.Item>
				{#if session.workspacePaneOpen && !compact}
					<DropdownMenu.Item class="fella-ui-menu-item" onSelect={() => ontoggleexpand?.()}><Icon name={expanded ? 'columns-2' : 'arrow-up-right'} size={14} />{expanded ? 'Restore split view' : 'Expand panel'}</DropdownMenu.Item>
				{/if}
			</DropdownMenu.Content></DropdownMenu.Portal>
		</DropdownMenu.Root>
	</div>
</header>

<style>
	.titlebar { position:absolute; inset:0 0 auto; z-index:50; display:flex; height:56px; align-items:flex-start; justify-content:center; padding:12px 20px 0; pointer-events:none; user-select:none; -webkit-app-region:drag; }
	.titlebar-workspace { min-width:0; max-width:calc(100% - 140px); pointer-events:auto; -webkit-app-region:no-drag; }
	.titlebar-actions { position:absolute; top:14px; right:12px; display:flex; gap:2px; pointer-events:auto; -webkit-app-region:no-drag; }
	.titlebar-overlay { inset:0 auto auto env(titlebar-area-x, 0px); width:env(titlebar-area-width, 100%); height:env(titlebar-area-height, 32px); display:grid; grid-template-columns:minmax(0,1fr) auto minmax(0,1fr); align-items:center; gap:8px; padding:0 8px; background:var(--app-chrome); }
	.titlebar-overlay.integrated-menu { width:min(env(titlebar-area-width, calc(100vw - 140px)), calc(100vw - 140px)); }
	.titlebar-overlay .titlebar-leading { grid-column:1; display:flex; min-width:0; align-items:center; justify-self:start; gap:8px; pointer-events:auto; }
	.titlebar-overlay .titlebar-logo { display:flex; flex:none; align-items:center; justify-content:center; }
	.titlebar-overlay .titlebar-menus { display:flex; min-width:0; align-items:center; justify-self:start; gap:2px; pointer-events:auto; -webkit-app-region:no-drag; }
	.titlebar-overlay .titlebar-menus button { min-width:0; height:26px; padding:0 8px; border-radius:5px; color:var(--text); font-size:12px; font-weight:400; }
	.titlebar-overlay .titlebar-menus button:hover, .titlebar-overlay .titlebar-menus button:focus-visible { background:var(--bg-raised-2); }
	.titlebar-overlay .titlebar-workspace { grid-column:2; align-self:stretch; display:flex; align-items:center; box-sizing:border-box; margin-block:0; padding-block:6px; max-width:min(274px, calc(100vw - 420px)); }
	.titlebar-overlay .titlebar-actions { position:static; grid-column:3; justify-self:end; }
	.titlebar-overlay :global(.chrome-action) { width:26px; height:26px; }
	@media (max-width:700px) { .titlebar-overlay { gap:2px; padding-inline:4px; } .titlebar-overlay .titlebar-leading { gap:4px; } .titlebar-overlay .titlebar-menus button { padding-inline:5px; } .titlebar-overlay .titlebar-workspace { max-width:calc(100vw - 500px); } }
	@media (max-width:640px) { .titlebar-overlay.integrated-menu .titlebar-workspace { display:none; } }
	:global(.chrome-action) { display:grid; width:28px; height:28px; place-items:center; padding:0; border:0; border-radius:50%; background:transparent; color:var(--text-faint); cursor:pointer; }
	:global(.chrome-action:hover), :global(.chrome-action[data-state='open']) { background:var(--bg-raised); color:var(--text); }
	:global(.workspace-context-separator) { height:1px; margin:6px 0; background:var(--border); }
</style>

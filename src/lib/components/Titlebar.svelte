<script lang="ts">
	import { session } from '$lib/session.svelte';
	import { DropdownMenu } from '$lib/components/ui/dropdown-menu';
	import WorkspaceSwitcher from './WorkspaceSwitcher.svelte';
	import Icon from './Icon.svelte';
	let { onpalette, onworkspaceselect, expanded = false, compact = false, ontoggleexpand }: {
		onpalette: () => void; onworkspaceselect: (id:string) => void | Promise<void>;
		expanded?: boolean; compact?: boolean; ontoggleexpand?: () => void;
	} = $props();
</script>

<header class="titlebar" aria-label="Workspace navigation">
	<div class="titlebar-workspace"><WorkspaceSwitcher compact onselect={onworkspaceselect} /></div>
	<div class="titlebar-actions">
		{#if session.activeChat.workspaceScope}
			<button class="chrome-action" aria-label={session.workspacePaneOpen ? 'Close the panel' : 'Open the panel'} title={session.workspacePaneOpen ? 'Close the panel' : 'Open the panel'} aria-expanded={session.workspacePaneOpen} onclick={() => session.setContentLayout(session.workspacePaneOpen ? 'chat' : 'notebook')}><Icon name="panel-right" size={14} /></button>
		{/if}
		<DropdownMenu.Root>
			<DropdownMenu.Trigger class="chrome-action" aria-label="Workspace actions" title="Workspace actions"><Icon name="more-horizontal" size={16} /></DropdownMenu.Trigger>
			<DropdownMenu.Portal><DropdownMenu.Content class="fella-ui-menu-content" align="end" sideOffset={6}>
				<DropdownMenu.Item class="fella-ui-menu-item" onSelect={onpalette}><Icon name="search" size={14} />Search Fella</DropdownMenu.Item>
				<DropdownMenu.Item class="fella-ui-menu-item" onSelect={() => { session.setContentLayout('chat'); if (expanded) ontoggleexpand?.(); }}><Icon name="ask" size={14} />Chat view</DropdownMenu.Item>
				<DropdownMenu.Item class="fella-ui-menu-item" onSelect={() => session.setContentLayout('notebook')}><Icon name="notebook-pen" size={14} />Notebook view</DropdownMenu.Item>
				{#if session.catalog.workspace}
					<DropdownMenu.Item class="fella-ui-menu-item" onSelect={() => session.openWorkspaceToolChooser()}><Icon name="plus" size={14} />Open another tool</DropdownMenu.Item>
				{/if}
				{#if session.workspacePaneOpen && !compact}
					<DropdownMenu.Item class="fella-ui-menu-item" onSelect={() => ontoggleexpand?.()}><Icon name={expanded ? 'columns-2' : 'arrow-up-right'} size={14} />{expanded ? 'Restore split view' : 'Expand panel'}</DropdownMenu.Item>
				{/if}
			</DropdownMenu.Content></DropdownMenu.Portal>
		</DropdownMenu.Root>
	</div>
</header>

<style>
	.titlebar { position:absolute; inset:0 0 auto; z-index:15; display:flex; height:56px; align-items:flex-start; justify-content:center; padding:12px 20px 0; pointer-events:none; user-select:none; -webkit-app-region:drag; }
	.titlebar-workspace { min-width:0; max-width:calc(100% - 140px); pointer-events:auto; -webkit-app-region:no-drag; }
	.titlebar-actions { position:absolute; top:14px; right:12px; display:flex; gap:2px; pointer-events:auto; -webkit-app-region:no-drag; }
	:global(.chrome-action) { display:grid; width:28px; height:28px; place-items:center; padding:0; border:0; border-radius:50%; background:transparent; color:var(--text-faint); cursor:pointer; }
	:global(.chrome-action:hover), :global(.chrome-action[data-state='open']) { background:var(--bg-raised); color:var(--text); }
</style>

<script lang="ts">
	import { openWorkspaceTerminal } from '$lib/ipc';
	import { session } from '$lib/session.svelte';
	import { Button } from '$lib/components/ui/button';

	let workspacePath = $derived(session.catalog.workspace);
	let opening = $state(false);
	let opened = $state(false);
	let error = $state('');

	async function openTerminal(): Promise<void> {
		const path = workspacePath;
		if (!path || opening) return;
		opening = true;
		opened = false;
		error = '';
		try {
			await openWorkspaceTerminal(path);
			opened = true;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : String(cause);
		} finally {
			opening = false;
		}
	}
</script>

<section class="terminal-view" aria-labelledby="terminal-title">
	<div class="terminal-content">
		<div class="terminal-mark" aria-hidden="true">›_</div>
		<h1 id="terminal-title">Terminal</h1>
		<p class="terminal-description">
			Open your system terminal at this workspace. It runs with your normal system permissions, including access outside this folder.
		</p>
		{#if workspacePath}
			<p class="terminal-path" title={workspacePath}>{workspacePath}</p>
			<Button type="button" disabled={opening} onclick={() => void openTerminal()}>
				{opening ? 'Opening…' : opened ? 'Open another terminal' : 'Open terminal'}
			</Button>
		{:else}
			<p class="terminal-unavailable">Open a workspace to start a terminal there.</p>
		{/if}
		<p class="terminal-boundary">
			The terminal runs outside Fella. Fella cannot read its commands or output, or control the session.
		</p>
		{#if opened}<p class="terminal-status" role="status">Terminal opened at this workspace.</p>{/if}
		{#if error}<p class="terminal-error" role="alert">{error}</p>{/if}
	</div>
</section>

<style>
	.terminal-view {
		display: grid;
		flex: 1;
		min-width: 0;
		min-height: 0;
		place-items: center;
		padding: clamp(24px, 5vw, 56px);
		overflow: auto;
	}
	.terminal-content { width: min(100%, 480px); }
	.terminal-mark {
		display: grid;
		place-items: center;
		width: 40px;
		height: 40px;
		margin-bottom: var(--space-4);
		border-radius: var(--radius-md);
		background: var(--bg-inset);
		color: var(--text-dim);
		font-family: var(--mono);
		font-size: var(--fs-sm);
		font-weight: 650;
	}
	h1 { margin: 0; font-size: var(--fs-xl); font-weight: 620; letter-spacing: -0.025em; }
	.terminal-description { margin: var(--space-2) 0 var(--space-4); color: var(--text-dim); line-height: 1.55; }
	.terminal-path {
		margin: 0 0 var(--space-4);
		padding: 10px 12px;
		border-radius: var(--radius-md);
		background: var(--bg-inset);
		color: var(--text);
		font-family: var(--mono);
		font-size: var(--fs-xs);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.terminal-boundary, .terminal-unavailable {
		margin: var(--space-3) 0 0;
		color: var(--text-faint);
		font-size: var(--fs-xs);
		line-height: 1.5;
	}
	.terminal-status { margin: var(--space-3) 0 0; color: var(--text-dim); font-size: var(--fs-sm); }
	.terminal-error { margin: var(--space-3) 0 0; color: var(--danger, #b42318); font-size: var(--fs-sm); overflow-wrap: anywhere; }
</style>

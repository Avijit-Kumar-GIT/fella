<script lang="ts">
	import { baseName, openFolder } from '$lib/commands';
	import { session } from '$lib/session.svelte';
	import Icon from './Icon.svelte';

	let { open = $bindable(false) }: { open?: boolean } = $props();
	let name = $state('');
	let workspace = $state('');
	let nameInput = $state<HTMLInputElement>();
	let wasOpen = $state(false);

	let repositories = $derived.by(() => {
		const paths = [...session.repositoryPaths];
		if (session.catalog.workspace && !paths.includes(session.catalog.workspace)) {
			paths.unshift(session.catalog.workspace);
		}
		return paths.map((path) => ({ path, name: baseName(path) }));
	});

	$effect(() => {
		if (open && !wasOpen) {
			name = '';
			workspace = session.catalog.workspace ?? repositories[0]?.path ?? '';
			queueMicrotask(() => nameInput?.focus());
		}
		wasOpen = open;
	});

	function close(): void {
		open = false;
	}

	function create(): void {
		if (!name.trim() || !workspace) return;
		session.createProject(name, workspace);
		close();
	}

	async function chooseRepository(): Promise<void> {
		await openFolder();
		workspace = session.catalog.workspace ?? workspace;
	}

	function key(event: KeyboardEvent): void {
		if (!open) return;
		if (event.key === 'Escape') {
			event.preventDefault();
			close();
		}
	}
</script>

<svelte:window onkeydown={key} />

{#if open}
	<div class="scrim">
		<button class="backdrop" type="button" aria-label="Close new project" onclick={close}></button>
		<form class="dialog" aria-label="New project" onsubmit={(event) => { event.preventDefault(); create(); }}>
			<div class="dialog-heading">
				<span class="dialog-mark"><Icon name="bookmark" size={20} /></span>
				<div>
					<p class="eyebrow">Projects</p>
					<h1>New project</h1>
				</div>
			</div>

			<label>
				<span>Project name</span>
				<input bind:this={nameInput} bind:value={name} placeholder="e.g. Q3 planning" autocomplete="off" />
			</label>

			<label>
				<span>Repository</span>
				{#if repositories.length}
					<select bind:value={workspace}>
						{#each repositories as repository (repository.path)}
							<option value={repository.path}>{repository.name}</option>
						{/each}
					</select>
				{:else}
					<div class="repository-empty">
						<span>Mount a repository to give this project a home.</span>
						<button class="pill ghost" type="button" onclick={() => void chooseRepository()}>
							<Icon name="folder" size={16} /> Open repository
						</button>
					</div>
				{/if}
			</label>

			<div class="dialog-actions">
				<button class="pill ghost" type="button" onclick={close}>Cancel</button>
				<button class="pill primary" type="submit" disabled={!name.trim() || !workspace}>Create project</button>
			</div>
		</form>
	</div>
{/if}

<style>
	.scrim {
		position: fixed;
		inset: 0;
		z-index: 60;
		display: grid;
		place-items: start center;
		padding-top: 15vh;
	}
	.backdrop {
		position: fixed;
		inset: 0;
		background: color-mix(in srgb, var(--bg) 58%, transparent);
		cursor: default;
	}
	.dialog {
		position: relative;
		z-index: 1;
		width: min(440px, calc(100vw - 32px));
		display: grid;
		gap: var(--space-4);
		padding: 20px;
		background: var(--bg-raised);
		border: 1px solid var(--border);
		border-radius: 12px;
		box-shadow: var(--shadow-pop);
	}
	.dialog-heading {
		display: flex;
		align-items: center;
		gap: var(--space-3);
	}
	.dialog-mark {
		display: grid;
		place-items: center;
		width: 36px;
		height: 36px;
		border-radius: var(--radius-sm);
		background: color-mix(in srgb, var(--brand) 12%, var(--bg-inset));
		color: var(--brand);
	}
	.eyebrow {
		margin: 0;
		color: var(--text-faint);
		font-size: var(--fs-xs);
		font-weight: 650;
		letter-spacing: 0.03em;
		text-transform: uppercase;
	}
	h1 {
		margin: 1px 0 0;
		font-size: var(--fs-lg);
		font-weight: 650;
		letter-spacing: -0.02em;
	}
	label {
		display: grid;
		gap: 6px;
		color: var(--text-dim);
		font-size: var(--fs-sm);
	}
	input,
	select {
		width: 100%;
		min-height: 34px;
		padding: 7px 9px;
		border: 1px solid var(--border-strong);
		border-radius: var(--radius-sm);
		background: var(--bg);
		color: var(--text);
		font: inherit;
		outline: none;
	}
	input:focus,
	select:focus {
		border-color: var(--link);
		box-shadow: var(--focus-ring);
	}
	.repository-empty {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-3);
		padding: 10px;
		border: 1px dashed var(--border-strong);
		border-radius: var(--radius-sm);
		color: var(--text-faint);
	}
	.repository-empty .pill {
		flex: none;
		padding: 6px 9px;
		font-size: var(--fs-sm);
	}
	.dialog-actions {
		display: flex;
		justify-content: flex-end;
		gap: var(--space-2);
		padding-top: var(--space-1);
	}
	.dialog-actions .pill {
		padding: 7px 12px;
	}
	.dialog-actions .pill:disabled {
		cursor: default;
		opacity: 0.45;
	}
</style>

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
	let selectedRepository = $derived(repositories.find((repository) => repository.path === workspace));

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
		<button class="backdrop" type="button" aria-label="Close project dialog" onclick={close}></button>
		<form class="dialog" aria-labelledby="project-dialog-title" aria-describedby="project-dialog-description" onsubmit={(event) => { event.preventDefault(); create(); }}>
			<div class="dialog-heading">
				<div>
					<p class="eyebrow"><Icon name="bookmark" size={12} /> Project</p>
					<h1 id="project-dialog-title">Create a project</h1>
					<p id="project-dialog-description" class="dialog-description">Keep notes and context alongside one repository.</p>
				</div>
				<button class="close-button" type="button" aria-label="Close project dialog" title="Close" onclick={close}>
					<Icon name="x" size={16} />
				</button>
			</div>

			<div class="fields">
				<div class="field">
					<label for="project-name">Name</label>
					<input id="project-name" bind:this={nameInput} bind:value={name} placeholder="e.g. Q3 planning" autocomplete="off" />
				</div>

				<div class="field">
					<label for="project-repository">Repository</label>
					{#if repositories.length}
						<select id="project-repository" bind:value={workspace}>
							{#each repositories as repository (repository.path)}
								<option value={repository.path}>{repository.name}</option>
							{/each}
						</select>
						{#if selectedRepository}
							<p class="field-hint" title={selectedRepository.path}>{selectedRepository.path}</p>
						{/if}
					{:else}
						<div class="repository-empty">
							<span>No repository mounted.</span>
							<button class="pill ghost" type="button" onclick={() => void chooseRepository()}>
								<Icon name="folder" size={16} /> Mount repository
							</button>
						</div>
					{/if}
				</div>
			</div>

			<div class="dialog-note">
				<Icon name="info" size={12} /> <span>Projects are saved locally on this computer.</span>
			</div>

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
		padding: 10vh 16px 24px;
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
		width: min(460px, 100%);
		display: grid;
		gap: 20px;
		padding: 24px;
		background: var(--bg-raised);
		border: 1px solid var(--border);
		border-radius: var(--radius);
		box-shadow: var(--shadow-pop);
	}
	.dialog-heading {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: var(--space-4);
	}
	.dialog-heading > div {
		min-width: 0;
	}
	.eyebrow {
		display: flex;
		align-items: center;
		gap: 5px;
		margin: 0 0 var(--space-1);
		color: var(--brand);
		font-size: var(--fs-xs);
		font-weight: 650;
		letter-spacing: 0.04em;
		text-transform: uppercase;
	}
	h1 {
		margin: 0;
		font-size: 20px;
		font-weight: 650;
		letter-spacing: -0.025em;
	}
	.dialog-description {
		margin: var(--space-2) 0 0;
		color: var(--text-dim);
		font-size: var(--fs-sm);
	}
	.close-button {
		display: grid;
		place-items: center;
		width: 26px;
		height: 26px;
		flex: none;
		border-radius: var(--radius-chip);
		color: var(--text-faint);
	}
	.close-button:hover {
		background: var(--bg-inset);
		color: var(--text);
	}
	.fields {
		display: grid;
		gap: var(--space-4);
	}
	.field {
		display: grid;
		gap: 6px;
		min-width: 0;
	}
	label {
		color: var(--text-dim);
		font-size: var(--fs-sm);
		font-weight: 600;
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
	.field-hint {
		margin: -1px 0 0;
		overflow: hidden;
		color: var(--text-faint);
		font-size: var(--fs-xs);
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.repository-empty {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-3);
		padding: 8px 10px;
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		background: var(--bg-inset);
		color: var(--text-faint);
	}
	.repository-empty .pill {
		flex: none;
		padding: 6px 9px;
		font-size: var(--fs-sm);
	}
	.dialog-note {
		display: flex;
		align-items: center;
		gap: 6px;
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.dialog-note :global(svg) {
		flex: none;
	}
	.dialog-actions {
		display: flex;
		justify-content: flex-end;
		gap: var(--space-2);
		padding-top: var(--space-4);
		border-top: 1px solid var(--border);
	}
	.dialog-actions .pill {
		padding: 7px 12px;
	}
	.dialog-actions .pill:disabled {
		cursor: default;
		opacity: 0.45;
	}
</style>

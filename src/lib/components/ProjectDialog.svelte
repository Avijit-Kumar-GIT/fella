<script lang="ts">
	import { baseName, openFolder } from '$lib/commands';
	import { session } from '$lib/session.svelte';
	import { Button } from '$lib/components/ui/button';
	import { Dialog } from '$lib/components/ui/dialog';
	import { Input } from '$lib/components/ui/input';
	import { Select } from '$lib/components/ui/select';
	import Icon from './Icon.svelte';

	let { open = $bindable(false), initialWorkspace = null }: { open?: boolean; initialWorkspace?: string | null } = $props();
	let name = $state('');
	let workspace = $state('');
	let suggestedName = $state('');
	let nameInput = $state<HTMLInputElement | null>(null);
	let wasOpen = $state(false);

	let allRepositories = $derived.by(() => {
		const paths = [...session.repositoryPaths];
		if (session.catalog.workspace && !paths.includes(session.catalog.workspace)) paths.unshift(session.catalog.workspace);
		return paths.map((path) => ({ path, name: baseName(path) }));
	});
	let repositories = $derived(allRepositories.filter((repository) => !session.projectForWorkspace(repository.path)));
	let selectedRepository = $derived(repositories.find((repository) => repository.path === workspace));
	let hasMountedRepositories = $derived(allRepositories.length > 0);

	$effect(() => {
		if (open && !wasOpen) {
			workspace = repositories.find((repository) => repository.path === initialWorkspace)?.path ?? repositories[0]?.path ?? '';
			suggestedName = workspace ? baseName(workspace) : '';
			name = suggestedName;
		}
		wasOpen = open;
	});

	function close(): void {
		open = false;
	}

	function create(): void {
		if (!name.trim() || !workspace || !repositories.some((repository) => repository.path === workspace)) return;
		session.createProject(name, workspace);
		close();
	}

	function selectRepository(path: string): void {
		const nextName = baseName(path);
		if (!name.trim() || name === suggestedName) name = nextName;
		suggestedName = nextName;
		workspace = path;
	}

	async function chooseRepository(): Promise<void> {
		await openFolder();
		const next = session.catalog.workspace;
		const available = (next && repositories.find((repository) => repository.path === next)) ?? repositories[0];
		if (available) selectRepository(available.path);
		else {
			workspace = '';
			name = '';
			suggestedName = '';
		}
	}
</script>

<Dialog.Root bind:open>
	<Dialog.Portal>
		<Dialog.Overlay class="fella-ui-dialog-overlay" />
		<Dialog.Content
			class="fella-ui-dialog-content project-dialog"
			onOpenAutoFocus={(event) => {
				event.preventDefault();
				queueMicrotask(() => nameInput?.focus());
			}}
		>
			<form class="project-form" onsubmit={(event) => { event.preventDefault(); create(); }}>
				<div class="dialog-heading">
					<div>
						<p class="eyebrow"><Icon name="project" size={12} /> Project</p>
						<Dialog.Title class="dialog-title" level={1}>Create a project</Dialog.Title>
						<Dialog.Description class="dialog-description">Keep notes and context alongside one repository.</Dialog.Description>
					</div>
					<Dialog.Close class="close-button" type="button" aria-label="Close project dialog" title="Close">
						<Icon name="x" size={16} />
					</Dialog.Close>
				</div>

				<div class="fields">
					<div class="field">
						<label for="project-name">Name</label>
						<Input id="project-name" bind:ref={nameInput} bind:value={name} placeholder="e.g. Q3 planning" autocomplete="off" />
					</div>

					<div class="field">
						<label for="project-repository">Repository</label>
						{#if repositories.length}
							<Select
								id="project-repository"
								value={workspace}
								onchange={(event) => selectRepository((event.currentTarget as HTMLSelectElement).value)}
							>
								{#each repositories as repository (repository.path)}
									<option value={repository.path}>{repository.name}</option>
								{/each}
							</Select>
							{#if selectedRepository}
								<p class="field-hint" title={selectedRepository.path}>{selectedRepository.path}</p>
							{/if}
						{:else}
							<div class="repository-empty">
								{#if hasMountedRepositories}
									<span>Each repository already has a project.</span>
								{:else}
									<span>No repository mounted.</span>
									<Button variant="outline" size="sm" type="button" onclick={() => void chooseRepository()}>
										<Icon name="folder" size={16} /> Mount repository
									</Button>
								{/if}
							</div>
						{/if}
					</div>
				</div>

				<div class="dialog-note">
					<Icon name="info" size={12} /> <span>Projects are saved locally on this computer.</span>
				</div>

				<div class="dialog-actions">
					<Button variant="ghost" type="button" onclick={close}>Cancel</Button>
					<Button
						type="submit"
						disabled={!name.trim() || !workspace || !repositories.some((repository) => repository.path === workspace)}
					>
						Create project
					</Button>
				</div>
			</form>
		</Dialog.Content>
	</Dialog.Portal>
</Dialog.Root>

<style>
	:global(.project-dialog) {
		width: min(460px, calc(100vw - 32px));
		padding: 24px;
	}
	.project-form {
		display: grid;
		gap: 20px;
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
		color: var(--text-faint);
		font-size: var(--fs-xs);
		font-weight: 650;
		letter-spacing: 0.04em;
		text-transform: uppercase;
	}
	:global(.dialog-title) {
		margin: 0;
		font-size: 20px;
		font-weight: 650;
		letter-spacing: -0.025em;
	}
	:global(.dialog-description) {
		margin: var(--space-2) 0 0;
		color: var(--text-dim);
		font-size: var(--fs-sm);
	}
	:global(.close-button) {
		display: grid;
		place-items: center;
		width: 28px;
		height: 28px;
		flex: none;
		border-radius: var(--radius-chip);
		color: var(--text-faint);
	}
	:global(.close-button:hover) {
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
	.field label {
		color: var(--text-dim);
		font-size: var(--fs-sm);
		font-weight: 600;
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
	.repository-empty :global(button) {
		flex: none;
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
</style>

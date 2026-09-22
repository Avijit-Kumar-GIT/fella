<script lang="ts">
	import { baseName, openFolder } from '$lib/commands';
	import { ipc, isTauri } from '$lib/ipc';
	import { session } from '$lib/session.svelte';
	import type { ConversationSummary } from '$lib/types';
	import Icon from './Icon.svelte';

	let project = $derived(session.activeProject);
	let name = $state('');
	let body = $state('');
	let loadedId = $state<string | null>(null);
	let history = $state<ConversationSummary[]>([]);
	let historyRequest = 0;

	$effect(() => {
		const current = project;
		if (!current) {
			loadedId = null;
			return;
		}
		if (loadedId !== current.id) {
			loadedId = current.id;
			name = current.name;
			body = current.body;
		}
	});

	$effect(() => {
		const workspace = project?.workspace;
		session.historyVersion;
		if (!workspace || !isTauri()) {
			history = [];
			return;
		}
		void loadHistory(workspace);
	});

	let mounted = $derived(Boolean(project && project.workspace === session.catalog.workspace));
	let sourceCount = $derived(mounted ? session.catalog.sources.length : null);
	let tableCount = $derived(
		mounted
			? session.catalog.sources.filter((source) =>
					['csv', 'tsv', 'parquet', 'xlsx', 'json', 'ndjson'].includes(source.kind)
				).length
			: null
	);

	async function loadHistory(workspace: string): Promise<void> {
		const request = ++historyRequest;
		try {
			const next = await ipc.conversationsList();
			if (request === historyRequest) history = next.filter((item) => item.workspace === workspace);
		} catch {
			if (request === historyRequest) history = [];
		}
	}

	function commitName(): void {
		if (!project) return;
		const next = name.trim();
		if (!next) {
			name = project.name;
			return;
		}
		if (next !== project.name) session.updateProject(project.id, { name: next });
	}

	function updateBody(event: Event): void {
		const next = (event.currentTarget as HTMLTextAreaElement).value;
		body = next;
		if (project) session.updateProject(project.id, { body: next });
	}

	async function mountRepository(): Promise<void> {
		if (project) await openFolder(project.workspace);
	}

	async function askRepository(): Promise<void> {
		if (!mounted) await mountRepository();
		if (!session.catalog.workspace || session.catalog.workspace !== project?.workspace) return;
		session.setWorkspaceView('ask');
		session.newTab();
	}

	async function openSources(): Promise<void> {
		if (!mounted) await mountRepository();
		if (!session.catalog.workspace || session.catalog.workspace !== project?.workspace) return;
		session.setWorkspacePane('sources');
	}

	function deleteProject(): void {
		if (!project) return;
		if (confirm(`Delete the local project “${project.name}”?`)) session.deleteProject(project.id);
	}

	function updatedLabel(value: number): string {
		return new Date(value).toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: 'numeric' });
	}
</script>

{#if project}
	<section class="project-page" aria-labelledby="project-title">
		<header class="project-header">
			<div class="project-heading">
				<span class="project-mark"><Icon name="bookmark" size={20} /></span>
				<div class="project-title-wrap">
					<p class="eyebrow">Project</p>
					<input id="project-title" class="project-title" bind:value={name} onblur={commitName} aria-label="Project name" />
				</div>
			</div>
			<div class="project-actions">
				<button class="pill ghost" type="button" onclick={() => void askRepository()}>
					<Icon name="compose" size={16} /> Ask repository
				</button>
				<button class="pill ghost" type="button" onclick={() => void openSources()}>
					<Icon name="table" size={16} /> Sources
				</button>
				<button class="icon-action" type="button" aria-label="Delete project" title="Delete project" onclick={deleteProject}>
					<Icon name="x" size={16} />
				</button>
			</div>
		</header>

		<div class="project-location" title={project.workspace}>
			<Icon name="folder" size={12} />
			<span>{baseName(project.workspace)}</span>
			<span class="location-separator">·</span>
			<span>{project.workspace}</span>
		</div>

		{#if !mounted}
			<div class="mount-note">
				<Icon name="info" size={16} />
				<span>This project is saved locally. Mount <strong>{baseName(project.workspace)}</strong> to see its live files and ask questions.</span>
				<button class="pill ghost" type="button" onclick={() => void mountRepository()}>Mount repository</button>
			</div>
		{/if}

		<div class="project-grid">
			<section class="wiki-card" aria-labelledby="wiki-title">
				<div class="card-heading">
					<div>
						<h1 id="wiki-title">Wiki</h1>
						<p>A private set of notes for this repository.</p>
					</div>
					<span class="saved-label">Saved locally</span>
				</div>
				<textarea
					value={body}
					oninput={updateBody}
					placeholder="Capture decisions, definitions, recurring questions, and anything you want Fella to remember about this workspace."
					aria-label="Project wiki"
				></textarea>
			</section>

			<aside class="snapshot-card" aria-labelledby="snapshot-title">
				<div class="card-heading">
					<div>
						<h1 id="snapshot-title">Workspace snapshot</h1>
						<p>Useful context at a glance.</p>
					</div>
				</div>
				<dl>
					<div><dt>Files</dt><dd>{sourceCount ?? '—'}</dd></div>
					<div><dt>Tables</dt><dd>{tableCount ?? '—'}</dd></div>
					<div><dt>Conversations</dt><dd>{history.length || '—'}</dd></div>
					<div><dt>Updated</dt><dd>{updatedLabel(project.updated_at_ms)}</dd></div>
				</dl>
				{#if !mounted}
					<button class="pill ghost snapshot-action" type="button" onclick={() => void mountRepository()}>
						<Icon name="folder" size={16} /> Mount repository
					</button>
				{/if}
			</aside>
		</div>
	</section>
{/if}

<style>
	.project-page {
		flex: 1;
		min-width: 0;
		min-height: 0;
		overflow-y: auto;
		padding: 42px var(--pad) 56px;
	}
	.project-page > * {
		width: min(100%, var(--content-max));
		margin-inline: auto;
	}
	.project-header {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: var(--space-5);
	}
	.project-heading {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		min-width: 0;
	}
	.project-mark {
		display: grid;
		place-items: center;
		width: 42px;
		height: 42px;
		flex: none;
		border-radius: var(--radius);
		background: color-mix(in srgb, var(--brand) 13%, var(--bg-inset));
		color: var(--brand);
	}
	.project-title-wrap {
		min-width: 0;
	}
	.eyebrow {
		margin: 0;
		color: var(--text-faint);
		font-size: var(--fs-xs);
		font-weight: 650;
		letter-spacing: 0.04em;
		text-transform: uppercase;
	}
	.project-title {
		width: min(440px, 100%);
		margin-top: 1px;
		padding: 0;
		border: 0;
		background: transparent;
		color: var(--text);
		font: inherit;
		font-size: 24px;
		font-weight: 650;
		letter-spacing: -0.03em;
		outline: none;
	}
	.project-title:focus {
		box-shadow: inset 0 -1px var(--link);
	}
	.project-actions {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		flex: none;
	}
	.project-actions .pill {
		padding: 7px 10px;
	}
	.icon-action {
		display: grid;
		place-items: center;
		width: 30px;
		height: 30px;
		border-radius: var(--radius-sm);
		color: var(--text-faint);
	}
	.icon-action:hover {
		background: color-mix(in srgb, var(--err) 10%, var(--bg-inset));
		color: var(--err);
	}
	.project-location {
		display: flex;
		align-items: center;
		gap: 6px;
		margin-top: var(--space-2);
		color: var(--text-faint);
		font-size: var(--fs-sm);
		overflow: hidden;
		white-space: nowrap;
	}
	.project-location span:last-child {
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.project-location :global(svg) {
		flex: none;
		color: var(--brand);
	}
	.location-separator {
		color: var(--border-strong);
	}
	.mount-note {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		margin-top: var(--space-5);
		padding: 10px 12px;
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		background: var(--bg-inset);
		color: var(--text-dim);
		font-size: var(--fs-sm);
	}
	.mount-note > :global(svg) {
		flex: none;
		color: var(--brand);
	}
	.mount-note span {
		min-width: 0;
		flex: 1;
	}
	.mount-note strong {
		color: var(--text);
		font-weight: 600;
	}
	.mount-note .pill {
		flex: none;
		padding: 6px 10px;
	}
	.project-grid {
		display: grid;
		grid-template-columns: minmax(0, 1fr) minmax(220px, 280px);
		gap: var(--space-4);
		margin-top: var(--space-5);
	}
	.wiki-card,
	.snapshot-card {
		min-width: 0;
		padding: 18px;
		border: 1px solid var(--border);
		border-radius: var(--radius);
		background: var(--bg-raised);
	}
	.wiki-card {
		display: flex;
		flex-direction: column;
		min-height: 420px;
	}
	.card-heading {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: var(--space-3);
		margin-bottom: var(--space-4);
	}
	h1 {
		margin: 0;
		font-size: var(--fs-lg);
		font-weight: 650;
		letter-spacing: -0.02em;
	}
	.card-heading p {
		margin: 3px 0 0;
		color: var(--text-faint);
		font-size: var(--fs-sm);
	}
	.saved-label {
		flex: none;
		padding-top: 2px;
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	textarea {
		flex: 1;
		width: 100%;
		min-height: 300px;
		resize: vertical;
		padding: 0;
		border: 0;
		background: transparent;
		color: var(--text);
		font: inherit;
		line-height: 1.65;
		outline: none;
	}
	textarea::placeholder {
		color: var(--text-faint);
	}
	.snapshot-card {
		align-self: start;
	}
	dl {
		display: grid;
		gap: 0;
		margin: var(--space-4) 0 0;
	}
	dl div {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: var(--space-3);
		padding: 9px 0;
		border-top: 1px solid var(--border);
	}
	dt {
		color: var(--text-faint);
		font-size: var(--fs-sm);
	}
	dd {
		margin: 0;
		color: var(--text);
		font-size: var(--fs-sm);
		font-variant-numeric: tabular-nums;
	}
	.snapshot-action {
		width: 100%;
		margin-top: var(--space-3);
	}
	@media (max-width: 760px) {
		.project-page {
			padding: var(--space-5) var(--space-4) 40px;
		}
		.project-header {
			align-items: flex-start;
			flex-direction: column;
		}
		.project-actions {
			width: 100%;
		}
		.project-actions .pill {
			flex: 1;
		}
		.project-grid {
			grid-template-columns: 1fr;
		}
	}
</style>

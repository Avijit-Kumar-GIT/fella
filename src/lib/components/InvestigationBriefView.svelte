<script lang="ts">
	import { firstActualQuestion, session } from '$lib/session.svelte';
	import { Button } from '$lib/components/ui/button';
	import { Textarea } from '$lib/components/ui/textarea';
	import { ipc, isDesktop } from '$lib/ipc';
	import type { Investigation, InvestigationBrief } from '$lib/types';
	import Icon from './Icon.svelte';

	let investigations = $state<Investigation[]>([]);
	let selectedId = $state<string | null>(null);
	let workspacePath = $state<string | null>(null);
	let loadedWorkspace = $state<string | null>(null);
	let goal = $state('');
	let audience = $state('');
	let desiredOutput = $state('');
	let constraintsText = $state('');
	let savedBrief = $state('');
	let loading = $state(false);
	let creating = $state(false);
	let saving = $state(false);
	let error = $state('');
	let requestSerial = 0;

	let selected = $derived(investigations.find((item) => item.id === selectedId) ?? null);
	let currentBrief: InvestigationBrief = $derived({
		goal,
		audience,
		desired_output: desiredOutput,
		constraints: constraintsText.split('\n').map((item) => item.trim()).filter(Boolean)
	});
	let briefKey = $derived(JSON.stringify(currentBrief));
	let dirty = $derived(Boolean(selected && briefKey !== savedBrief));

	async function loadWorkspace(path: string): Promise<void> {
		const serial = ++requestSerial;
		loading = true;
		error = '';
		investigations = [];
		selectedId = null;
		try {
			const all = await ipc.investigationsList();
			if (serial !== requestSerial) return;
			investigations = all.filter((item) => item.workspace_id === path);
			selectedId = investigations[0]?.id ?? null;
		} catch (cause) {
			if (serial === requestSerial) error = cause instanceof Error ? cause.message : String(cause);
		} finally {
			if (serial === requestSerial) loading = false;
		}
	}

	$effect(() => {
		const path = session.catalog.workspace;
		workspacePath = path;
		if (!path) {
			requestSerial += 1;
			loadedWorkspace = null;
			investigations = [];
			selectedId = null;
			loading = false;
			return;
		}
		if (path === loadedWorkspace) return;
		loadedWorkspace = path;
		void loadWorkspace(path);
	});

	$effect(() => {
		const item = selected;
		if (!item) {
			goal = '';
			audience = '';
			desiredOutput = '';
			constraintsText = '';
			savedBrief = '';
			return;
		}
		goal = item.brief.goal;
		audience = item.brief.audience;
		desiredOutput = item.brief.desired_output;
		constraintsText = item.brief.constraints.join('\n');
		savedBrief = JSON.stringify(item.brief);
	});

	function titleFromRequest(request: string | undefined): string {
		if (!request) return 'New investigation';
		const singleLine = request.replace(/\s+/g, ' ').trim();
		return singleLine.length > 64 ? `${singleLine.slice(0, 61).trimEnd()}…` : singleLine;
	}

	async function createInvestigation(): Promise<void> {
		if (!workspacePath || creating || !isDesktop()) return;
		creating = true;
		error = '';
		const conversation = session.activeChat;
		const request = conversation.workspaceScope === workspacePath
			? firstActualQuestion(conversation.messages)
			: undefined;
		try {
			const created = await ipc.investigationCreate(
				workspacePath,
				titleFromRequest(request?.text),
				request?.text,
				request?.id
			);
			investigations = [created, ...investigations];
			selectedId = created.id;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : String(cause);
		} finally {
			creating = false;
		}
	}

	async function saveBrief(): Promise<boolean> {
		if (!selected || saving || !dirty) return !dirty;
		saving = true;
		error = '';
		try {
			const updated = await ipc.investigationUpdateBrief(selected.id, currentBrief);
			investigations = investigations.map((item) => item.id === updated.id ? updated : item);
			savedBrief = JSON.stringify(updated.brief);
			return true;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : String(cause);
			return false;
		} finally {
			saving = false;
		}
	}

	async function selectInvestigation(id: string): Promise<void> {
		if (id === selectedId || saving) return;
		if (dirty && !(await saveBrief())) return;
		selectedId = id;
	}
</script>

<section class="investigation-page" aria-labelledby="investigation-title">
	<header class="page-head">
		<div>
			<h1 id="investigation-title">Investigation brief</h1>
			<p>Keep the goal, audience, deliverable, and constraints clear as the analysis develops.</p>
		</div>
		{#if workspacePath}
			<Button type="button" disabled={loading || creating} onclick={() => void createInvestigation()}>
				<Icon name="plus" size={16} />{creating ? 'Starting…' : 'New investigation'}
			</Button>
		{/if}
	</header>
	{#if error && !selected}
		<p class="error" role="alert">{error}</p>
	{/if}

	{#if !workspacePath}
		<div class="empty-state">
			<Icon name="folder" size={20} />
			<h2>Open a workspace to start an Investigation</h2>
			<p>Investigations are scoped to a folder so their intent stays alongside the data being analyzed.</p>
		</div>
	{:else if loading}
		<p class="status" role="status">Loading investigations…</p>
	{:else if !investigations.length}
		<div class="empty-state">
			<Icon name="bookmark" size={20} />
			<h2>No investigations yet</h2>
			<p>Start one from this workspace. If the current conversation has a question, its wording seeds the goal.</p>
			<Button type="button" disabled={creating} onclick={() => void createInvestigation()}>
				<Icon name="plus" size={16} />{creating ? 'Starting…' : 'Start from this workspace'}
			</Button>
		</div>
	{:else}
		<div class="investigation-layout">
			<nav class="investigation-list" aria-label="Workspace investigations">
				{#each investigations as item (item.id)}
					<button
						class="investigation-choice"
						class:selected={item.id === selectedId}
						type="button"
						aria-current={item.id === selectedId ? 'true' : undefined}
						disabled={saving}
						onclick={() => void selectInvestigation(item.id)}
					>
						<span>{item.title}</span>
						<small>{new Date(item.updated_at_ms).toLocaleDateString()}</small>
					</button>
				{/each}
			</nav>

			{#if selected}
				<div class="brief-editor">
					<div class="editor-heading">
						<div>
							<h2>{selected.title}</h2>
							{#if selected.brief_source_message_id}
								<p>Goal seeded from the conversation’s first request. Editing this brief does not change that message.</p>
							{/if}
						</div>
						<Button type="button" disabled={!dirty || saving} onclick={() => void saveBrief()}>
							<Icon name="check" size={16} />{saving ? 'Saving…' : 'Save brief'}
						</Button>
					</div>
					<label for="investigation-goal">Goal</label>
					<Textarea id="investigation-goal" class="brief-field goal-field" bind:value={goal} disabled={saving} placeholder="What question should this Investigation answer?" />
					<label for="investigation-audience">Audience</label>
					<Textarea id="investigation-audience" class="brief-field audience-field" bind:value={audience} disabled={saving} placeholder="Who needs this analysis?" />
					<label for="investigation-output">Desired output</label>
					<Textarea id="investigation-output" class="brief-field" bind:value={desiredOutput} disabled={saving} placeholder="What should the analyst deliver?" />
					<label for="investigation-constraints">Constraints</label>
					<Textarea id="investigation-constraints" class="brief-field constraints-field" bind:value={constraintsText} disabled={saving} placeholder="One constraint per line" />
					<div class="editor-foot">
						<span role={error ? 'alert' : 'status'}>{error || (saving ? 'Saving…' : dirty ? 'Unsaved changes' : 'Saved locally')}</span>
						{#if selected.brief_source_message_id}<span>Original request preserved</span>{/if}
					</div>
				</div>
			{/if}
		</div>
	{/if}
</section>

<style>
	.investigation-page {
		flex: 1;
		min-height: 0;
		width: 100%;
		max-width: var(--content-max);
		margin: 0 auto;
		padding: var(--space-6) var(--pad);
		overflow: auto;
	}
	.page-head,
	.editor-heading,
	.editor-foot {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-4);
	}
	.page-head { margin-bottom: var(--space-5); }
	h1, h2, p { margin: 0; }
	h1 { font-size: clamp(24px, 3vw, 32px); font-weight: 650; letter-spacing: -0.03em; }
	.page-head p, .editor-heading p { margin-top: var(--space-1); color: var(--text-dim); }
	.investigation-layout { display: grid; grid-template-columns: minmax(180px, 240px) minmax(0, 1fr); gap: var(--space-6); }
	.investigation-list { display: flex; flex-direction: column; gap: 4px; }
	.investigation-choice {
		display: flex; flex-direction: column; align-items: flex-start; gap: 3px;
		padding: 9px 11px; border: 1px solid transparent; border-radius: var(--radius-md);
		background: transparent; color: var(--text); text-align: left; cursor: pointer;
	}
	.investigation-choice:hover { background: var(--bg-inset); }
	.investigation-choice.selected { border-color: var(--border); background: var(--bg-inset); }
	.investigation-choice span { max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--fs-sm); font-weight: 600; }
	.investigation-choice small, .status, .editor-foot { color: var(--text-faint); font-size: var(--fs-xs); }
	.error { margin: 0 0 var(--space-3); color: var(--danger, #b42318); font-size: var(--fs-sm); overflow-wrap: anywhere; }
	.brief-editor { min-width: 0; border-block: 1px solid var(--border); }
	.editor-heading { padding: var(--space-3) 0; border-bottom: 1px solid var(--border); }
	h2 { font-size: var(--fs-lg); font-weight: 650; }
	.brief-editor label { display: block; margin: var(--space-4) 0 var(--space-2); font-size: var(--fs-sm); font-weight: 600; }
	.brief-editor :global(.brief-field) { width: 100%; min-height: 74px; resize: vertical; }
	.brief-editor :global(.goal-field) { min-height: 112px; }
	.brief-editor :global(.constraints-field) { min-height: 96px; }
	.editor-foot { padding: var(--space-3) 0; }
	.empty-state { max-width: 48ch; margin: 12vh auto 0; display: grid; justify-items: center; gap: var(--space-3); text-align: center; }
	.empty-state > :global(svg) { color: var(--text-faint); }
	.empty-state h2 { font-size: var(--fs-lg); }
	.empty-state p { color: var(--text-dim); }
	@media (max-width: 720px) {
		.investigation-layout { grid-template-columns: 1fr; gap: var(--space-4); }
		.investigation-list { max-height: 180px; overflow: auto; }
	}
</style>

<script lang="ts">
	import { Button } from '$lib/components/ui/button';
	import { session } from '$lib/session.svelte';
	import type { Investigation, InvestigationSourceSelection } from '$lib/types';

	let {
		workspacePath,
		investigation,
		questionId,
		selection,
		disabled = false,
		onChange
	}: {
		workspacePath: string;
		investigation: Investigation;
		questionId: string;
		selection?: InvestigationSourceSelection | null;
		disabled?: boolean;
		onChange: (selection: InvestigationSourceSelection | null) => void;
	} = $props();

	let catalogLoaded = $derived(session.catalog.workspace === workspacePath);
	let catalogSources = $derived(catalogLoaded ? session.catalog.sources : []);
	let catalogPaths = $derived([...new Set(catalogSources.map((source) => relativePath(source.path)))]);
	let availablePaths = $derived.by(() => {
		return investigation.source_selection.mode === 'all_sources'
			? catalogPaths
			: catalogPaths.filter((path) => investigation.source_selection.included_paths.includes(path));
	});
	let selectedPaths = $derived(new Set(selection?.included_paths ?? []));
	let effectiveCount = $derived(
		selection
			? [...selectedPaths].filter((path) => availablePaths.includes(path)).length
			: availablePaths.length
	);
	let missingPaths = $derived.by(() => {
		const requestedPaths = selection?.included_paths ?? (
			investigation.source_selection.mode === 'selected_sources'
				? investigation.source_selection.included_paths
				: []
		);
		return [...new Set(requestedPaths)].filter((path) => !catalogPaths.includes(path));
	});
	let outOfScopePaths = $derived(
		selection
			? [...selectedPaths].filter((path) => catalogPaths.includes(path) && !availablePaths.includes(path))
			: []
	);
	let scopeSummary = $derived.by(() => {
		const selectedLabel = selection
			? `${selectedPaths.size} selected`
			: 'inherit Investigation';
		return catalogLoaded
			? `Sources · ${selectedLabel} · ${effectiveCount} effective`
			: `Sources · ${selectedLabel} · catalog loading`;
	});

	function relativePath(path: string): string {
		const root = workspacePath.replace(/[/\\]+$/, '');
		if (path === root) return path;
		if (path.startsWith(`${root}/`) || path.startsWith(`${root}\\`)) {
			return path.slice(root.length + 1).replace(/\\/g, '/');
		}
		return path.replace(/\\/g, '/');
	}

	function chooseSpecificSources(): void {
		if (!availablePaths.length) return;
		onChange({ mode: 'selected_sources', included_paths: availablePaths });
	}

	function setIncluded(path: string, included: boolean): void {
		const next = new Set(
			(selection?.included_paths ?? []).filter((item) => availablePaths.includes(item))
		);
		if (included) next.add(path);
		else if (next.size > 1) next.delete(path);
		onChange({ mode: 'selected_sources', included_paths: [...next] });
	}

	function resetToCurrentScope(): void {
		if (!availablePaths.length) {
			onChange(null);
			return;
		}
		onChange({ mode: 'selected_sources', included_paths: availablePaths });
	}
</script>

<details class="question-source-scope">
	<summary>
		{scopeSummary}
	</summary>
	<div class="question-source-panel">
		<label class="question-source-choice">
			<input
				type="radio"
				name={`question-scope-${questionId}`}
				checked={!selection}
				disabled={disabled}
				onchange={() => onChange(null)}
			/>
			Inherit Investigation source set
		</label>
		<label class="question-source-choice">
			<input
				type="radio"
				name={`question-scope-${questionId}`}
				checked={Boolean(selection)}
				disabled={disabled || !availablePaths.length}
				onchange={chooseSpecificSources}
			/>
			Use a specific source set
		</label>
		{#if !selection && missingPaths.length}
			<p class="question-source-status" role="status">
				{missingPaths.length} Investigation-selected path{missingPaths.length === 1 ? ' is' : 's are'} not present in the current catalog and cannot be used until cataloged.
			</p>
			<ul class="question-source-paths" aria-label="Investigation paths missing from the current catalog">
				{#each missingPaths as path (path)}<li>{path}</li>{/each}
			</ul>
		{/if}
		{#if selection}
			{#if missingPaths.length}
				<p class="question-source-status" role="status">
					{missingPaths.length} selected path{missingPaths.length === 1 ? ' is' : 's are'} not present in the current catalog and cannot be used until cataloged. Check unavailable-file details for a skip reason.
				</p>
				<ul class="question-source-paths" aria-label="Question paths missing from the current catalog">
					{#each missingPaths as path (path)}<li>{path}</li>{/each}
				</ul>
			{/if}
			{#if outOfScopePaths.length}
				<p class="question-source-status" role="status">
					{outOfScopePaths.length} selected source{outOfScopePaths.length === 1 ? ' is' : 's are'} in the catalog but outside this Investigation’s scope.
				</p>
				<ul class="question-source-paths" aria-label="Question sources outside the Investigation scope">
					{#each outOfScopePaths as path (path)}<li>{path}</li>{/each}
				</ul>
			{/if}
			{#if (missingPaths.length || outOfScopePaths.length) && availablePaths.length}
				<Button type="button" variant="ghost" size="sm" disabled={disabled} onclick={resetToCurrentScope}>
					Reset to current Investigation sources
				</Button>
			{/if}
			{#if availablePaths.length}
				<ul class="question-source-list" aria-label={`Sources for Question ${questionId}`}>
					{#each availablePaths as path (path)}
						<li>
							<label>
								<input
									type="checkbox"
									checked={selectedPaths.has(path)}
									disabled={disabled || (selectedPaths.has(path) && selectedPaths.size === 1)}
									onchange={(event) => setIncluded(path, event.currentTarget.checked)}
								/>
								<span>{path}</span>
							</label>
						</li>
					{/each}
				</ul>
			{:else}
				<p role="status">No files are currently available inside this Investigation's source set.</p>
				<Button type="button" variant="ghost" size="sm" disabled={disabled} onclick={resetToCurrentScope}>
					Inherit current scope
				</Button>
			{/if}
		{/if}
		<p class="question-source-note">A Question can narrow the Investigation scope, never expand it. Each run records its selected scope and the sources it actually used.</p>
	</div>
</details>

<style>
	.question-source-scope { margin-top: var(--space-2); color: var(--text-dim); font-size: var(--fs-xs); }
	.question-source-scope summary { width: fit-content; cursor: pointer; }
	.question-source-panel { display: grid; gap: var(--space-2); margin-top: var(--space-2); padding: var(--space-3); border: 1px solid var(--border); border-radius: var(--radius-md); }
	.question-source-choice, .question-source-list label { display: flex; align-items: center; gap: var(--space-2); }
	.question-source-list { display: grid; gap: var(--space-1); max-height: 180px; overflow: auto; margin: 0; padding: 0 0 0 var(--space-5); list-style: none; }
	.question-source-list label { min-width: 0; }
	.question-source-list span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.question-source-note { margin: 0; }
	.question-source-status { margin: 0; color: var(--text-dim); line-height: 1.45; }
	.question-source-paths { display: grid; gap: var(--space-1); max-height: 120px; overflow: auto; margin: calc(var(--space-2) * -1) 0 0; padding-left: var(--space-5); color: var(--text-faint); overflow-wrap: anywhere; }
</style>

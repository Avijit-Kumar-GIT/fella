<script lang="ts">
	import { onDestroy, untrack } from 'svelte';
	import { ipc } from '$lib/ipc';
	import type { ArtifactRecord, ImportedReport, ReportPackagePreview, ReportPackageSummary, ResolvedArtifact, StudioCheckpoint, StudioDocument, StudioPlacement } from '$lib/analytics-artifacts';
	import { buildStudioReport, studioReportFilename, type StudioReportArtifacts } from '$lib/studio-report';
	import { Button } from '$lib/components/ui/button';
	import { DropdownMenu } from '$lib/components/ui/dropdown-menu';
	import { Dialog } from '$lib/components/ui/dialog';
	import { Input } from '$lib/components/ui/input';
	import { Spinner } from '$lib/components/ui/spinner';
	import ArtifactView from './ArtifactView.svelte';
	import ImportedReportView from './ImportedReportView.svelte';
	import MarkdownContent from './MarkdownContent.svelte';
	import Icon from './Icon.svelte';

	let { workspace, documentId = null, onopen, initialArtifactId = null, onrequestchange, newReport = false }: {
		workspace: string;
		documentId?: string | null;
		onopen?: (id: string, title?: string) => void;
		newReport?: boolean;
		initialArtifactId?: string | null;
		onrequestchange?: (artifact: ResolvedArtifact, instruction: string) => void | Promise<void>;
	} = $props();

	type Draft = {
		document: StudioDocument;
		title: string;
		body: string;
		placements: StudioPlacement[];
		edits: number;
		savedEdits: number;
		saving: boolean;
		locked: boolean;
		error: string;
		notice: string;
	};
	let current = $state<Draft | null>(null);
	let documents = $state<StudioDocument[]>([]);
	let artifacts = $state<ArtifactRecord[]>([]);
	let resolved = $state<Record<string, ResolvedArtifact>>({});
	let resolveErrors = $state<Record<string, string>>({});
	let loading = $state(true);
	let loadError = $state('');
	let listError = $state('');
	let exportBusy = $state(false);
	let packageBusy = $state(false);
	let reportManagerOpen = $state(false);
	let reportViewerOpen = $state(false);
	let reportFileInput = $state<HTMLInputElement>();
	let reportFiles = $state<ReportPackageSummary[]>([]);
	let reportPreview = $state<ReportPackagePreview | null>(null);
	let selectedReport = $state<ImportedReport | null>(null);
	let reportJson = '';
	let reportBusy = $state(false);
	let reportError = $state('');
	let notesEditing = $state(false);
	let pickerOpen = $state(false);
	let pickerSearch = $state('');
	let createOpen = $state(false);
	let newTitle = $state('');
	let createError = $state('');
	let creating = $state(false);
	let checkpointOpen = $state(false);
	let checkpointTitle = $state('');
	let restoreTarget = $state<StudioCheckpoint | null>(null);
	$effect(() => { if (newReport) untrack(() => { newTitle = ''; createError = ''; createOpen = true; }); });
	let announcement = $state('');
	let requestedKey = '';
	let navigation = 0;
	let disposed = false;
	const drafts = new Map<string, Draft>();
	const timers = new Map<Draft, ReturnType<typeof setTimeout>>();
	const saves = new Map<Draft, Promise<boolean>>();
	const resolving = new Map<string, Promise<void>>();
	const initialRequests = new Set<string>();
	let filteredArtifacts = $derived(artifacts.filter(artifact => artifact.title.toLocaleLowerCase().includes(pickerSearch.trim().toLocaleLowerCase())));
	let status = $derived(current?.saving ? 'Saving…' : current?.error ? 'Couldn’t save' : current && current.edits !== current.savedEdits ? 'Unsaved changes' : 'Saved');

	function clone<T>(value: T): T { return JSON.parse(JSON.stringify(value)) as T; }
	function message(cause: unknown): string { return cause instanceof Error ? cause.message : String(cause); }
	function documentKey(root: string, id: string): string { return JSON.stringify([root, id]); }
	function resultKey(root: string, placement: Pick<StudioPlacement, 'artifact_id' | 'revision'>): string { return JSON.stringify([root, placement.artifact_id, placement.revision]); }
	function makeDraft(document: StudioDocument): Draft {
		const draft = $state<Draft>({ document, title: document.title, body: document.body, placements: clone(document.placements), edits: 0, savedEdits: 0, saving: false, locked: false, error: '', notice: '' });
		drafts.set(documentKey(document.workspace, document.id), draft);
		return draft;
	}
	function remember(document: StudioDocument): void {
		if (document.workspace !== workspace || disposed) return;
		documents = [...documents.filter(item => item.id !== document.id), document].sort((a, b) => b.updated_at_ms - a.updated_at_ms);
	}
	function changed(target: Draft): void {
		if (target.locked) return;
		++target.edits; target.error = ''; target.notice = '';
		const timer = timers.get(target);
		if (timer) clearTimeout(timer);
		timers.set(target, setTimeout(() => { timers.delete(target); void saveDraft(target); }, 600));
	}

	/** Exposed to the host so desktop-close/navigation can await all accepted edits. */
	export async function flush(): Promise<boolean> {
		const results = await Promise.all([...drafts.values()].map(saveDraft));
		return results.every(Boolean);
	}

	async function saveDraft(target: Draft): Promise<boolean> {
		const timer = timers.get(target);
		if (timer) { clearTimeout(timer); timers.delete(target); }
		const pending = saves.get(target);
		if (pending) return pending;
		if (target.edits === target.savedEdits) return true;
		// Serialize writes per document and retain edits made while an earlier write is in flight.
		const task = (async () => {
			target.saving = true;
			try {
				while (target.edits !== target.savedEdits) {
					const edits = target.edits;
					const payload = clone({ title: target.title.trim(), body: target.body, placements: target.placements });
					if (!payload.title) throw new Error('Give this document a title before saving. Your edits are still in this tab.');
					const saved = await ipc.studioSave(target.document.workspace, target.document.id, target.document.version, payload);
					target.document = saved; target.savedEdits = edits; target.error = '';
					remember(saved);
					if (!disposed && workspace === saved.workspace && current === target) onopen?.(saved.id, saved.title);
				}
				return true;
			} catch (cause) {
				target.error = `${message(cause)} Your edits are kept in this tab. Retry, or save a separate copy.`;
				return false;
			} finally { target.saving = false; }
		})();
		saves.set(target, task);
		try { return await task; } finally { saves.delete(target); }
	}

	async function resolvePlacement(root: string, placement: StudioPlacement, force = false): Promise<void> {
		const key = resultKey(root, placement);
		if (resolved[key] && !force) return;
		if (resolving.has(key)) return resolving.get(key);
		resolveErrors = { ...resolveErrors, [key]: '' };
		const task = ipc.artifactResolve(root, placement.artifact_id, placement.revision).then(value => {
			if (value.artifact.workspace !== root || value.artifact.id !== placement.artifact_id || value.revision.revision !== placement.revision) throw new Error('The saved output does not match this workspace and pinned revision.');
			if (!disposed) resolved = { ...resolved, [key]: value };
		}).catch(cause => {
			if (!disposed) resolveErrors = { ...resolveErrors, [key]: message(cause) };
		}).finally(() => resolving.delete(key));
		resolving.set(key, task);
		return task;
	}

	async function refreshOutputs(root: string): Promise<void> {
		try {
			const available = await ipc.artifactList(root);
			if (!disposed && workspace === root) { artifacts = available; listError = ''; }
		} catch (cause) { if (!disposed && workspace === root) listError = message(cause); }
	}

	async function open(root: string, id: string | null, artifactId: string | null = null): Promise<void> {
		const request = ++navigation;
		if (current) void saveDraft(current);
		requestedKey = JSON.stringify([root, id]);
		current = null; loading = true; loadError = ''; documents = []; artifacts = [];
		notesEditing = false; pickerOpen = false; checkpointOpen = false; restoreTarget = null;
		try {
			const available = await ipc.studioList(root);
			if (disposed || request !== navigation) return;
			documents = available;
			void refreshOutputs(root);
			let document = id ? available.find(item => item.id === id) : available[0];
			if (id && !document) throw new Error('This report is no longer available in the selected workspace.');
			if (!document && artifactId) {
				const output = await ipc.artifactResolve(root, artifactId);
				if (disposed || request !== navigation) return;
				document = await ipc.studioCreate(root, output.artifact.title);
				if (disposed || request !== navigation) return;
				remember(document);
			}
			if (document) {
				const target = drafts.get(documentKey(root, document.id)) ?? makeDraft(document);
				current = target;
				requestedKey = JSON.stringify([root, document.id]);
				for (const placement of target.placements) void resolvePlacement(root, placement);
				if (artifactId) await addInitial(target, artifactId);
				if (!disposed && request === navigation && id !== document.id) onopen?.(document.id);
			}
		} catch (cause) { if (!disposed && request === navigation) loadError = message(cause); }
		finally { if (!disposed && request === navigation) loading = false; }
	}

	$effect(() => {
		const root = workspace;
		const id = documentId;
		const artifact = initialArtifactId;
		untrack(() => {
			const key = JSON.stringify([root, id]);
			if (key !== requestedKey && !(id === null && current?.document.workspace === root)) void open(root, id, artifact);
			else if (artifact && current?.document.workspace === root) void addInitial(current, artifact);
		});
	});

	async function addInitial(target: Draft, id: string): Promise<void> {
		const key = JSON.stringify([target.document.workspace, id]);
		if (initialRequests.has(key)) return;
		initialRequests.add(key);
		try {
			if (target.placements.some(placement => placement.artifact_id === id)) return;
			const output = await ipc.artifactResolve(target.document.workspace, id);
			if (disposed || target.locked) { initialRequests.delete(key); return; }
			addResolved(target, output);
		} catch (cause) { initialRequests.delete(key); target.error = message(cause); }
	}

	function switchDocument(id: string): void {
		if (current?.document.id === id) return;
		const root = workspace;
		void open(root, id);
		onopen?.(id);
	}

	async function create(): Promise<void> {
		const root = workspace;
		const title = newTitle.trim();
		if (!title || creating) return;
		creating = true; createError = '';
		try {
			const document = await ipc.studioCreate(root, title);
			if (disposed || workspace !== root) return;
			if (current) void saveDraft(current);
			++navigation; loading = false; loadError = '';
			current = makeDraft(document); notesEditing = true;
			requestedKey = JSON.stringify([root, document.id]);
			remember(document); createOpen = false; newTitle = '';
			onopen?.(document.id);
		} catch (cause) { if (!disposed && workspace === root) createError = message(cause); }
		finally { if (!disposed && workspace === root) creating = false; }
	}

	function addResolved(target: Draft, output: ResolvedArtifact): void {
		if (target.locked || output.artifact.workspace !== target.document.workspace) return;
		const placement: StudioPlacement = { id: crypto.randomUUID(), artifact_id: output.artifact.id, revision: output.revision.revision, view_state: {} };
		resolved = { ...resolved, [resultKey(target.document.workspace, placement)]: output };
		target.placements = [...target.placements, placement];
		changed(target); pickerOpen = false;
		announcement = `${output.artifact.title} added to the report.`;
	}

	async function addArtifact(id: string): Promise<void> {
		const target = current;
		if (!target || target.locked) return;
		try {
			const output = await ipc.artifactResolve(target.document.workspace, id);
			if (disposed || current !== target) return;
			addResolved(target, output);
		} catch (cause) { if (!disposed && current === target) target.error = message(cause); }
	}

	function move(target: Draft, id: string, direction: -1 | 1): void {
		if (target.locked) return;
		const index = target.placements.findIndex(item => item.id === id);
		const next = index + direction;
		if (index < 0 || next < 0 || next >= target.placements.length) return;
		const placements = [...target.placements];
		[placements[index], placements[next]] = [placements[next], placements[index]];
		target.placements = placements; changed(target);
		announcement = `Output moved to position ${next + 1} of ${placements.length}.`;
	}

	function remove(target: Draft, id: string): void {
		if (target.locked) return;
		target.placements = target.placements.filter(item => item.id !== id); changed(target);
		announcement = 'Output removed from this document. Its saved analytical record is unchanged.';
	}
	function pin(target: Draft, id: string, revision: number): void {
		if (target.locked) return;
		const placement = target.placements.find(item => item.id === id);
		if (!placement || placement.revision === revision) return;
		target.placements = target.placements.map(item => item.id === id ? { ...item, revision, view_state: {} } : item);
		changed(target);
		void resolvePlacement(target.document.workspace, { ...placement, revision });
	}
	function selectState(target: Draft, id: string, state: Record<string, unknown>): void {
		if (target.locked) return;
		const placement = target.placements.find(item => item.id === id);
		if (!placement || JSON.stringify(placement.view_state) === JSON.stringify(state)) return;
		target.placements = target.placements.map(item => item.id === id ? { ...item, view_state: clone(state) } : item);
		changed(target);
	}

	async function checkpoint(): Promise<void> {
		const target = current;
		if (!target || target.locked || !checkpointTitle.trim()) return;
		const title = checkpointTitle.trim();
		target.locked = true;
		try {
			if (!await saveDraft(target)) return;
			const saved = await ipc.studioCheckpoint(target.document.workspace, target.document.id, target.document.version, title);
			target.document = saved; remember(saved);
			target.notice = 'Checkpoint saved.';
			if (current === target) { checkpointOpen = false; checkpointTitle = ''; }
		} catch (cause) { target.error = message(cause); }
		finally { target.locked = false; }
	}

	async function restore(): Promise<void> {
		const target = current;
		const selected = restoreTarget;
		if (!target || !selected || target.locked) return;
		target.locked = true;
		try {
			if (!await saveDraft(target)) return;
			// Preserve the outgoing document explicitly, including work since its last named checkpoint.
			target.document = await ipc.studioCheckpoint(target.document.workspace, target.document.id, target.document.version, `Before restoring ${selected.title}`);
			const saved = await ipc.studioRestore(target.document.workspace, target.document.id, target.document.version, selected.id);
			target.document = saved; target.title = saved.title; target.body = saved.body; target.placements = clone(saved.placements);
			target.savedEdits = target.edits; target.error = ''; target.notice = 'Checkpoint restored. The previous document is preserved in history.';
			remember(saved);
			for (const placement of saved.placements) void resolvePlacement(saved.workspace, placement);
			if (current === target) restoreTarget = null;
		} catch (cause) { target.error = message(cause); }
		finally { target.locked = false; }
	}

	async function saveCopy(): Promise<void> {
		const target = current;
		if (!target || target.locked) return;
		target.locked = true;
		try {
			const copy = await ipc.studioCreate(target.document.workspace, `${target.title.trim() || 'Untitled report'} copy`);
			const saved = await ipc.studioSave(copy.workspace, copy.id, copy.version, clone({ title: copy.title, body: target.body, placements: target.placements }));
			remember(saved);
			if (!disposed && current === target) {
				current = makeDraft(saved); current.notice = 'Edits saved as a separate document. The original is unchanged.';
				requestedKey = JSON.stringify([saved.workspace, saved.id]); onopen?.(saved.id);
			}
		} catch (cause) { target.error = message(cause); }
		finally { target.locked = false; }
	}

	async function download(): Promise<void> {
		const target = current;
		if (!target || exportBusy || target.locked) return;
		exportBusy = true;
		try {
			if (!await saveDraft(target)) return;
			const document = clone(target.document);
			const outputs: StudioReportArtifacts = {};
			await Promise.all(document.placements.map(async placement => {
				try { outputs[placement.id] = await ipc.artifactResolve(document.workspace, placement.artifact_id, placement.revision); }
				catch { outputs[placement.id] = null; }
			}));
			if (disposed) return;
			let presentation: import('$lib/studio-report').StudioReportOptions['presentation'];
			try {
				const { exportPresentation } = await import('$lib/presentation/export');
				presentation = ({ spec, results, state }) => {
					const snapshot = exportPresentation(spec, results, { state });
					return { markdown: snapshot.markdown, disclosures: snapshot.warnings };
				};
			} catch { /* The original presentation, answer and exact values remain exportable. */ }
			const markdown = buildStudioReport(document, outputs, { presentation });
			const url = URL.createObjectURL(new Blob([markdown], { type: 'text/markdown;charset=utf-8' }));
			const link = window.document.createElement('a');
			link.href = url; link.download = studioReportFilename(document.title);
			window.document.body.appendChild(link); link.click(); link.remove();
			setTimeout(() => URL.revokeObjectURL(url), 1000);
			const missing = document.placements.filter(placement => !outputs[placement.id]).length;
			target.notice = missing ? `Markdown exported with ${missing} unavailable output${missing === 1 ? '' : 's'} explicitly noted.` : 'Markdown exported from saved results.';
		} catch (cause) { target.error = message(cause); }
		finally { if (!disposed) exportBusy = false; }
	}

	function downloadBlob(name: string, contents: string, type: string): void {
		const url = URL.createObjectURL(new Blob([contents], { type }));
		const link = window.document.createElement('a');
		link.href = url; link.download = name; window.document.body.appendChild(link); link.click(); link.remove();
		setTimeout(() => URL.revokeObjectURL(url), 1000);
	}

	async function downloadPackage(): Promise<void> {
		const target = current;
		if (!target || packageBusy || target.locked) return;
		packageBusy = true; target.error = '';
		try {
			if (!await saveDraft(target)) return;
			const json = await ipc.studioExportPackage(target.document.workspace, target.document.id);
			if (disposed || current !== target) return;
			downloadBlob(`${studioReportFilename(target.title).replace(/\.md$/, '')}.fella-report.json`, json, 'application/vnd.fella.report+json');
			target.notice = 'Portable report snapshot exported. It contains selected results and analytical context, not source files.';
		} catch (cause) { target.error = message(cause); }
		finally { if (!disposed) packageBusy = false; }
	}

	async function refreshReportList(): Promise<void> {
		reportError = '';
		try {
			const items = await ipc.reportPackageList();
			if (!disposed) reportFiles = items;
		} catch (cause) { if (!disposed) reportError = message(cause); }
	}

	async function chooseReportFile(event: Event): Promise<void> {
		const input = event.currentTarget as HTMLInputElement;
		const file = input.files?.[0];
		input.value = '';
		if (!file) return;
		reportError = ''; reportPreview = null;
		if (file.size > 10 * 1024 * 1024) { reportError = 'This report snapshot exceeds the 10 MiB limit.'; return; }
		try {
			const json = await file.text();
			const preview = await ipc.reportPackagePreview(json);
			reportJson = json; reportPreview = preview;
		} catch (cause) { reportError = `Could not validate this report snapshot: ${message(cause)}`; }
	}

	async function importReport(): Promise<void> {
		if (!reportPreview || reportBusy) return;
		reportBusy = true; reportError = '';
		try {
			const imported = await ipc.reportPackageImport(reportJson);
			selectedReport = imported; reportPreview = null; reportJson = '';
			reportManagerOpen = false; reportViewerOpen = true;
			void refreshReportList();
		} catch (cause) { reportError = message(cause); }
		finally { reportBusy = false; }
	}

	async function openImportedReport(id: string): Promise<void> {
		reportBusy = true; reportError = '';
		try {
			selectedReport = await ipc.reportPackageLoad(id);
			reportManagerOpen = false; reportViewerOpen = true;
		} catch (cause) { reportError = message(cause); }
		finally { reportBusy = false; }
	}

	onDestroy(() => {
		disposed = true; ++navigation;
		for (const timer of timers.values()) clearTimeout(timer);
		timers.clear();
		for (const target of drafts.values()) if (target.edits !== target.savedEdits && !target.error) void saveDraft(target);
	});
</script>

<section class="studio" aria-label="Report editor" aria-busy={loading}>
	<header class="studio-toolbar">
		<div class="studio-breadcrumb">
			<span>Reports</span><span class="studio-breadcrumb-separator">/</span>
			<DropdownMenu.Root>
				<DropdownMenu.Trigger class="studio-document-trigger" aria-label="Choose report"><span>{current?.title.trim() || 'Choose a report'}</span><Icon name="chevron-down" size={12} /></DropdownMenu.Trigger>
				<DropdownMenu.Portal><DropdownMenu.Content class="fella-ui-menu-content studio-document-menu" align="start">
					{#each documents as document (document.id)}<DropdownMenu.Item class="fella-ui-menu-item" onSelect={() => switchDocument(document.id)}>{document.title}{#if current?.document.id === document.id}<Icon name="check" size={14} />{/if}</DropdownMenu.Item>{/each}
					<DropdownMenu.Item class="fella-ui-menu-item" onSelect={() => { newTitle = ''; createError = ''; createOpen = true; }}><Icon name="plus" size={14} />New report</DropdownMenu.Item>
				</DropdownMenu.Content></DropdownMenu.Portal>
			</DropdownMenu.Root>
		</div>
		<div class="toolbar-actions">
			{#if current}
				<Button class="studio-action" variant="ghost" size="sm" disabled={current.locked} onclick={() => {
					if (notesEditing) { notesEditing = false; if (current) void saveDraft(current); }
					else notesEditing = true;
				}}>{#if notesEditing}<Icon name="check" size={14} />Done{:else}<Icon name="pencil" size={12} />Edit{/if}</Button>
				<span class="save-status" role="status" aria-live="polite">{status}</span>
				<Button class="studio-action" variant="ghost" size="sm" disabled={exportBusy || current.locked} onclick={() => void download()}><Icon name="download" size={12} />{exportBusy ? 'Exporting…' : 'Export'}</Button>
			{/if}
			<DropdownMenu.Root><DropdownMenu.Trigger class="studio-icon-button" aria-label="More report actions" title="More report actions"><Icon name="more-horizontal" size={16} /></DropdownMenu.Trigger><DropdownMenu.Portal><DropdownMenu.Content class="fella-ui-menu-content" align="end">
				<DropdownMenu.Item class="fella-ui-menu-item" onSelect={() => { reportManagerOpen = true; reportPreview = null; reportJson = ''; reportError = ''; void refreshReportList(); }}>Import snapshot…</DropdownMenu.Item>
				{#if current}
					<DropdownMenu.Item class="fella-ui-menu-item" disabled={packageBusy || current.locked} onSelect={() => void downloadPackage()}>Export portable snapshot</DropdownMenu.Item>
					<DropdownMenu.Item class="fella-ui-menu-item" disabled={current.locked} onSelect={() => { checkpointTitle = ''; checkpointOpen = true; }}>Save checkpoint…</DropdownMenu.Item>
					<DropdownMenu.Item class="fella-ui-menu-item" disabled={current.locked} onSelect={() => void saveCopy()}>Save a copy</DropdownMenu.Item>
				{/if}
			</DropdownMenu.Content></DropdownMenu.Portal></DropdownMenu.Root>
		</div>
	</header>
	<div class="studio-scroll">
		{#if loading}<div class="empty-state" role="status"><Spinner size={16} /><p>Opening report…</p></div>
		{:else if loadError}<div class="empty-state"><h1>Couldn’t open report</h1><p role="alert">{loadError}</p><Button size="sm" variant="outline" onclick={() => void open(workspace, documentId, initialArtifactId)}>Retry</Button></div>
		{:else if !current}<div class="empty-state"><Icon name="file" size={20} /><h1>Keep useful analysis together.</h1><p>Add saved outputs and your own notes. Arrange them here without rerunning the analysis.</p><Button size="sm" onclick={() => { newTitle = ''; createError = ''; createOpen = true; }}><Icon name="plus" size={14} />New report</Button></div>
		{:else}
			{@const target = current}
			<div class="studio-document" data-studio-document-id={target.document.id}>
				<div class="document-heading">
					{#if notesEditing}
						<input class="document-title" aria-label="Report title" maxlength="120" value={target.title} disabled={target.locked} oninput={event => { target.title = event.currentTarget.value; changed(target); }} placeholder="Untitled report" />
					{:else}
						<h1 class="document-title">{target.title.trim() || 'Untitled report'}</h1>
					{/if}
					<p class="document-caption">Report · Updated {new Intl.DateTimeFormat(undefined, { dateStyle: 'medium' }).format(new Date(target.document.updated_at_ms))}</p>
				</div>
				{#if target.error}<div class="save-error"><p role="alert">{target.error}</p><div><Button size="sm" variant="outline" disabled={target.locked || target.saving} onclick={() => void saveDraft(target)}>Retry save</Button><Button size="sm" variant="ghost" disabled={target.locked} onclick={() => void saveCopy()}>Save a copy</Button></div></div>{:else if target.notice}<p class="document-notice" role="status">{target.notice}</p>{/if}
				<section class="document-notes" class:document-notes-empty={!notesEditing && !target.body.trim()} aria-label="Document notes">
					{#if notesEditing}
						<label class="notes-label">Notes <textarea class="notes-input" aria-label="Report notes" rows="6" value={target.body} disabled={target.locked} oninput={event => { target.body = event.currentTarget.value; changed(target); }} placeholder="Add context or a narrative. Markdown is supported."></textarea></label>
					{:else if target.body.trim()}
						<MarkdownContent text={target.body} />
					{/if}
				</section>
				<ol class="placements" aria-label="Report results">
					{#each target.placements as placement, index (placement.id)}
						{@const key = resultKey(target.document.workspace, placement)}
						<li class="placement" data-placement-id={placement.id}>
							{#if notesEditing}<div class="placement-controls"><span class="position-label">Output {index + 1}</span><div class="placement-buttons"><button class="studio-icon-button" aria-label={`Move output ${index + 1} up`} title="Move up" disabled={index === 0 || target.locked} onclick={() => move(target, placement.id, -1)}><Icon name="chevron-up" size={14} /></button><button class="studio-icon-button" aria-label={`Move output ${index + 1} down`} title="Move down" disabled={index === target.placements.length - 1 || target.locked} onclick={() => move(target, placement.id, 1)}><Icon name="chevron-down" size={14} /></button><button class="studio-icon-button" aria-label={`Remove output ${index + 1} from document`} title="Remove from document, keep saved output" disabled={target.locked} onclick={() => remove(target, placement.id)}><Icon name="x" size={14} /></button></div></div>{/if}
							{#if resolved[key]}
								{#key `${placement.id}:${placement.revision}`}<ArtifactView resolved={resolved[key]} viewState={placement.view_state} onstatechange={state => selectState(target, placement.id, state)} onrevisionchange={revision => pin(target, placement.id, revision)} {onrequestchange} />{/key}
							{:else if resolveErrors[key]}<div class="output-unavailable"><h2>Saved output unavailable</h2><p role="alert">{resolveErrors[key]}</p><p class="document-caption">{placement.artifact_id} · revision {placement.revision}. The placement is retained.</p><Button size="sm" variant="outline" onclick={() => void resolvePlacement(target.document.workspace, placement, true)}>Retry output</Button></div>
							{:else}<p class="output-loading" role="status"><Spinner size={14} />Opening saved output…</p>{/if}
						</li>
					{/each}
				</ol>
				{#if !target.placements.length}<p class="document-caption output-empty">No saved analysis in this report yet.</p>{/if}
				{#if notesEditing}<div class="add-output-row"><Button size="sm" variant="outline" disabled={target.locked} onclick={() => { pickerSearch = ''; listError = ''; pickerOpen = true; void refreshOutputs(target.document.workspace); }}><Icon name="plus" size={14} />Add saved output</Button></div>{/if}
				{#if target.document.checkpoints.length}<details class="history"><summary>History · {target.document.checkpoints.length} {target.document.checkpoints.length === 1 ? 'checkpoint' : 'checkpoints'}</summary><ul>{#each [...target.document.checkpoints].reverse() as checkpoint (checkpoint.id)}<li><div><strong>{checkpoint.title}</strong><span>{new Date(checkpoint.created_at_ms).toLocaleString()}</span></div><Button size="sm" variant="ghost" disabled={target.locked} onclick={() => restoreTarget = checkpoint}>Restore</Button></li>{/each}</ul></details>{/if}
			</div>
		{/if}
	</div>
	<span class="sr-only" role="status" aria-live="polite">{announcement}</span>
</section>

<input class="sr-only" bind:this={reportFileInput} type="file" accept=".json,application/json,application/vnd.fella.report+json" aria-label="Choose a Fella report snapshot" onchange={event => void chooseReportFile(event)} />

<Dialog.Root bind:open={reportManagerOpen}><Dialog.Portal><Dialog.Overlay class="fella-ui-dialog-overlay" /><Dialog.Content class="fella-ui-dialog-content report-manager">
	<div class="picker-heading"><div><Dialog.Title class="dialog-title">Report snapshots</Dialog.Title><Dialog.Description class="dialog-description">Portable, read-only copies of a report and its saved analysis.</Dialog.Description></div><Dialog.Close class="studio-icon-button" aria-label="Close report snapshots"><Icon name="x" size={16} /></Dialog.Close></div>
	{#if reportPreview}
		<div class="report-preview-summary"><h3>{reportPreview.title}</h3><p>{reportPreview.artifact_count} saved outputs · {reportPreview.run_count} analysis runs</p><p>Fella will retain this as a read-only snapshot. It cannot query original files; imported presentations do not execute code.</p><ul>{#each reportPreview.warnings as warning}<li>{warning}</li>{/each}</ul><div class="dialog-actions"><Button variant="ghost" size="sm" disabled={reportBusy} onclick={() => { reportPreview = null; reportJson = ''; }}>Cancel</Button><Button size="sm" disabled={reportBusy} onclick={() => void importReport()}>{reportBusy ? 'Importing…' : 'Import read-only snapshot'}</Button></div></div>
	{:else}
		<div class="report-manager-actions"><Button size="sm" variant="outline" disabled={reportBusy} onclick={() => reportFileInput?.click()}><Icon name="plus" size={14} />Import snapshot…</Button><Button size="sm" variant="ghost" disabled={reportBusy} onclick={() => void refreshReportList()}>Refresh</Button></div>
		{#if reportFiles.length}<ul class="report-file-list">{#each reportFiles as report (report.id)}<li><button disabled={reportBusy} onclick={() => void openImportedReport(report.id)}><span><strong>{report.title}</strong><small>{report.artifact_count} outputs · {report.run_count} runs · {new Date(report.modified_at_ms).toLocaleDateString()}</small></span><span>Open</span></button></li>{/each}</ul>
		{:else if !reportError}<p class="document-caption">No imported snapshots yet. Import a Fella report JSON file to retain a read-only copy.</p>{/if}
	{/if}
	{#if reportError}<p class="dialog-error" role="alert">{reportError}</p>{/if}
</Dialog.Content></Dialog.Portal></Dialog.Root>

<Dialog.Root bind:open={reportViewerOpen}><Dialog.Portal><Dialog.Overlay class="fella-ui-dialog-overlay" /><Dialog.Content class="fella-ui-dialog-content report-viewer"><div class="picker-heading"><div><Dialog.Title class="dialog-title">{selectedReport?.preview.title ?? 'Imported report'}</Dialog.Title><Dialog.Description class="dialog-description">Read-only snapshot · saved checks are not reverified.</Dialog.Description></div><Dialog.Close class="studio-icon-button" aria-label="Close imported report"><Icon name="x" size={16} /></Dialog.Close></div>{#if selectedReport}<ImportedReportView report={selectedReport.preview.package} />{/if}</Dialog.Content></Dialog.Portal></Dialog.Root>

<Dialog.Root bind:open={createOpen}><Dialog.Portal><Dialog.Overlay class="fella-ui-dialog-overlay" /><Dialog.Content class="fella-ui-dialog-content studio-dialog"><form class="dialog-form" onsubmit={event => { event.preventDefault(); void create(); }}><Dialog.Title class="dialog-title">New report</Dialog.Title><Dialog.Description class="dialog-description">Arrange saved analysis and your own notes.</Dialog.Description><label>Name<Input aria-label="New report name" bind:value={newTitle} maxlength={120} placeholder="For example, quarterly review" /></label>{#if createError}<p class="dialog-error" role="alert">{createError}</p>{/if}<div class="dialog-actions"><Dialog.Close class="studio-cancel">Cancel</Dialog.Close><Button type="submit" size="sm" disabled={!newTitle.trim() || creating}>{creating ? 'Creating…' : 'Create report'}</Button></div></form></Dialog.Content></Dialog.Portal></Dialog.Root>

<Dialog.Root bind:open={pickerOpen}><Dialog.Portal><Dialog.Overlay class="fella-ui-dialog-overlay" /><Dialog.Content class="fella-ui-dialog-content studio-picker"><div class="picker-heading"><div><Dialog.Title class="dialog-title">Add saved output</Dialog.Title><Dialog.Description class="dialog-description">Uses the saved result. No analysis is rerun.</Dialog.Description></div><Dialog.Close class="studio-icon-button" aria-label="Close saved outputs"><Icon name="x" size={16} /></Dialog.Close></div><Input aria-label="Find saved outputs" bind:value={pickerSearch} placeholder="Find an output…" />{#if listError}<p class="dialog-error" role="alert">{listError}</p><Button size="sm" variant="ghost" onclick={() => void refreshOutputs(workspace)}>Retry</Button>{/if}<ul class="output-picker-list">{#each filteredArtifacts as artifact (artifact.id)}<li><button onclick={() => void addArtifact(artifact.id)}><strong>{artifact.title}</strong><span>Revision {artifact.current_revision}</span><Icon name="plus" size={14} /></button></li>{/each}</ul>{#if !filteredArtifacts.length && !listError}<p class="document-caption">{pickerSearch.trim() ? 'No matching saved outputs.' : 'No saved results in this workspace yet. Ask a question to produce one.'}</p>{/if}</Dialog.Content></Dialog.Portal></Dialog.Root>

<Dialog.Root bind:open={checkpointOpen}><Dialog.Portal><Dialog.Overlay class="fella-ui-dialog-overlay" /><Dialog.Content class="fella-ui-dialog-content studio-dialog"><form class="dialog-form" onsubmit={event => { event.preventDefault(); void checkpoint(); }}><Dialog.Title class="dialog-title">Save checkpoint</Dialog.Title><Dialog.Description class="dialog-description">Keep this document’s notes, order, selections, and pinned revisions.</Dialog.Description><label>Name<Input aria-label="Checkpoint name" bind:value={checkpointTitle} maxlength={120} /></label><div class="dialog-actions"><Dialog.Close class="studio-cancel">Cancel</Dialog.Close><Button type="submit" size="sm" disabled={!checkpointTitle.trim() || current?.locked}>Save checkpoint</Button></div></form></Dialog.Content></Dialog.Portal></Dialog.Root>

<Dialog.Root open={restoreTarget !== null} onOpenChange={value => { if (!value) restoreTarget = null; }}><Dialog.Portal><Dialog.Overlay class="fella-ui-dialog-overlay" /><Dialog.Content class="fella-ui-dialog-content studio-dialog"><Dialog.Title class="dialog-title">Restore checkpoint?</Dialog.Title><Dialog.Description class="dialog-description">Restore “{restoreTarget?.title}”. Fella first saves the current document as a checkpoint. Later artifact revisions are not deleted.</Dialog.Description><div class="dialog-actions"><Dialog.Close class="studio-cancel">Cancel</Dialog.Close><Button size="sm" disabled={current?.locked} onclick={() => void restore()}>Restore checkpoint</Button></div></Dialog.Content></Dialog.Portal></Dialog.Root>

<style>
	.studio { display: flex; flex-direction: column; flex: 1; min-width: 0; min-height: 0; background: var(--bg); container-type: inline-size; }
	.studio-toolbar { display: flex; align-items: center; justify-content: space-between; flex: none; min-width: 0; padding:16px 24px 0; min-height:52px; -webkit-app-region:drag; gap: var(--space-2); }
	.studio-breadcrumb { display:flex; min-width:0; align-items:center; gap:8px; color:var(--text-faint); font-size:var(--fs-xs); }
	.studio-breadcrumb-separator { color:var(--text-faint); }
	:global(.studio-document-trigger) { display: flex; align-items: center; gap:6px; min-width:0; max-width:min(360px, 40vw); min-height:30px; padding:4px 2px; border:0; border-radius:8px; background:transparent; color:var(--text); font:inherit; font-size:var(--fs-xs); cursor:pointer; }
	:global(.studio-document-trigger:hover) { color:var(--text); background:var(--bg-raised); }
	:global(.studio-document-trigger > span) { min-width: 0; white-space: nowrap; text-overflow: ellipsis; overflow: hidden; }
	:global(.studio-document-menu) { max-width: min(380px, calc(100vw - 32px)); max-height: 350px; overflow: auto; }
	:global(.studio-document-menu .fella-ui-menu-item) { gap: var(--space-2); justify-content: space-between; overflow-wrap: anywhere; }
	.toolbar-actions { -webkit-app-region:no-drag; min-width:0; display: flex; align-items: center; gap: 6px; }
	:global(.studio-action) { min-height:36px; gap:7px; padding-inline:12px; border-radius:999px; background:var(--bg-raised); color:var(--text); font-size:var(--fs-xs); }
	:global(.studio-action:hover:not(:disabled)) { background:var(--bg-raised-2); }
	.save-status { color: var(--text-faint); font-size: 11px; white-space: nowrap; padding-inline: var(--space-1); }
	:global(.studio-icon-button) { display: inline-flex; flex: none; align-items: center; justify-content: center; min-height: 28px; min-width: 28px; padding: 5px; border: 0; border-radius: var(--radius-sm); background: transparent; color: var(--text-faint); cursor: pointer; }
	:global(.studio-icon-button:hover:not(:disabled)) { background: var(--bg-inset); color: var(--text); }
	:global(.studio-icon-button:disabled) { opacity: .35; cursor: default; }
	:global(.studio-icon-button:focus-visible), :global(.studio-document-trigger:focus-visible), .document-title:focus-visible, .notes-input:focus-visible, .history summary:focus-visible, .output-picker-list button:focus-visible { outline: 2px solid var(--brand); outline-offset: 2px; }
	.studio-scroll { min-height:0; flex:1; overflow:auto; overscroll-behavior:contain; padding:0 24px 64px; }
	.studio-document { box-sizing: border-box; width: 100%; max-width: 600px; margin: 0 auto; padding:48px 0 0; }
	.document-heading { margin-bottom:40px; }
	.document-title { display:block; box-sizing: border-box; width: 100%; min-width: 0; padding: 4px 0; margin: 0 0 5px; border: 0; background: transparent; color: var(--text); font:500 30px/38px var(--sans); letter-spacing:-.01em; text-overflow: ellipsis; }
	p { margin: 0; }
	.document-caption, .document-notice { color: var(--text-faint); font-size: var(--fs-xs); line-height: 1.6; overflow-wrap: anywhere; }
	.document-notice { margin: 0 0 var(--space-4); }
	.document-notes { min-width:0; margin-bottom:40px; font-size:15px; line-height:26px; }
	.document-notes-empty { margin-bottom: 0; }
	.notes-label { display: grid; gap: var(--space-2); color: var(--text-faint); font-size: var(--fs-xs); }
	.notes-input { box-sizing: border-box; width: 100%; resize: vertical; padding: var(--space-3); border: 1px solid var(--border); border-radius: var(--radius); background: var(--bg-inset); color: var(--text); font: var(--fs-sm)/1.65 var(--sans); }
	.placements { list-style: none; margin: 0; padding: 0; }
	.placement { min-width: 0; position: relative; margin-bottom: 32px; padding-top: 5px; }
	.placement-controls { display: flex; align-items: center; justify-content: space-between; margin-bottom: var(--space-2); }
	.position-label { color: var(--text-faint); font: 11px var(--mono); }
	.placement-buttons { display: flex; gap: 2px; opacity:0; transition:opacity var(--dur-fast) var(--ease); }
	.placement:hover .placement-buttons, .placement:focus-within .placement-buttons { opacity:1; }
	.output-unavailable { display: grid; gap: var(--space-3); padding: var(--space-4); border-radius: var(--radius-window); background: var(--bg-inset); }
	.output-unavailable h2 { margin: 0; font-size: var(--fs-sm); font-weight: 600; }
	.output-unavailable p[role='alert'] { color: var(--err); font-size: var(--fs-xs); overflow-wrap: anywhere; }
	.output-unavailable :global(button) { justify-self: start; }
	.output-loading { display: flex; align-items: center; gap: var(--space-2); color: var(--text-faint); font-size: var(--fs-xs); padding-block: var(--space-4); }
	.output-empty { margin-bottom: var(--space-4); }
	.add-output-row { margin-bottom: var(--space-5); }
	.empty-state { display: flex; flex-direction: column; align-items: start; gap: var(--space-4); margin: clamp(24px, 9cqi, 90px) auto; padding: var(--space-5); max-width: 440px; color: var(--text-faint); font-size: var(--fs-sm); line-height: 1.6; }
	.empty-state h1 { color: var(--text); font-size: var(--fs-xl); line-height: 1.4; font-weight: 550; margin: 0; }
	.history { font-size: var(--fs-xs); color: var(--text-faint); }
	.history summary { cursor: pointer; }
	.history ul { list-style: none; margin: var(--space-3) 0; padding: 0; }
	.history li { display: flex; align-items: center; justify-content: space-between; padding: var(--space-2) 0; gap: var(--space-3); }
	.history li > div { min-width: 0; display: grid; gap: 2px; overflow-wrap: anywhere; }
	.history strong { font-weight: 500; color: var(--text-dim); }
	.save-error { display: grid; gap: var(--space-2); margin-bottom: var(--space-4); font-size: var(--fs-xs); }
	.save-error p { color: var(--err); overflow-wrap: anywhere; }
	.save-error > div { display: flex; gap: var(--space-2); flex-wrap: wrap; }
	:global(.studio-dialog), :global(.studio-picker), :global(.report-manager), :global(.report-viewer) { width: min(460px, calc(100vw - 32px)); padding: var(--space-5); display: grid; gap: var(--space-4); }
	:global(.studio-picker) { width: min(520px, calc(100vw - 32px)); }
	:global(.report-manager) { width: min(560px, calc(100vw - 32px)); max-height: min(84vh, 720px); overflow: auto; }
	:global(.report-viewer) { width: min(940px, calc(100vw - 32px)); max-height: min(88vh, 860px); overflow: auto; }
	.report-manager-actions { display:flex; gap:var(--space-2); align-items:center; }
	.report-preview-summary { display:grid; gap:var(--space-2); color:var(--text-dim); font-size:var(--fs-xs); line-height:1.55; }
	.report-preview-summary h3 { margin:0; color:var(--text-primary); font-size:var(--fs-md); }
	.report-preview-summary p { margin:0; }
	.report-preview-summary ul { margin:0; padding-left:20px; }
	.report-file-list { margin:0; padding:0; list-style:none; }
	.report-file-list li + li { border-top:1px solid var(--border-subtle); }
	.report-file-list button { display:flex; width:100%; gap:var(--space-3); align-items:center; justify-content:space-between; padding:10px 4px; border:0; background:transparent; color:var(--text-primary); text-align:left; cursor:pointer; }
	.report-file-list button:hover:not(:disabled) { background:var(--bg-inset); }
	.report-file-list button > span:first-child { display:grid; gap:3px; min-width:0; }
	.report-file-list strong { overflow:hidden; text-overflow:ellipsis; white-space:nowrap; font-size:var(--fs-sm); font-weight:500; }
	.report-file-list small { color:var(--text-faint); font-size:var(--fs-xs); }
	.dialog-form { display: grid; gap: var(--space-4); }
	:global(.studio-dialog .dialog-title), :global(.studio-picker .dialog-title) { margin: 0; font-size: 20px; font-weight: 600; color: var(--text); }
	:global(.studio-dialog .dialog-description), :global(.studio-picker .dialog-description) { margin: 0; color: var(--text-faint); font-size: var(--fs-sm); line-height: 1.5; }
	.dialog-form label { display: grid; gap: var(--space-2); color: var(--text-dim); font-size: var(--fs-xs); }
	.dialog-actions { display: flex; justify-content: end; align-items: center; gap: var(--space-2); }
	:global(.studio-cancel) { border: 0; background: transparent; color: var(--text-dim); border-radius: var(--radius); padding: 8px 12px; font: inherit; font-size: var(--fs-xs); cursor: pointer; }
	.dialog-error { color: var(--err); font-size: var(--fs-xs); }
	.picker-heading { display: flex; justify-content: space-between; gap: var(--space-3); }
	.picker-heading > div { display: grid; gap: var(--space-2); }
	.output-picker-list { list-style: none; padding: 0; margin: 0; max-height: min(420px, 50vh); overflow: auto; }
	.output-picker-list button { box-sizing: border-box; display: grid; grid-template-columns: minmax(0, 1fr) auto; align-items: center; gap: 3px var(--space-3); width: 100%; padding: 12px; border: 0; border-radius: var(--radius); background: transparent; color: var(--text); text-align: left; cursor: pointer; font: inherit; }
	.output-picker-list button:hover { background: var(--bg-inset); }
	.output-picker-list strong { font-weight: 500; font-size: var(--fs-sm); overflow-wrap: anywhere; }
	.output-picker-list span { color: var(--text-faint); font-size: var(--fs-xs); grid-row: 2; }
	.output-picker-list :global(svg) { grid-column: 2; grid-row: 1 / 3; color: var(--text-faint); }
	@container (max-width: 480px) { .save-status { position: absolute; width: 1px; height: 1px; overflow: hidden; clip-path: inset(50%); } .studio-toolbar { padding-inline: 10px; } .studio-document { padding-top: var(--space-4); } :global(.studio-document-trigger) { max-width:34vw; } :global(.studio-action) { gap:4px; padding-inline:8px; } }
</style>

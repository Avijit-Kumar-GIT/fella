import { readFile, readdir } from 'node:fs/promises';
import { join } from 'node:path';
import { test, expect, question, caveat, answer, columns, exactRows, comparisonSpec, secondSpec, invoke, rejected, chooseWorkspace, engineRequests, startBudget, assertNoExecution, launch, seedRun, placement, createArtifact, createDocument, assertCanonicalUnchanged, openStudio, openStudioDocument, addSavedOutput, assertSavedRows, expectMarkdownDownload, expectPackageDownload } from './generative-workspace-harness.mjs';

/**
 * Organization-only relocation map (2026-10-22), original task set v1.5.
 * Keep: same-turn json-render/state, finalizer-only correction, invalid UI fallback,
 * Markdown conversion/export, portable report export/reopening.
 * Move unchanged: immutable revisions/checkpoints, workspace ownership, chat
 * deletion retention, Studio edits/reordering/restart. Shared setup and helpers
 * live in generative-workspace-harness.mjs. Test titles/bodies/assertions are
 * unchanged; this split is not a new baseline or an exclusion of app coverage.
 */

test('generative v2.0: immutable revisions, pinned placements, optimistic writes, and checkpoints', async ({ desktop }) => {
	const { app, page, workspace, run } = desktop;
	const artifact = await createArtifact(desktop, run, 'Revenue baseline', secondSpec);
	const original = await invoke(page, 'artifact_resolve', { workspaceId: workspace, artifactId: artifact.id });
	expect(original.results).toHaveLength(1);
	expect(original.results[0]).toMatchObject({ id: 'sales-result', columns, exact_rows: exactRows, complete: true, row_count: 2 });
	const doc = await createDocument(desktop, [artifact], { body: 'Before the presentation edit.' });
	const checkpointed = await invoke(page, 'studio_checkpoint', { workspaceId: workspace, documentId: doc.id, baseVersion: doc.version, title: 'Original review' });
	expect(checkpointed.checkpoints).toHaveLength(1);
	const revision = await invoke(page, 'artifact_revise', {
		workspaceId: workspace, artifactId: artifact.id, baseRevision: artifact.current_revision,
		changeNote: 'Presentation-only revision; no new calculation.'
	});
	expect(revision.artifact.current_revision).toBe(artifact.current_revision);
	expect(revision.artifact.revisions).toHaveLength(2);
	const candidate = revision.revision;
	expect(candidate.run_id).toBe(run.id);
	expect(candidate.parent_revision).toBe(artifact.current_revision);
	const adopted = await invoke(page, 'artifact_adopt', { workspaceId: workspace, artifactId: artifact.id, baseRevision: artifact.current_revision, revision: candidate.revision });
	expect(adopted.artifact.current_revision).toBe(candidate.revision);
	for (const command of ['artifact_revise', 'artifact_adopt']) {
		const failure = await rejected(page, command, {
			workspaceId: workspace, artifactId: artifact.id, baseRevision: artifact.current_revision,
			revision: artifact.current_revision, changeNote: 'Stale mutation must fail.'
		});
		expect(failure.rejected, `${command} must reject a stale adopted base`).toBe(true);
		expect(failure.message).toMatch(/changed|reload|conflict|stale/i);
	}
	const pinned = await invoke(page, 'studio_load', { workspaceId: workspace, documentId: doc.id });
	expect(pinned.placements[0].revision).toBe(artifact.current_revision);
	const changed = await invoke(page, 'studio_save', {
		workspaceId: workspace, documentId: doc.id, baseVersion: pinned.version,
		update: { title: doc.title, body: 'After the presentation edit.', placements: [placement(adopted.artifact, 'placement-1')] }
	});
	expect((await rejected(page, 'studio_save', {
		workspaceId: workspace, documentId: doc.id, baseVersion: pinned.version,
		update: { title: doc.title, body: 'A stale write must not win.', placements: [] }
	})).rejected).toBe(true);
	const afterCheckpoint = await invoke(page, 'studio_checkpoint', { workspaceId: workspace, documentId: doc.id, baseVersion: changed.version, title: 'Revised review' });
	const restored = await invoke(page, 'studio_restore', {
		workspaceId: workspace, documentId: doc.id, baseVersion: afterCheckpoint.version,
		checkpointId: checkpointed.checkpoints[0].id
	});
	expect(restored.body).toBe('Before the presentation edit.');
	expect(restored.placements).toEqual(doc.placements);
	expect(restored.version).toBeGreaterThan(afterCheckpoint.version);
	expect(restored.checkpoints.map(item => item.title)).toEqual(['Original review', 'Revised review']);
	const latest = await invoke(page, 'artifact_resolve', { workspaceId: workspace, artifactId: artifact.id });
	expect(latest.artifact.revisions).toHaveLength(2);
	expect(latest.artifact.current_revision).toBe(candidate.revision);
	expect(latest.results[0].exact_rows).toEqual(exactRows);
	await assertCanonicalUnchanged(run);
	await assertNoExecution(desktop);
});

test('generative v2.0: workspace ownership cannot be bypassed by artifact or Studio IDs', async ({ desktop }) => {
	const { app, page, workspace, otherWorkspace, run } = desktop;
	const artifact = await createArtifact(desktop, run, 'Private commerce result', secondSpec);
	const doc = await createDocument(desktop, [artifact]);
	const checkpointed = await invoke(page, 'studio_checkpoint', { workspaceId: workspace, documentId: doc.id, baseVersion: doc.version, title: 'Commerce checkpoint' });
	const foreign = { workspaceId: otherWorkspace };
	const attempts = [
		['artifact_create', { ...foreign, turnId: run.id, title: 'Stolen run', spec: secondSpec }],
		['artifact_resolve', { ...foreign, artifactId: artifact.id }],
		['artifact_revise', { ...foreign, artifactId: artifact.id, baseRevision: artifact.current_revision, changeNote: 'Foreign revision' }],
		['artifact_adopt', { ...foreign, artifactId: artifact.id, baseRevision: artifact.current_revision, revision: artifact.current_revision }],
		['studio_load', { ...foreign, documentId: doc.id }],
		['studio_save', { ...foreign, documentId: doc.id, baseVersion: checkpointed.version, update: { title: 'Stolen document', body: '', placements: [] } }],
		['studio_checkpoint', { ...foreign, documentId: doc.id, baseVersion: checkpointed.version, title: 'Foreign checkpoint' }],
		['studio_restore', { ...foreign, documentId: doc.id, baseVersion: checkpointed.version, checkpointId: checkpointed.checkpoints[0].id }]
	];
	for (const [command, args] of attempts) {
		const failure = await rejected(page, command, args);
		expect(failure.rejected, `${command} must enforce the requested workspace, not only the record ID`).toBe(true);
		expect(failure.message).not.toContain('9007199254740993');
	}
	expect(await invoke(page, 'artifact_list', foreign)).toEqual([]);
	expect(await invoke(page, 'studio_list', foreign)).toEqual([]);
	const otherDocument = await invoke(page, 'studio_create', { ...foreign, title: 'Personal notes' });
	expect((await rejected(page, 'studio_save', {
		...foreign, documentId: otherDocument.id, baseVersion: otherDocument.version,
		update: { title: otherDocument.title, body: 'Must not attach foreign results.', placements: [placement(artifact, 'foreign-placement')] }
	})).rejected).toBe(true);
	const untouched = await invoke(page, 'studio_load', { ...foreign, documentId: otherDocument.id });
	expect(untouched.version).toBe(otherDocument.version);
	expect(untouched.placements).toEqual([]);
	expect(await invoke(page, 'studio_load', { workspaceId: workspace, documentId: doc.id })).toEqual(checkpointed);
	const excluded = await seedRun(desktop, { id: 'gen-excluded-run', resultId: 'excluded-result', excluded: true });
	const eligible = await invoke(page, 'presentation_results', { workspaceId: workspace, turnId: excluded.id });
	expect(eligible.results, 'verifier-excluded evidence must not enter presentation bindings').toEqual([]);
	const partial = await seedRun(desktop, { id: 'gen-partial-run', complete: false });
	const partialBindings = await invoke(page, 'presentation_results', { workspaceId: workspace, turnId: partial.id });
	expect(partialBindings.results[0]).toMatchObject({ complete: false, row_count: 3, exact_rows: exactRows });
	await assertCanonicalUnchanged(excluded);
	await assertCanonicalUnchanged(partial);
	await assertCanonicalUnchanged(run);
	await assertNoExecution(desktop);
});

test('generative v2.0: deleting a chat retains saved analytical work but not unrelated unsaved runs', async ({ desktop }) => {
	const { app, page, workspace, run } = desktop;
	const unsaved = await seedRun(desktop, { id: 'gen-unsaved-run' });
	const artifact = await createArtifact(desktop, run, 'Retained revenue comparison', comparisonSpec);
	const doc = await createDocument(desktop, [artifact], { title: 'Retained review' });
	const row = page.locator('.item-wrap').filter({ has: page.getByRole('button', { name: `Open conversation: ${question}`, exact: true }) });
	await row.hover();
	const remove = row.getByRole('button', { name: 'Delete conversation', exact: true });
	await remove.click();
	const confirmation = page.getByRole('dialog', { name: 'Delete conversation?', exact: true });
	await expect(confirmation).toContainText(/saved Studio outputs, their supporting analysis records, and revisions will remain available/i);
	await confirmation.getByRole('button', { name: 'Delete conversation', exact: true }).click();
	await expect(page.getByRole('button', { name: `Open conversation: ${question}`, exact: true })).toHaveCount(0);
	expect((await invoke(page, 'conversations_list')).some(chat => chat.id === 'gen-revenue-chat')).toBe(false);
	const retained = await invoke(page, 'artifact_resolve', { workspaceId: workspace, artifactId: artifact.id });
	expect(retained.artifact.saved).toBe(true);
	expect(retained.run.id).toBe(run.id);
	expect(retained.run.question).toBe(question);
	expect(retained.run.result.text).toBe(answer);
	expect(retained.results[0].exact_rows).toEqual(exactRows);
	expect((await invoke(page, 'studio_load', { workspaceId: workspace, documentId: doc.id })).placements).toEqual(doc.placements);
	await assertCanonicalUnchanged(run);
	await expect(readFile(unsaved.path, 'utf8')).rejects.toMatchObject({ code: 'ENOENT' });
	await page.reload();
	expect((await invoke(page, 'artifact_resolve', { workspaceId: workspace, artifactId: artifact.id })).results[0].exact_rows).toEqual(exactRows);
	await page.screenshot({ path: test.info().outputPath('saved-work-after-chat-deletion.png') });
	await assertNoExecution(desktop);
});

test('generative v2.0: recorded analysis to Studio, user edits and keyboard reorder survive a desktop restart', async ({ desktop }) => {
	const { page, workspace, profile, run } = desktop;
	const baselineFiles = (await readdir(join(profile, 'analysis', 'turns'))).sort();
	const first = await createArtifact(desktop, run, 'Revenue comparison', comparisonSpec);
	const second = await createArtifact(desktop, run, 'Supporting table', secondSpec);
	const studio = await openStudio(desktop);
	await studio.getByRole('button', { name: 'New document', exact: true }).click();
	const create = page.getByRole('dialog', { name: 'New Studio document', exact: true });
	await create.getByRole('textbox', { name: 'New Studio document name', exact: true }).fill('Quarterly working document');
	await create.getByRole('button', { name: 'Create document', exact: true }).click();
	await expect(create).toBeHidden();
	await addSavedOutput(desktop, studio, first.title);
	await addSavedOutput(desktop, studio, second.title);
	const comparison = studio.getByRole('article', { name: 'Saved output: Revenue comparison', exact: true });
	await expect(comparison.getByRole('heading', { name: 'Quarter comparison', exact: true })).toBeVisible();
	await expect(comparison.getByRole('img', { name: 'Revenue by quarter', exact: true }).locator('svg, canvas')).toBeVisible();
	await assertSavedRows(comparison, exactRows);
	const notes = '## Review notes\n\nKeep the supporting evidence before the main comparison.';
	await studio.getByRole('button', { name: 'Add notes', exact: true }).click();
	await studio.getByRole('textbox', { name: 'Studio notes', exact: true }).fill(notes);
	await studio.getByRole('button', { name: 'Done', exact: true }).click();
	// The same move control must work without a pointer or drag operation.
	const move = studio.getByRole('button', { name: 'Move output 2 up', exact: true });
	await move.focus();
	await move.press('Enter');
	await comparison.locator('summary').filter({ hasText: /^All saved values/ }).click();
	await comparison.getByRole('group', { name: 'Period', exact: true }).getByRole('button', { name: 'Q4', exact: true }).click();
	await assertSavedRows(comparison, [exactRows[1]]);
	const [doc] = await invoke(page, 'studio_list', { workspaceId: workspace });
	await expect.poll(async () => {
		const saved = await invoke(page, 'studio_load', { workspaceId: workspace, documentId: doc.id });
		return {
			body: saved.body,
			order: saved.placements.map(item => item.artifact_id),
			selection: saved.placements.find(item => item.artifact_id === first.id)?.view_state?.['filter:selected-period']
		};
	}).toEqual({ body: notes, order: [second.id, first.id], selection: { resultId: 'sales-result', field: 'period', value: 'Q4' } });
	await assertCanonicalUnchanged(run);
	const beforeRestart = await assertNoExecution(desktop);
	await test.info().attach('before-restart-engine-requests', { body: JSON.stringify(beforeRestart, null, 2), contentType: 'application/json' });
	await page.screenshot({ path: test.info().outputPath('studio-before-restart.png') });
	await desktop.app.close();
	desktop.app = null;
	Object.assign(desktop, await launch(profile));
	await startBudget(desktop);
	// This reload is observed from its first renderer request, so the zero-call
	// claim does not depend on a race attaching instrumentation during startup.
	await desktop.page.reload();
	const reopened = desktop.page.getByRole('region', { name: 'Studio', exact: true });
	await expect(reopened.getByRole('textbox', { name: 'Studio document title', exact: true })).toHaveValue('Quarterly working document');
	await expect(reopened.getByRole('region', { name: 'Document notes', exact: true })).toContainText('Keep the supporting evidence before the main comparison.');
	await expect.poll(() => reopened.getByRole('list', { name: 'Studio outputs', exact: true }).getByRole('article').evaluateAll(elements => elements.map(element => element.getAttribute('aria-label')))).toEqual(['Saved output: Supporting table', 'Saved output: Revenue comparison']);
	await assertSavedRows(reopened.getByRole('article', { name: 'Saved output: Revenue comparison', exact: true }), [exactRows[1]]);
	expect((await readdir(join(profile, 'analysis', 'turns'))).sort()).toEqual(baselineFiles);
	await assertCanonicalUnchanged(run);
	await assertNoExecution(desktop);
	await desktop.page.setViewportSize({ width: 960, height: 680 });
	await expect(reopened.getByRole('textbox', { name: 'Studio document title', exact: true })).toBeInViewport();
	await expect(reopened.getByRole('button', { name: 'Export', exact: true })).toBeInViewport();
	await desktop.page.screenshot({ path: test.info().outputPath('studio-reopened-compact.png') });
});

import { test, expect, question, invoke, createArtifact, createDocument, seedRun, chooseWorkspace, assertNoExecution, assertCanonicalUnchanged } from './generative-workspace-harness.mjs';

// Chat/Notebook v1: frozen in docs/testing/CHAT-NOTEBOOK-v1.md.
// Real Electron/Rust records; no model or analytics calls are permitted by the
// shared harness. These new cases do not alter pre-existing suite expectations.
const program = 'root = Document([Title("Quarterly revenue"), Metric("sales-result", "revenue", 1, "Q4 revenue", "USD"), Table("sales-result")])';
// v1.1 user-approved setup correction: the archive is a fresh conversation,
// not a disk replacement for a live one. Behavioral assertions are unchanged.
const conversationId = 'notebook-fixture-chat';
const conversationTitle = 'Notebook layout fixture';

async function openSavedAnswer(desktop, output) {
 const { page, workspace } = desktop;
 const run = output.run;
 await invoke(page, 'archive_conversation', { id: conversationId, body: JSON.stringify({
  id: conversationId, title: conversationTitle, workspace, saved_at_ms: Date.now(), messages: [
   { id: 'notebook-fixture-user', role: 'user', text: question, ts: 1 },
   { id: 'saved-output', role: 'assistant', text: run.result.text, ts: 2,
    answer: { ...run.result, turn_id: run.id, workspace: { path: workspace, revision: run.workspace_revision } },
    artifact_id: output.artifact.id, artifact_revision: output.revision.revision,
    presentation_snapshot: { workspace, turn_id: run.id, program: output.revision.program, results: output.results }
   }
  ]
 }) });
 await page.reload();
 await chooseWorkspace(page, 'commerce');
 await page.getByRole('button', { name: `Open conversation: ${conversationTitle}`, exact: true }).click();
 await expect(page.locator('.analytical-answer').getByRole('heading', { name: 'Quarterly revenue', exact: true })).toBeVisible();
}

test('Chat and Notebook retain the conversation, draft and saved result at wide and narrow widths', async ({ desktop }) => {
 const { page, workspace } = desktop;
 desktop.run = await seedRun(desktop, { id: 'notebook-original-run', conversationId });
 const artifact = await createArtifact(desktop, desktop.run, 'Quarterly revenue', program);
 const output = await invoke(page, 'artifact_resolve', { workspaceId: workspace, artifactId: artifact.id });
 const investigation = await invoke(page, 'investigation_ensure', { workspaceId: workspace, conversationId, initialRequest: question, sourceMessageId: 'notebook-fixture-user' });
 await invoke(page, 'investigation_link_analysis_turns', { investigationId: investigation.id, analysisTurnIds: [desktop.run.id] });
 await openSavedAnswer(desktop, output);
 const composer = page.getByRole('combobox', { name: /Ask about/ });
 await composer.fill('Keep this draft while changing layout');
 const transcript = await page.getByRole('log', { name: 'conversation', exact: true }).elementHandle();
 const marker = await page.evaluate(() => { document.__notebookMarker = crypto.randomUUID(); return document.__notebookMarker; });
 await page.getByRole('button', { name: 'Notebook', exact: true }).click();
 await expect(page.locator('.workspace-chat-layout')).toHaveAttribute('data-notebook', 'true');
 await expect(page.locator('.workspace-tools-pane')).toContainText('Quarterly revenue');
 await expect(composer).toHaveValue('Keep this draft while changing layout');
 const rectangles = await page.locator('.workspace-chat-layout').evaluate(host => ({
  document: host.querySelector('.workspace-tools-pane').getBoundingClientRect().x,
  chat: host.querySelector('.workspace-conversation-pane').getBoundingClientRect().x
 }));
 expect(rectangles.document).toBeLessThan(rectangles.chat);
 await page.screenshot({ path: test.info().outputPath('wide-notebook.png') });
 await page.setViewportSize({ width: 600, height: 720 });
 await expect(page.getByRole('tab', { name: 'Document', exact: true })).toBeVisible();
 await page.getByRole('tab', { name: 'Chat', exact: true }).click();
 await expect(composer).toBeVisible();
 await expect(composer).toHaveValue('Keep this draft while changing layout');
 expect(await transcript.evaluate(element => element.isConnected)).toBe(true);
 await page.getByRole('tab', { name: 'Document', exact: true }).click();
 await expect(page.locator('.workspace-tools-pane')).toBeVisible();
 await page.screenshot({ path: test.info().outputPath('compact-notebook.png') });
 expect(await page.evaluate(() => document.documentElement.scrollWidth <= document.documentElement.clientWidth + 1)).toBe(true);
 await page.getByRole('button', { name: 'Chat', exact: true }).click();
 await expect(composer).toBeVisible();
 await expect(composer).toHaveValue('Keep this draft while changing layout');
 expect(await transcript.evaluate(element => element.isConnected)).toBe(true);
 expect(await page.evaluate(() => document.__notebookMarker)).toBe(marker);
 await assertNoExecution(desktop); await assertCanonicalUnchanged(desktop.run);
 await page.screenshot({ path: test.info().outputPath('compact-chat.png') });
});

test('a candidate can be reviewed from chat without adopting it or changing a pinned report', async ({ desktop }) => {
 const { page, workspace } = desktop;
 desktop.run = await seedRun(desktop, { id: 'notebook-original-run', conversationId });
 const artifact = await createArtifact(desktop, desktop.run, 'Revenue revision', program);
 const report = await createDocument(desktop, [artifact], { title: 'Pinned quarterly report' });
 const candidateRun = await seedRun(desktop, { id: 'notebook-candidate-run', conversationId, amounts: [10, 30] });
 const candidate = await invoke(page, 'artifact_revise', { workspaceId: workspace, artifactId: artifact.id, baseRevision: 1, runId: candidateRun.id, program, changeNote: 'Updated revenue calculation.' });
 expect(candidate.artifact.current_revision).toBe(1);
 await openSavedAnswer(desktop, candidate);
 await page.getByRole('button', { name: 'Review changes', exact: true }).click();
 const diff = page.getByRole('region', { name: 'Analytical comparison', exact: true });
 await expect(diff).toBeVisible();
 for (const name of ['Meaning', 'Inputs', 'Method', 'Results', 'Impact']) await expect(diff.getByRole('heading', { name, exact: true })).toBeVisible();
 await expect(diff).toContainText('Pinned quarterly report');
 expect((await invoke(page, 'artifact_resolve', { workspaceId: workspace, artifactId: artifact.id })).artifact.current_revision).toBe(1);
 await diff.getByRole('button', { name: 'Use proposed revision', exact: true }).click();
 await expect.poll(async () => (await invoke(page, 'artifact_resolve', { workspaceId: workspace, artifactId: artifact.id })).artifact.current_revision).toBe(2);
 const reports = await invoke(page, 'studio_list', { workspaceId: workspace });
 expect(reports.find(item => item.id === report.id).placements[0].revision).toBe(1);
 await assertNoExecution(desktop); await assertCanonicalUnchanged(desktop.run); await assertCanonicalUnchanged(candidateRun);
});

test('Notebook naming, files and new reports preserve the linked conversation', async ({ desktop }) => {
 const { page, workspace } = desktop;
 desktop.run = await seedRun(desktop, { id: 'notebook-original-run', conversationId });
 const artifact = await createArtifact(desktop, desktop.run, 'Quarterly revenue', program);
 const output = await invoke(page, 'artifact_resolve', { workspaceId: workspace, artifactId: artifact.id });
 const investigation = await invoke(page, 'investigation_ensure', { workspaceId: workspace, conversationId, initialRequest: question, sourceMessageId: 'notebook-fixture-user' });
 await invoke(page, 'investigation_link_analysis_turns', { investigationId: investigation.id, analysisTurnIds: [desktop.run.id] });
 await openSavedAnswer(desktop, output);
 const composer = page.getByRole('combobox', { name: /Ask about/ });
 await composer.fill('Follow-up draft');
 await page.getByRole('button', { name: 'Notebook', exact: true }).click();
 await page.getByRole('button', { name: 'Rename Investigation', exact: true }).click();
 await page.getByRole('textbox', { name: 'Investigation name', exact: true }).fill('Revenue review');
 await page.getByRole('button', { name: 'Save name', exact: true }).click();
 await expect.poll(async () => (await invoke(page, 'investigation_load', { investigationId: investigation.id })).title).toBe('Revenue review');
 await page.getByRole('button', { name: 'Files', exact: true }).click();
 await expect(page.locator('.sidebar')).toContainText('sales.csv');
 await page.getByRole('button', { name: 'Reports', exact: true }).click();
 await page.getByRole('button', { name: 'New report', exact: true }).click();
 const dialog = page.getByRole('dialog', { name: 'New report', exact: true });
 await expect(dialog).toBeVisible();
 await dialog.getByRole('textbox', { name: 'New report name', exact: true }).fill('Decision summary');
 await dialog.getByRole('button', { name: 'Create report', exact: true }).click();
 await expect(dialog).toBeHidden();
 await expect(page.getByRole('textbox', { name: 'Report title', exact: true })).toHaveValue('Decision summary');
 await expect(composer).toHaveValue('Follow-up draft');
 await expect(page.locator('.workspace-conversation-pane .analytical-answer')).toContainText('Quarterly revenue');
 await assertNoExecution(desktop); await assertCanonicalUnchanged(desktop.run);
});

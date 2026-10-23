import { test, expect, question, invoke, createArtifact, createDocument, seedRun, chooseWorkspace, assertNoExecution, assertCanonicalUnchanged } from './generative-workspace-harness.mjs';

// Chat/Notebook v1: frozen in docs/testing/CHAT-NOTEBOOK-v1.md.
// Real Electron/Rust records; no model or analytics calls are permitted by the
// shared harness. These new cases do not alter pre-existing suite expectations.
const spec = { root: 'doc', elements: {
 doc: { type: 'Document', props: {}, children: ['title', 'metric', 'table'] },
 title: { type: 'Title', props: { content: 'Quarterly revenue' }, children: [] },
 metric: { type: 'Metric', props: { resultId: 'sales-result', field: 'revenue', row: 1, title: 'Q4 revenue', unit: 'USD' }, children: [] },
 table: { type: 'Table', props: { resultId: 'sales-result' }, children: [] }
} };
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
    presentation_snapshot: { workspace, turn_id: run.id, spec: output.revision.spec, results: output.results }
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
 const artifact = await createArtifact(desktop, desktop.run, 'Quarterly revenue', spec);
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
 const artifact = await createArtifact(desktop, desktop.run, 'Revenue revision', spec);
 const report = await createDocument(desktop, [artifact], { title: 'Pinned quarterly report' });
 const candidateRun = await seedRun(desktop, { id: 'notebook-candidate-run', conversationId, amounts: [10, 30] });
 const candidate = await invoke(page, 'artifact_revise', { workspaceId: workspace, artifactId: artifact.id, baseRevision: 1, runId: candidateRun.id, spec, changeNote: 'Updated revenue calculation.' });
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
 const artifact = await createArtifact(desktop, desktop.run, 'Quarterly revenue', spec);
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

test('Notebook titles legacy runs with the captured Question instead of repeated plan scaffolding', async ({ desktop }) => {
 const { page, workspace } = desktop;
 const conversationId = 'notebook-legacy-question-chat';
 const conversationTitle = 'Legacy Question title fixture';
 const mainQuestion = 'Why did Northstar Outfitters’ net merchandise revenue change between Q3 and Q4 2025?';
 const selectedQuestion = 'Which product categories saw the largest increases or decreases in revenue between Q3 and Q4 2025?';
 const legacyPrompt = `Use this current analyst-edited Question plan as context for the request below. Keep the immediate request primary, use the plan to understand the broader Investigation, and do not present unanswered Questions as findings. Current Question plan: Main Question: ${mainQuestion} Subquestion 1: What was the total net merchandise revenue for Northstar Outfitters in Q3 and Q4 2025? Subquestion 2: How did the average order value (AOV) change between Q3 and Q4 2025? Subquestion 3: What was the change in total order volume between Q3 and Q4 2025? Subquestion 4: ${selectedQuestion} Subquestion 5: How did the rate of returns and allowances change between Q3 and Q4 2025? Immediate request: ${selectedQuestion}`;

 await invoke(page, 'archive_conversation', { id: conversationId, body: JSON.stringify({
  id: conversationId, title: conversationTitle, workspace, saved_at_ms: Date.now(), messages: [
   { id: 'legacy-question-user', role: 'user', text: mainQuestion, ts: 1 }
  ]
 }) });
 const investigation = await invoke(page, 'investigation_ensure', {
  workspaceId: workspace, conversationId, initialRequest: mainQuestion, sourceMessageId: 'legacy-question-user'
 });
 await invoke(page, 'investigation_update_questions', { investigationId: investigation.id, questions: [
  { id: 'northstar-main', text: mainQuestion },
  { id: 'northstar-categories', parent_id: 'northstar-main', text: selectedQuestion }
 ] });
 desktop.run = await seedRun(desktop, {
  id: 'notebook-legacy-question-run', conversationId, question: legacyPrompt,
  investigationContext: {
   investigation_id: investigation.id, question_id: 'northstar-categories', question_text: selectedQuestion,
   definitions: [], source_selection: { mode: 'all_sources', included_paths: [] }
  }
 });
 await invoke(page, 'investigation_link_analysis_turns', { investigationId: investigation.id, analysisTurnIds: [desktop.run.id] });
 await invoke(page, 'investigation_set_analysis_turn_questions', {
  investigationId: investigation.id, analysisTurnId: desktop.run.id, questionIds: ['northstar-categories']
 });
 const run = await invoke(page, 'analysis_turn_load', { turnId: desktop.run.id });
 expect(run.question).toBe(legacyPrompt); // Legacy provenance remains unchanged.
 await invoke(page, 'archive_conversation', { id: conversationId, body: JSON.stringify({
  id: conversationId, title: conversationTitle, workspace, saved_at_ms: Date.now(), messages: [
   { id: 'legacy-question-user', role: 'user', text: mainQuestion, ts: 1 },
   { id: 'legacy-question-answer', role: 'assistant', text: run.result.text, ts: 2,
    answer: { ...run.result, turn_id: run.id, workspace: { path: workspace, revision: run.workspace_revision } } }
  ]
 }) });
 await page.reload();
 await chooseWorkspace(page, 'commerce');
 await page.getByRole('button', { name: `Open conversation: ${conversationTitle}`, exact: true }).click();
 await page.getByRole('button', { name: 'Notebook', exact: true }).click();

 const output = page.locator(`.notebook-output[data-analysis-turn-id="${desktop.run.id}"]`);
 await expect(output.getByRole('heading', { name: selectedQuestion, exact: true })).toBeVisible();
 await expect(output).not.toContainText('Use this current analyst-edited Question plan as context');
 await assertCanonicalUnchanged(desktop.run);
});

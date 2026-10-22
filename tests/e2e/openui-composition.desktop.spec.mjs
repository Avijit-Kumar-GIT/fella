import { test, expect, createArtifact, createDocument, openStudioDocument, assertNoExecution, assertCanonicalUnchanged } from './generative-workspace-harness.mjs';

// Composition regression v1.1: real Electron/Svelte/Vega, recorded data, no
// mocked renderer. Expected chart mounting/drawing is independent of model
// choices; the companion live suite checks model-selected presentations.
const chart = 'Chart("sales-result", {"mark":"bar","encoding":{"x":{"field":"period","type":"nominal"},"y":{"field":"revenue","type":"quantitative"}}}, "Nested revenue")';

async function expectDrawnChart(output) {
	const canvas = output.getByRole('img', { name: 'Nested revenue', exact: true }).locator('canvas');
	await expect(canvas).toBeVisible();
	await expect.poll(() => canvas.evaluate((element) => {
		const { width, height } = element;
		if (width < 100 || height < 50) return false;
		const context = element.getContext('2d');
		if (!context) return false;
		const pixels = context.getImageData(0, 0, width, height).data;
		let filled = 0;
		for (let index = 3; index < pixels.length; index += 4) if (pixels[index] > 0 && ++filled > 100) return true;
		return false;
	}), { message: 'the chart canvas must contain drawn pixels' }).toBe(true);
	await expect(output.getByText(/Visualization unavailable/)).toHaveCount(0);
}

test('OpenUI layouts draw their nested charts and preserve their metrics', async ({ desktop }) => {
	const content = `Section("Nested content", [Metric("sales-result", "revenue", 0, "Saved revenue"), ${chart}])`;
	const layouts = {
		Row: (child) => `Row([${child}])`,
		Grid: (child) => `Grid([${child}], 2)`,
		Columns: (child) => `Columns([${child}], 2)`,
		Tabs: (child) => `Tabs("Review", ["Summary"], [${child}])`,
		Nested: (child) => `Columns([Grid([Row([Tabs("Review", ["Summary"], [${child}])])])])`
	};
	for (const [name, wrap] of Object.entries(layouts)) {
		const artifact = await createArtifact(desktop, desktop.run, `OpenUI ${name}`, `root = Document([${wrap(content)}, Table("sales-result")])`);
		const doc = await createDocument(desktop, [artifact], { title: `Composition ${name}` });
		const studio = await openStudioDocument(desktop, doc);
		const output = studio.getByRole('article', { name: `Saved output: OpenUI ${name}`, exact: true });
		await expect(output.getByText('Saved revenue', { exact: true })).toBeVisible();
		await expectDrawnChart(output);
	}
	await assertCanonicalUnchanged(desktop.run);
	await assertNoExecution(desktop);
	await desktop.page.screenshot({ path: test.info().outputPath('nested-openui-chart.png') });
});

test('OpenUI tab changes mount the selected chart and records without analysis calls', async ({ desktop }) => {
	const program = `root = Document([Tabs("Review", ["Chart", "Records"], [${chart}, Table("sales-result")])])`;
	const artifact = await createArtifact(desktop, desktop.run, 'Tabbed OpenUI', program);
	const doc = await createDocument(desktop, [artifact], { title: 'Tabbed composition' });
	const studio = await openStudioDocument(desktop, doc);
	const output = studio.getByRole('article', { name: 'Saved output: Tabbed OpenUI', exact: true });
	await expectDrawnChart(output);
	await output.getByRole('tab', { name: 'Records', exact: true }).click();
	await expect(output.getByRole('tabpanel').getByRole('table')).toBeVisible();
	await output.getByRole('tab', { name: 'Chart', exact: true }).click();
	await expectDrawnChart(output);
	await assertNoExecution(desktop);
});

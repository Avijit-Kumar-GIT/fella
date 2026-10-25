import { defineConfig } from '@playwright/test';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

// Focused real-desktop coverage for generated presentations and saved artifacts.
// Build the app and Rust sidecar beforehand; these cases use local fixtures.
export default defineConfig({
	testDir: './',
	testMatch: ['**/generative-workspace.desktop.spec.mjs', '**/studio-artifacts.desktop.spec.mjs', '**/chat-notebook.desktop.spec.mjs'],
	fullyParallel: false,
	workers: 1,
	retries: 0,
	timeout: 45_000,
	expect: { timeout: 7_000 },
	reporter: 'list',
	outputDir: join(tmpdir(), 'fella-json-render-workspace-v2-results'),
	use: { trace: 'retain-on-failure', screenshot: 'only-on-failure' }
});

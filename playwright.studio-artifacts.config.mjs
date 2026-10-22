import { defineConfig } from '@playwright/test';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

// Organization-only split of the existing generative workspace contract v1.5.
// Run after the same app/sidecar build as playwright.generative.config.mjs.
export default defineConfig({
	testDir: './tests/e2e',
	testMatch: '**/studio-artifacts.desktop.spec.mjs',
	fullyParallel: false,
	workers: 1,
	retries: 0,
	timeout: 45_000,
	expect: { timeout: 7_000 },
	reporter: 'list',
	outputDir: join(tmpdir(), 'fella-studio-artifacts-v1.5-results'),
	use: { trace: 'retain-on-failure', screenshot: 'only-on-failure' }
});

import { defineConfig } from '@playwright/test';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

// Independent real-desktop suite. Build the app and Rust sidecar beforehand;
// no dev server, hosted model, existing config, or assertion is replaced.
export default defineConfig({
	testDir: './tests/e2e',
	testMatch: '**/generative-workspace.desktop.spec.mjs',
	fullyParallel: false,
	workers: 1,
	retries: 0,
	timeout: 45_000,
	expect: { timeout: 7_000 },
	reporter: 'list',
	outputDir: join(tmpdir(), 'fella-generative-workspace-v1.2-results'),
	use: { trace: 'retain-on-failure', screenshot: 'only-on-failure' }
});

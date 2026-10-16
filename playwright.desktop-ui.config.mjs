import { defineConfig } from '@playwright/test';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

// Real Electron + Rust, disposable profiles, no hosted-model calls.
export default defineConfig({
	testDir: './tests/e2e', testMatch: '**/workspace-ui.desktop.spec.mjs',
	workers: 1, retries: 0, timeout: 40_000, expect: { timeout: 7000 },
	reporter: 'list', outputDir: join(tmpdir(), 'fella-desktop-ui-results'),
	use: { trace: 'retain-on-failure', screenshot: 'only-on-failure' }
});

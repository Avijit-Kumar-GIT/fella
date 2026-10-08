import { defineConfig } from '@playwright/test';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

export default defineConfig({
	testDir: './tests/e2e',
	testMatch: '**/workspace-board.spec.mjs',
	fullyParallel: false,
	workers: 1,
	retries: 0,
	timeout: 30_000,
	expect: { timeout: 5_000 },
	reporter: 'list',
	outputDir: join(tmpdir(), 'fella-workspace-board-playwright-results'),
	use: {
		baseURL: 'http://127.0.0.1:1421',
		trace: 'off',
		video: 'off',
		screenshot: 'only-on-failure'
	},
	webServer: {
		command: 'pnpm dev --host 127.0.0.1 --port 1421',
		url: 'http://127.0.0.1:1421',
		reuseExistingServer: !process.env.CI,
		timeout: 60_000
	}
});

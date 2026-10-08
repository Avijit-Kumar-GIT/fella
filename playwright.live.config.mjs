import { tmpdir } from 'node:os';
import { join } from 'node:path';

/**
 * Opt-in, credentialed Electron acceptance tests. These are intentionally kept
 * separate from the fast, offline test suite because they call a real provider.
 */
export default {
	testDir: './tests/e2e',
	testMatch: '**/*.live.spec.mjs',
	fullyParallel: false,
	workers: 1,
	retries: 0,
	timeout: 180_000,
	expect: { timeout: 90_000 },
	reporter: 'list',
	outputDir: join(tmpdir(), 'fella-playwright-live-results'),
	use: {
		trace: 'off',
		video: 'off',
		screenshot: 'off'
	}
};

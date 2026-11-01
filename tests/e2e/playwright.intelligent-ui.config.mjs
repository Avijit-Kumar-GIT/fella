import { defineConfig } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createServer } from 'vite';

const port = Number(process.env.FELLA_UI_CONTRACT_PORT ?? 1437);
const origin = `http://127.0.0.1:${port}`;

// The parent builds this shared native/WASM validator before browser execution.
// This server deliberately does not build or replace acceptance code.
if (process.argv.includes('--serve')) {
	const bytes = await readFile('src/lib/presentation/generated/contract.wasm').catch(() => {
		throw new Error('Parent readiness required: run node scripts/build-presentation-contract.mjs first.');
	});
	if (!bytes.length) throw new Error('The generated presentation contract WASM is empty.');
	const server = await createServer({
		configFile: 'vite.config.ts',
		plugins: [{
			name: 'fella-ui-contract-harness',
			configureServer(vite) {
				// Register now, before Vite/SvelteKit internal middleware; returning a
				// callback here would put this route behind the application fallback.
				vite.middlewares.use((request, response, next) => {
					if (request.url?.split('?')[0] !== '/__fella_ui_contract') return next();
					void vite.transformIndexHtml(request.url, `<!doctype html>
<html lang="en"><head><meta charset="utf-8"><title>Frozen UI contract</title></head>
<body><div id="contract-root"></div><script type="module" src="/tests/fixtures/intelligent-ui-harness.ts"></script></body></html>`)
						.then(html => {
							response.setHeader('Content-Type', 'text/html; charset=utf-8');
							response.end(html);
						}).catch(next);
				});
			}
		}],
		server: { host: '127.0.0.1', port, strictPort: true, hmr: false, watch: null }
	});
	await server.listen();
	for (const signal of ['SIGTERM', 'SIGINT']) process.once(signal, () => {
		void server.close().then(() => process.exit(0));
	});
}

export default defineConfig({
	testDir: './',
	testMatch: 'intelligent-ui.contract.spec.mjs',
	workers: 1,
	fullyParallel: false,
	retries: 0,
	timeout: 45_000,
	expect: { timeout: 8_000 },
	reporter: 'list',
	outputDir: join(tmpdir(), 'fella-intelligent-ui-contract-results'),
	use: {
		baseURL: origin,
		browserName: 'chromium',
		headless: true,
		viewport: { width: 1100, height: 850 },
		locale: 'en-US',
		trace: 'retain-on-failure',
		screenshot: 'only-on-failure'
	},
	webServer: {
		command: 'node tests/e2e/playwright.intelligent-ui.config.mjs --serve',
		cwd: new URL('../..', import.meta.url).pathname,
		url: `${origin}/__fella_ui_contract`,
		reuseExistingServer: false,
		timeout: 60_000
	}
});

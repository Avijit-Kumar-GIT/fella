import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vite';
import { createHash } from 'node:crypto';
import { resolve } from 'node:path';

const devRootKey = createHash('sha256')
	.update(resolve(process.cwd()).replaceAll('\\', '/').toLowerCase())
	.digest('hex')
	.slice(0, 16);

// @sveltejs/kit in this version takes adapter/config through the plugin options
// rather than a separate svelte.config.js.
export default defineConfig({
	plugins: [
		tailwindcss(),
		sveltekit({
			compilerOptions: {
				// Force runes mode for the project, except for libraries. Can be removed in svelte 6.
				runes: ({ filename }) =>
					filename.split(/[/\\]/).includes('node_modules') ? undefined : true
			},
			// Electron serves a static bundle; no server runtime.
			adapter: adapter({
				pages: 'build',
				assets: 'build',
				fallback: 'fella-fallback.html',
				precompress: false,
				strict: true
			})
		})
	],

	// Electron development prefers port 1420 and selects a nearby free port if
	// another process already owns it.
	clearScreen: false,
	// These dependencies are reached through lazy Svelte panes and renderers.
	// Pre-bundling the known set together avoids several successive optimizer
	// passes (and renderer reloads) during the first Electron dev launch.
	optimizeDeps: {
		include: [
			'@json-render/svelte',
			'@json-render/svelte/schema',
			'@lucide/svelte',
			'bits-ui',
			'd3-scale',
			'd3-shape',
			'html-to-image',
			'marked',
			'tailwind-merge',
			'tailwind-variants',
			'vega-interpreter',
			'vega-lite',
			'vega',
			'zod/v4'
		]
	},
	server: {
		host: '127.0.0.1',
		port: Number(process.env.FELLA_VITE_PORT ?? 1420),
		strictPort: true,
		headers: { 'X-Fella-Dev-Root': devRootKey },
		watch: {
			// Rust changes are rebuilt by Cargo, not Vite.
			ignored: ['**/backend/**']
		}
	},
	envPrefix: ['VITE_']
});

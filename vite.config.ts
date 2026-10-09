import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vite';

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
				fallback: 'index.html',
				precompress: false,
				strict: true
			})
		})
	],

	// Electron development uses the same stable Vite port in electron/dev.mjs.
	clearScreen: false,
	server: {
		port: 1420,
		strictPort: true,
		watch: {
			// Rust changes are rebuilt by Cargo, not Vite.
			ignored: ['**/backend/**']
		}
	},
	envPrefix: ['VITE_']
});

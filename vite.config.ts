import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

// @sveltejs/kit in this version takes adapter/config through the plugin options
// rather than a separate svelte.config.js.
export default defineConfig({
	plugins: [
		sveltekit({
			compilerOptions: {
				// Force runes mode for the project, except for libraries. Can be removed in svelte 6.
				runes: ({ filename }) =>
					filename.split(/[/\\]/).includes('node_modules') ? undefined : true
			},
			// Tauri serves a static bundle; no server runtime.
			adapter: adapter({
				pages: 'build',
				assets: 'build',
				fallback: 'index.html',
				precompress: false,
				strict: true
			})
		})
	],

	// --- Tauri integration -------------------------------------------------
	// Tauri expects a fixed dev server it can point the webview at.
	clearScreen: false,
	server: {
		port: 1420,
		strictPort: true,
		watch: {
			// src-tauri is watched by the Tauri CLI, not Vite.
			ignored: ['**/src-tauri/**']
		}
	},
	// Only VITE_ and TAURI_ vars are exposed to the frontend.
	envPrefix: ['VITE_', 'TAURI_ENV_']
});

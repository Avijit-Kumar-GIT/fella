import { createServer } from 'vite';
import { writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';

// Generate the engine-facing schema and prompt from the same catalog used by the Svelte renderer.
const server = await createServer({ configFile: 'vite.config.ts', appType: 'custom', logLevel: 'error', optimizeDeps: { noDiscovery: true, entries: [] }, server: { middlewareMode: true, hmr: false, ws: false } });
try {
	const { presentationManifest } = await server.ssrLoadModule('/src/lib/presentation/manifest.ts');
	const manifest = presentationManifest();
	manifest.hash = createHash('sha256').update(JSON.stringify(manifest.schema)).digest('hex');
	await writeFile('backend/resources/presentation-catalog.json', JSON.stringify(manifest, null, 2) + '\n');
	console.log(`Generated ${manifest.version} (${manifest.componentNames.length} components; schema ${manifest.hash.slice(0, 12)})`);
} finally { await server.close(); }

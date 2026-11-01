import { mkdir, readFile, readdir, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';

const cachePath = 'src/lib/presentation/generated/catalog-cache.json';
const outputPaths = ['backend/resources/presentation-catalog.json', 'backend/resources/presentation-contract.json'];
const digest = (bytes) => createHash('sha256').update(bytes).digest('hex');

async function sourcePaths(directory, extension) {
	const entries = await readdir(directory, { withFileTypes: true });
	const paths = await Promise.all(entries.map((entry) => {
		// Use repository-relative forward-slash paths on every platform. Only
		// source files participate; generated Wasm and the cache cannot feed back.
		const path = `${directory}/${entry.name}`;
		if (entry.isDirectory()) return sourcePaths(path, extension);
		return entry.isFile() && entry.name.endsWith(extension) ? [path] : [];
	}));
	return paths.flat();
}

async function inputHash() {
	const paths = [
		...await sourcePaths('src/lib/presentation', '.ts'),
		...await sourcePaths('crates/presentation-contract/src', '.rs'),
		'crates/presentation-contract/Cargo.toml',
		'backend/Cargo.toml',
		'backend/Cargo.lock',
		'scripts/generate-presentation-catalog.mjs',
		'package.json',
		'pnpm-lock.yaml',
		'vite.config.ts'
	].sort();
	// Encode path/content-hash pairs to avoid ambiguous concatenations. Hash
	// bytes, not mtimes or decoded text, so edits and deletions invalidate safely.
	return digest(JSON.stringify(await Promise.all(paths.map(async (path) => [path, digest(await readFile(path))]))));
}

async function outputHashes() {
	return Object.fromEntries(await Promise.all(outputPaths.map(async (path) => [path, digest(await readFile(path))])));
}

async function cacheMatches(inputs) {
	try {
		const cache = JSON.parse(await readFile(cachePath, 'utf8'));
		if (cache.version !== 1 || cache.inputHash !== inputs) return false;
		const outputs = await outputHashes();
		return outputPaths.every((path) => cache.outputs?.[path] === outputs[path]);
	} catch {
		// Missing/corrupted cache or resources require full safety validation.
		return false;
	}
}

async function generate() {
	const inputs = await inputHash();
	if (!process.argv.includes('--force') && await cacheMatches(inputs)) {
		console.log('Presentation catalog cache hit (inputs and outputs verified)');
		return;
	}

	// Generate the engine-facing schema and prompt from the same catalog used by the Svelte renderer.
	// Import Vite only on a miss: even loading its dependency graph costs time.
	const { createServer } = await import('vite');
	const server = await createServer({ configFile: 'vite.config.ts', appType: 'custom', logLevel: 'error', optimizeDeps: { noDiscovery: true, entries: [] }, server: { middlewareMode: true, hmr: false, ws: false } });
	try {
		const { presentationManifest } = await server.ssrLoadModule('/src/lib/presentation/manifest.ts');
		const manifest = presentationManifest();
		// Fail generation rather than silently ship a schema keyword the shared
		// native/WASM interpreter does not enforce.
		const keywords = new Set(['$schema','$ref','$defs','type','const','enum','anyOf','oneOf','allOf','properties','required','additionalProperties','propertyNames','items','minItems','maxItems','uniqueItems','minLength','maxLength','pattern','minimum','maximum','exclusiveMinimum','exclusiveMaximum','description','title','default']);
		function supported(schema) {
			if (typeof schema === 'boolean') { if (!schema) throw new Error('The contract cannot interpret a false schema.'); return; }
			for (const [key,value] of Object.entries(schema)) {
				if (!keywords.has(key)) throw new Error(`Unsupported shared contract schema keyword: ${key}`);
				if (['$defs','properties'].includes(key)) Object.values(value).forEach(supported);
				else if (['anyOf','oneOf','allOf'].includes(key)) value.forEach(supported);
				else if (['items','propertyNames'].includes(key) || key === 'additionalProperties' && typeof value === 'object') supported(value);
			}
		}
		supported(manifest.authorSchema);
		Object.values(manifest.componentSchemas).forEach(supported);
		manifest.implementationHash = createHash('sha256').update(await readFile('crates/presentation-contract/src/lib.rs')).update(await readFile('crates/presentation-contract/src/schema.rs')).digest('hex');
		manifest.hash = createHash('sha256').update(JSON.stringify({schema: manifest.schema, authorSchema: manifest.authorSchema, components: manifest.componentSchemas, slots: manifest.slots, limits: manifest.limits, capabilities: manifest.capabilities, implementationHash: manifest.implementationHash})).digest('hex');
		const output = JSON.stringify(manifest, null, 2) + '\n';
		if (await readFile('backend/resources/presentation-catalog.json','utf8').catch(()=>null) !== output) await writeFile('backend/resources/presentation-catalog.json', output);
		// The shared acceptance binary needs no generation prompt or redundant
		// renderer schema. Keep its embedded manifest small and hash-identical.
		const contract = JSON.stringify(Object.fromEntries(['authorSchema','componentSchemas','slots','limits','hash'].map(key => [key, manifest[key]]))) + '\n';
		if (await readFile('backend/resources/presentation-contract.json','utf8').catch(()=>null) !== contract) await writeFile('backend/resources/presentation-contract.json', contract);
		console.log(`Generated ${manifest.version} (${manifest.componentNames.length} components; schema ${manifest.hash.slice(0, 12)})`);
	} finally { await server.close(); }

	// Cache metadata stays local; the tracked manifest format and hashes above
	// remain unchanged. Avoid caching a generation whose inputs changed in flight.
	if (await inputHash() === inputs) {
		const cache = JSON.stringify({ version: 1, inputHash: inputs, outputs: await outputHashes() }, null, 2) + '\n';
		await mkdir('src/lib/presentation/generated', { recursive: true });
		if (await readFile(cachePath, 'utf8').catch(() => null) !== cache) await writeFile(cachePath, cache);
	}
}

await generate();

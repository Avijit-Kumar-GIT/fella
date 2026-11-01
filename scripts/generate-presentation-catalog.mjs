import { createServer } from 'vite';
import { readFile, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';

// Generate the engine-facing schema and prompt from the same catalog used by the Svelte renderer.
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

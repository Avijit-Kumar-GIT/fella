/** Read legacy memory notes. Kept in TS because Vite's Svelte dependency scan
 * strips HTML comment markers even when they appear inside script strings. */
export function parseLegacyMemory(markdown: string | null): { title: string; entries: string[] }[] {
	if (!markdown) return [];
	const sections: { title: string; entries: string[] }[] = [];
	let current: { title: string; entries: string[] } | null = null;
	let inComment = false;
	for (const raw of markdown.split(/\r?\n/)) {
		const line = raw.trim();
		if (line.includes('<!--')) inComment = true;
		if (!inComment && line.startsWith('## ')) {
			if (line.slice(3).trim().toLowerCase() === 'semantic facts') break;
			current = { title: line.slice(3).trim(), entries: [] };
			sections.push(current);
			continue;
		}
		if (inComment) {
			if (line.includes('-->')) inComment = false;
			continue;
		}
		if (!current || !line || line.startsWith('#')) continue;
		const entry = line.replace(/^-\s+/, '').trim();
		if (entry) current.entries.push(entry);
	}
	return sections.filter((section) => section.entries.length > 0);
}

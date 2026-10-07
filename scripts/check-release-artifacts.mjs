import { readdir, stat } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const artifactNames = {
	mac: (version) => [`Fella_${version}_universal.dmg`, `Fella_${version}_universal.zip`],
	win: (version) => [`Fella_${version}_x64.exe`, `Fella_${version}_x64.msi`],
	linux: (version) => [`Fella_${version}_x64.AppImage`, `Fella_${version}_x64.deb`]
};

export function expectedReleaseArtifacts(tag, platform = 'all') {
	const version = String(tag).trim().replace(/^v/i, '');
	if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(version)) {
		throw new Error(`invalid release tag: ${tag}`);
	}

	const platforms = platform === 'all' ? Object.keys(artifactNames) : [platform];
	const unknown = platforms.filter((name) => !Object.hasOwn(artifactNames, name));
	if (unknown.length) throw new Error(`unknown release platform: ${unknown.join(', ')}`);
	return platforms.flatMap((name) => artifactNames[name](version));
}

export async function checkReleaseArtifacts(tag, directory, platform = 'all') {
	const expected = expectedReleaseArtifacts(tag, platform);
	const present = new Set(await readdir(directory));
	const failures = [];
	for (const name of expected) {
		if (!present.has(name)) {
			failures.push(`missing ${name}`);
			continue;
		}
		const info = await stat(join(directory, name));
		if (!info.isFile() || info.size === 0) failures.push(`${name} is not a non-empty file`);
	}
	if (failures.length) throw new Error(`release artifacts are incomplete: ${failures.join('; ')}`);
	return expected;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
	const [, , tag, platform = 'all', directory = 'dist-electron'] = process.argv;
	if (!tag) {
		console.error('usage: node scripts/check-release-artifacts.mjs <tag> [mac|win|linux|all] [directory]');
		process.exitCode = 2;
	} else {
		try {
			const files = await checkReleaseArtifacts(tag, directory, platform);
			console.log(`release artifacts present (${platform}): ${files.join(', ')}`);
		} catch (error) {
			console.error(error instanceof Error ? error.message : String(error));
			process.exitCode = 1;
		}
	}
}

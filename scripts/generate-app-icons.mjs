import { chromium } from '@playwright/test';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = join(dirname(fileURLToPath(import.meta.url)), '..');
const sourcePath = join(repoRoot, 'static/brand/fella-orb.svg');
const outputDir = join(repoRoot, 'electron/icons');
const icoSizes = [16, 24, 32, 48, 64, 256];
const icnsSizes = [
	['icp4', 16],
	['icp5', 32],
	['icp6', 64],
	['ic07', 128],
	['ic08', 256],
	['ic09', 512],
	['ic10', 1024],
	['ic11', 32],
	['ic12', 64],
	['ic13', 256],
	['ic14', 512]
];

const svg = await readFile(sourcePath, 'utf8');
const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 1024, height: 1024 }, deviceScaleFactor: 1 });
const rendered = new Map();

async function pngAt(size) {
	if (rendered.has(size)) return rendered.get(size);
	await page.setViewportSize({ width: size, height: size });
	const sizedSvg = svg.replace('<svg ', `<svg width="${size}" height="${size}" `);
	await page.setContent(`<!doctype html><html><head><style>html,body{width:${size}px;height:${size}px;margin:0;overflow:hidden;background:transparent}svg{display:block}</style></head><body>${sizedSvg}</body></html>`);
	const png = await page.screenshot({ omitBackground: true, animations: 'disabled' });
	rendered.set(size, png);
	return png;
}

function makeIco(entries) {
	const header = Buffer.alloc(6 + entries.length * 16);
	header.writeUInt16LE(0, 0);
	header.writeUInt16LE(1, 2);
	header.writeUInt16LE(entries.length, 4);
	let offset = header.length;
	const images = [];
	for (const [index, [size, png]] of entries.entries()) {
		const entryOffset = 6 + index * 16;
		header[entryOffset] = size === 256 ? 0 : size;
		header[entryOffset + 1] = size === 256 ? 0 : size;
		header.writeUInt16LE(1, entryOffset + 4);
		header.writeUInt16LE(32, entryOffset + 6);
		header.writeUInt32LE(png.length, entryOffset + 8);
		header.writeUInt32LE(offset, entryOffset + 12);
		offset += png.length;
		images.push(png);
	}
	return Buffer.concat([header, ...images]);
}

function makeIcns(entries) {
	const chunks = entries.map(([type, png]) => {
		const header = Buffer.alloc(8);
		header.write(type, 0, 4, 'ascii');
		header.writeUInt32BE(png.length + header.length, 4);
		return Buffer.concat([header, png]);
	});
	const header = Buffer.alloc(8);
	header.write('icns', 0, 4, 'ascii');
	header.writeUInt32BE(chunks.reduce((total, chunk) => total + chunk.length, header.length), 4);
	return Buffer.concat([header, ...chunks]);
}

try {
	await mkdir(outputDir, { recursive: true });
	const iconPng = await pngAt(512);
	const icoEntries = await Promise.all(icoSizes.map(async (size) => [size, await pngAt(size)]));
	const icnsEntries = await Promise.all(icnsSizes.map(async ([type, size]) => [type, await pngAt(size)]));
	await Promise.all([
		writeFile(join(outputDir, 'icon.png'), iconPng),
		writeFile(join(outputDir, 'icon.ico'), makeIco(icoEntries)),
		writeFile(join(outputDir, 'icon.icns'), makeIcns(icnsEntries))
	]);
} finally {
	await browser.close();
}

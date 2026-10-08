import assert from 'node:assert/strict';
import { after, test } from 'node:test';
import { createServer } from 'vite';

const server = await createServer({
	configFile: 'vite.config.ts',
	appType: 'custom',
	logLevel: 'error',
	server: { middlewareMode: true, hmr: false, ws: false }
});
const { resolveWorkspaceTileLayout } = await server.ssrLoadModule('/src/lib/workspace-layout.ts');
after(() => server.close());

function assertTilesCoverSurface(tiles) {
	assert.equal(tiles.reduce((sum, tile) => sum + tile.width * tile.height, 0), 1);
	for (const tile of tiles) {
		assert.ok(tile.x >= 0 && tile.y >= 0);
		assert.ok(tile.x + tile.width <= 1 && tile.y + tile.height <= 1);
	}
	for (let left = 0; left < tiles.length; left += 1) {
		for (let right = left + 1; right < tiles.length; right += 1) {
			const a = tiles[left];
			const b = tiles[right];
			const overlapWidth = Math.min(a.x + a.width, b.x + b.width) - Math.max(a.x, b.x);
			const overlapHeight = Math.min(a.y + a.height, b.y + b.height) - Math.max(a.y, b.y);
			assert.ok(overlapWidth <= 0 || overlapHeight <= 0, `${a.id} overlaps ${b.id}`);
		}
	}
}

test('zero tiles returns an empty composition and one tile fills the surface', () => {
	assert.deepEqual(resolveWorkspaceTileLayout([]), []);
	const [tile] = resolveWorkspaceTileLayout(['repo-a']);
	assert.deepEqual(tile, { id: 'repo-a', x: 0, y: 0, width: 1, height: 1 });
});

test('two tiles split equally side-by-side by default or stacked on request', () => {
	assert.deepEqual(resolveWorkspaceTileLayout(['a', 'b']), [
		{ id: 'a', x: 0, y: 0, width: 0.5, height: 1 },
		{ id: 'b', x: 0.5, y: 0, width: 0.5, height: 1 }
	]);
	assert.deepEqual(resolveWorkspaceTileLayout(['a', 'b'], { two: 'stacked' }), [
		{ id: 'a', x: 0, y: 0, width: 1, height: 0.5 },
		{ id: 'b', x: 0, y: 0.5, width: 1, height: 0.5 }
	]);
});

test('all four three-tile arrangements use exact half splits and cover the surface', () => {
	const cases = [
		['two-top-one-bottom', [
			{ id: 'a', x: 0, y: 0, width: 0.5, height: 0.5 },
			{ id: 'b', x: 0.5, y: 0, width: 0.5, height: 0.5 },
			{ id: 'c', x: 0, y: 0.5, width: 1, height: 0.5 }
		]],
		['one-top-two-bottom', [
			{ id: 'a', x: 0, y: 0, width: 1, height: 0.5 },
			{ id: 'b', x: 0, y: 0.5, width: 0.5, height: 0.5 },
			{ id: 'c', x: 0.5, y: 0.5, width: 0.5, height: 0.5 }
		]],
		['two-left-one-right', [
			{ id: 'a', x: 0, y: 0, width: 0.5, height: 0.5 },
			{ id: 'b', x: 0, y: 0.5, width: 0.5, height: 0.5 },
			{ id: 'c', x: 0.5, y: 0, width: 0.5, height: 1 }
		]],
		['one-left-two-right', [
			{ id: 'a', x: 0, y: 0, width: 0.5, height: 1 },
			{ id: 'b', x: 0.5, y: 0, width: 0.5, height: 0.5 },
			{ id: 'c', x: 0.5, y: 0.5, width: 0.5, height: 0.5 }
		]]
	];
	for (const [preset, expected] of cases) {
		const actual = resolveWorkspaceTileLayout(['a', 'b', 'c'], { three: preset });
		assert.deepEqual(actual, expected, preset);
		assertTilesCoverSurface(actual);
	}
});

test('four tiles form a fixed row-major 2-by-2 grid', () => {
	const tiles = resolveWorkspaceTileLayout(['a', 'b', 'c', 'd']);
	assert.deepEqual(tiles, [
		{ id: 'a', x: 0, y: 0, width: 0.5, height: 0.5 },
		{ id: 'b', x: 0.5, y: 0, width: 0.5, height: 0.5 },
		{ id: 'c', x: 0, y: 0.5, width: 0.5, height: 0.5 },
		{ id: 'd', x: 0.5, y: 0.5, width: 0.5, height: 0.5 }
	]);
	assertTilesCoverSurface(tiles);
});

test('layout rejects more than four tiles, duplicate IDs, and empty IDs', () => {
	assert.throws(() => resolveWorkspaceTileLayout(['a', 'b', 'c', 'd', 'e']), RangeError);
	assert.throws(() => resolveWorkspaceTileLayout(['same', 'same']), TypeError);
	assert.throws(() => resolveWorkspaceTileLayout(['']), TypeError);
});

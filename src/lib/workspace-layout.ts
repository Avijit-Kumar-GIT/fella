/** Fixed, equal-split geometry shared by repository tiles and workspace panes.
 * Coordinates are normalized to the containing surface: x/y are the top-left
 * corner and width/height are fractions in [0, 1]. */
export interface WorkspaceTileRect {
	id: string;
	x: number;
	y: number;
	width: number;
	height: number;
}

export type TwoTileLayout = 'side-by-side' | 'stacked';
export type ThreeTileLayout =
	| 'two-top-one-bottom'
	| 'one-top-two-bottom'
	| 'two-left-one-right'
	| 'one-left-two-right';

export interface WorkspaceTilePreference {
	two?: TwoTileLayout;
	three?: ThreeTileLayout;
}

/**
 * Return a deterministic 1–4 tile composition. Each split is exactly 50/50;
 * three-tile layouts are recursive halves (one half plus two quarters).
 * Caller order defines placement order within each named preset.
 */
export function resolveWorkspaceTileLayout(
	ids: readonly string[],
	preference: WorkspaceTilePreference = {}
): WorkspaceTileRect[] {
	if (ids.length > 4) throw new RangeError('A workspace composition supports at most four tiles.');
	if (ids.some((id) => !id.trim())) throw new TypeError('Workspace tile IDs must not be empty.');
	if (new Set(ids).size !== ids.length) throw new TypeError('Workspace tile IDs must be unique.');

	const rects: Omit<WorkspaceTileRect, 'id'>[] = [];
	switch (ids.length) {
		case 0:
			return [];
		case 1:
			rects.push({ x: 0, y: 0, width: 1, height: 1 });
			break;
		case 2:
			if (preference.two === 'stacked') {
				rects.push(
					{ x: 0, y: 0, width: 1, height: 0.5 },
					{ x: 0, y: 0.5, width: 1, height: 0.5 }
				);
			} else {
				rects.push(
					{ x: 0, y: 0, width: 0.5, height: 1 },
					{ x: 0.5, y: 0, width: 0.5, height: 1 }
				);
			}
			break;
		case 3:
			switch (preference.three ?? 'two-top-one-bottom') {
				case 'one-top-two-bottom':
					rects.push(
						{ x: 0, y: 0, width: 1, height: 0.5 },
						{ x: 0, y: 0.5, width: 0.5, height: 0.5 },
						{ x: 0.5, y: 0.5, width: 0.5, height: 0.5 }
					);
					break;
				case 'two-left-one-right':
					rects.push(
						{ x: 0, y: 0, width: 0.5, height: 0.5 },
						{ x: 0, y: 0.5, width: 0.5, height: 0.5 },
						{ x: 0.5, y: 0, width: 0.5, height: 1 }
					);
					break;
				case 'one-left-two-right':
					rects.push(
						{ x: 0, y: 0, width: 0.5, height: 1 },
						{ x: 0.5, y: 0, width: 0.5, height: 0.5 },
						{ x: 0.5, y: 0.5, width: 0.5, height: 0.5 }
					);
					break;
				case 'two-top-one-bottom':
				default:
					rects.push(
						{ x: 0, y: 0, width: 0.5, height: 0.5 },
						{ x: 0.5, y: 0, width: 0.5, height: 0.5 },
						{ x: 0, y: 0.5, width: 1, height: 0.5 }
					);
					break;
			}
			break;
		default:
			for (let index = 0; index < 4; index += 1) {
				rects.push({
					x: (index % 2) * 0.5,
					y: Math.floor(index / 2) * 0.5,
					width: 0.5,
					height: 0.5
				});
			}
	}

	return ids.map((id, index) => ({ id, ...rects[index] }));
}

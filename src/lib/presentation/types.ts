/** The host supplies saved analytical results. Presentations never fetch their own data. */
export interface BoundResult {
	id: string;
	columns: string[];
	rows: unknown[][];
	exact_rows: string[][];
	row_count: number;
	complete: boolean;
}

export type PresentationState = Record<string, unknown>;
export interface PresentationAction {
	type: 'continue';
	message: string;
	/** Presentation-only selections, not instructions or execution authority. */
	state: PresentationState;
}
export interface PresentationIssue {
	path: string;
	message: string;
}
export interface PresentationExport {
	markdown: string;
	/** Losses and fallbacks are explicit, never silently omitted. */
	warnings: string[];
	resultIds: string[];
}

export const PRESENTATION_VERSION = 'fella-openui-v1';
export const PRESENTATION_LIMITS = {
	programBytes: 100_000,
	components: 160,
	depth: 24,
	results: 32,
	rowsPerResult: 10_000,
	cells: 200_000,
	resultCharacters: 8_000_000,
	chartNodes: 6_000,
	chartLayers: 24,
	facets: 60,
	text: 16_000
} as const;

import type { BoundResult, PresentationState } from './types';
import { PRESENTATION_LIMITS as limits } from './types';

export function validateResults(results: BoundResult[]): void {
	if (!Array.isArray(results) || results.length > limits.results) throw new Error('Too many bound results.');
	const ids = new Set<string>();
	let cells = 0;
	let characters = 0;
	for (const result of results) {
		if (!result || typeof result.id !== 'string' || !result.id || ids.has(result.id)) throw new Error('Result IDs must be present and unique.');
		ids.add(result.id);
		if (!Array.isArray(result.columns) || !result.columns.length || result.columns.length > 100 || result.columns.some((v) => typeof v !== 'string' || !v || v.length > 240) || new Set(result.columns).size !== result.columns.length) throw new Error(`Invalid columns in ${result.id}.`);
		if (!Array.isArray(result.rows) || result.rows.length > limits.rowsPerResult || !Number.isSafeInteger(result.row_count) || result.row_count < result.rows.length || typeof result.complete !== 'boolean' || (result.complete && result.row_count !== result.rows.length)) throw new Error(`Invalid result extent in ${result.id}.`);
		if (!Array.isArray(result.exact_rows) || result.exact_rows.length !== result.rows.length) throw new Error(`Exact values are missing for ${result.id}.`);
		for (let row = 0; row < result.rows.length; row++) {
			if (!Array.isArray(result.rows[row]) || result.rows[row].length !== result.columns.length || !Array.isArray(result.exact_rows[row]) || result.exact_rows[row].length !== result.columns.length) throw new Error(`Invalid row width in ${result.id}.`);
			for (let col = 0; col < result.columns.length; col++) {
				const value = result.rows[row][col];
				if (value !== null && typeof value !== 'string' && typeof value !== 'boolean' && (typeof value !== 'number' || !Number.isFinite(value))) throw new Error(`Non-scalar value in ${result.id}.`);
				if (typeof result.exact_rows[row][col] !== 'string' || result.exact_rows[row][col].length > limits.text) throw new Error(`Invalid exact value in ${result.id}.`);
				characters += result.exact_rows[row][col].length + (typeof value === 'string' ? value.length : 0);
				if (characters > limits.resultCharacters) throw new Error('Bound result text exceeds the presentation size budget.');
			}
		}
		cells += result.rows.length * result.columns.length;
		if (cells > limits.cells) throw new Error('Bound results exceed the presentation cell budget.');
	}
}

export function requireResult(results: BoundResult[], id: string): BoundResult {
	const result = results.find((item) => item.id === id);
	if (!result) throw new Error(`Saved result “${id}” is not available to this presentation.`);
	return result;
}

export function exactValue(result: BoundResult, row: number, field: string): string {
	const col = result.columns.indexOf(field);
	if (col < 0 || !Number.isInteger(row) || row < 0 || row >= result.rows.length) throw new Error('The saved value reference is invalid.');
	// Null is distinct from an absent string or a numeric zero, irrespective of host formatting.
	return result.rows[row][col] === null ? 'null' : result.exact_rows[row][col];
}

export interface FilterSelection { resultId: string; field: string; value: string | null }
export function filteredRows(result: BoundResult, state: PresentationState): number[] {
	const filters = Object.entries(state).filter(([key]) => key.startsWith('filter:')).map(([, value]) => value as FilterSelection);
	return result.rows.flatMap((_, row) => filters.every((filter) => !filter || filter.resultId !== result.id || filter.value === null || (result.columns.includes(filter.field) && exactValue(result, row, filter.field) === filter.value)) ? [row] : []);
}

export function sanitizeState(state: PresentationState = {}): PresentationState {
	const clean: PresentationState = Object.create(null);
	for (const [key, value] of Object.entries(state).slice(0, 160)) {
		if (key.startsWith('tab:') && typeof value === 'number' && Number.isInteger(value) && value >= 0 && value < 12) clean[key] = value;
		if (key.startsWith('control:') && /^[A-Za-z][A-Za-z0-9_.-]{0,79}$/.test(key.slice(8)) && (value === null || typeof value === 'string' && value.length <= 256 || typeof value === 'boolean' || typeof value === 'number' && Number.isFinite(value) && Math.abs(value) <= 1e12)) clean[key] = value;
		if (key.startsWith('filter:') && value && typeof value === 'object') {
			const filter = value as FilterSelection;
			if (typeof filter.resultId === 'string' && filter.resultId.length <= 240 && typeof filter.field === 'string' && filter.field.length <= 240 && (typeof filter.value === 'string' && filter.value.length <= limits.text || filter.value === null)) clean[key] = { resultId: filter.resultId, field: filter.field, value: filter.value };
		}
	}
	return clean;
}

/** Rehydrate only controls declared in this spec; stale state never leaks across revisions. */
export function resolvePresentationState(spec: { root: string; elements: Record<string, { type: string; props: Record<string, unknown> }> }, state: PresentationState, results: BoundResult[]): PresentationState {
	const clean = sanitizeState(state);
	const resolved: PresentationState = Object.create(null);
	for (const element of Object.values(spec.elements)) {
		const props = element.props;
		if (element.type === 'Tabs') {
			const key = `tab:${String(props.name)}`;
			const value = clean[key];
			resolved[key] = typeof value === 'number' && value < (props.labels as string[]).length ? value : 0;
		}
		if (['Select', 'SegmentedControl'].includes(element.type)) {
			const stateKey = String(props.stateKey);
			const key = `control:${stateKey}`;
			const options = props.options as Array<{ value: string }>;
			const initial = String(props.defaultValue ?? options[0]?.value ?? '');
			const candidate = clean[key];
			resolved[key] = typeof candidate === 'string' && options.some(option => option.value === candidate) ? candidate : initial;
		}
		if (element.type === 'Checkbox') {
			const key = `control:${String(props.stateKey)}`;
			resolved[key] = typeof clean[key] === 'boolean' ? clean[key] : Boolean(props.defaultChecked ?? false);
		}
		if (element.type === 'ResultFilter') {
			const key = `filter:${String(props.name)}`;
			const value = clean[key] as FilterSelection | undefined;
			if (value && value.resultId === props.resultId && value.field === props.field) {
				const result = requireResult(results, value.resultId);
				if (value.value === null || result.rows.some((_, row) => exactValue(result, row, value.field) === value.value)) resolved[key] = value;
			} else resolved[key] = { resultId: String(props.resultId), field: String(props.field), value: null };
		}
	}
	return resolved;
}

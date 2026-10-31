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

export interface FilterSelection {
	resultId: string;
	field: string;
	value: string | null;
	interactionGroup?: string;
	interactionScope?: string;
	valueType?: 'string' | 'number' | 'boolean' | 'null';
}
export interface RowFilterValue { field: string; value: string; valueType: 'string' | 'number' | 'boolean' | 'null' }

export function resultFilterStateKey(name: string, interactionGroup?: string): string {
	return interactionGroup ? `filter:group:${encodeURIComponent(interactionGroup)}:${encodeURIComponent(name)}` : `filter:${name}`;
}

function typeOfResultValue(value: unknown): RowFilterValue['valueType'] | undefined {
	if (value === null) return 'null';
	if (typeof value === 'string') return 'string';
	if (typeof value === 'number') return 'number';
	if (typeof value === 'boolean') return 'boolean';
	return undefined;
}

export function exactRowFilterValuesFromRow(result: BoundResult, row: number, fields: string[]): RowFilterValue[] {
	if (!Number.isInteger(row) || row < 0 || row >= result.rows.length) return [];
	return fields.flatMap(field => {
		const column = result.columns.indexOf(field);
		if (column < 0) return [];
		const valueType = typeOfResultValue(result.rows[row][column]);
		return valueType ? [{ field, value: exactValue(result, row, field), valueType }] : [];
	});
}

export function filteredRows(result: BoundResult, state: PresentationState, interactionGroup?: string, linkedFields: string[] = [], interactionScope?: string): number[] {
	const filters = Object.entries(state).filter(([key]) => key.startsWith('filter:')).map(([key, value]) => ({ key, filter: value as FilterSelection }));
	return result.rows.flatMap((_, row) => filters.every(({ key, filter }) => {
		if (!filter || filter.value === null) return true;
		const selection = key.startsWith('filter:selection:');
		const sameResult = !selection && filter.resultId === result.id;
		const sameScope = filter.interactionScope === interactionScope;
		const explicitlyLinked = Boolean(filter.interactionGroup && interactionGroup === filter.interactionGroup && linkedFields.includes(filter.field) && sameScope);
		if (!sameResult && !explicitlyLinked) return true;
		if (!result.columns.includes(filter.field)) return true;
		const column = result.columns.indexOf(filter.field);
		const raw = result.rows[row][column];
		if (filter.valueType && typeOfResultValue(raw) !== filter.valueType) return false;
		return exactValue(result, row, filter.field) === filter.value;
	}) ? [row] : []);
}

/** Active group filters that cannot affect this component are surfaced in the
 * UI instead of looking like a successful cross-filter with unchanged data. */
export function incompatibleInteractionFilters(
	result: BoundResult,
	state: PresentationState,
	interactionGroup: string | undefined,
	linkedFields: string[],
	interactionScope: string | undefined
): FilterSelection[] {
	if (!interactionGroup || !interactionScope) return [];
	const incompatible = new Map<string, FilterSelection>();
	for (const [key, raw] of Object.entries(state)) {
		if (!key.startsWith('filter:') || !raw || typeof raw !== 'object') continue;
		const filter = raw as FilterSelection;
		if (filter.value === null || filter.interactionGroup !== interactionGroup || filter.interactionScope !== interactionScope) continue;
		const locallyBound = filter.resultId === result.id && result.columns.includes(filter.field);
		const explicitlyLinked = linkedFields.includes(filter.field) && result.columns.includes(filter.field);
		if (!locallyBound && !explicitlyLinked) incompatible.set(filter.field, filter);
	}
	return [...incompatible.values()];
}

/** Convert a clicked Vega datum back to exact values already present in a saved result. */
export function exactRowFilterValues(result: BoundResult, datum: unknown, fields: string[]): RowFilterValue[] {
	if (!datum || typeof datum !== 'object' || Array.isArray(datum)) return [];
	const values = datum as Record<string, unknown>;
	const encodings: Array<{ field: string; column: number; raw: unknown }> = [];
	for (const field of fields) {
		const column = result.columns.indexOf(field);
		const raw = values[`fella_col_${column}`];
		if (column < 0 || raw === undefined) continue;
		encodings.push({ field, column, raw });
	}
	const matches = (candidate: unknown, raw: unknown): boolean => {
		if (candidate === raw) return true;
		if (raw instanceof Date && typeof candidate === 'string') return Date.parse(candidate) === raw.getTime();
		if (typeof candidate === 'number' && typeof raw === 'string') return Number.isFinite(Number(raw)) && Number(raw) === candidate;
		return candidate !== null && candidate !== undefined && String(candidate) === String(raw);
	};
	const row = result.rows.findIndex(candidate => encodings.every(({ column, raw }) => matches(candidate[column], raw)));
	if (row < 0) return [];
	return exactRowFilterValuesFromRow(result, row, encodings.map(({ field }) => field));
}

export function sanitizeState(state: PresentationState = {}): PresentationState {
	const clean: PresentationState = Object.create(null);
	for (const [key, value] of Object.entries(state).slice(0, 160)) {
		if (key.startsWith('tab:') && typeof value === 'number' && Number.isInteger(value) && value >= 0 && value < 12) clean[key] = value;
		if (key.startsWith('control:') && /^[A-Za-z][A-Za-z0-9_.-]{0,79}$/.test(key.slice(8)) && (value === null || typeof value === 'string' && value.length <= 256 || typeof value === 'boolean' || typeof value === 'number' && Number.isFinite(value) && Math.abs(value) <= 1e12)) clean[key] = value;
		if (key.startsWith('filter:') && value && typeof value === 'object') {
			const filter = value as FilterSelection;
			if (typeof filter.resultId === 'string' && filter.resultId.length <= 240 && typeof filter.field === 'string' && filter.field.length <= 240 && (filter.interactionGroup === undefined || typeof filter.interactionGroup === 'string' && /^[A-Za-z][A-Za-z0-9_.-]{0,79}$/.test(filter.interactionGroup)) && (filter.interactionScope === undefined || typeof filter.interactionScope === 'string' && filter.interactionScope.length <= 240) && (filter.valueType === undefined || ['string', 'number', 'boolean', 'null'].includes(filter.valueType)) && (typeof filter.value === 'string' && filter.value.length <= limits.text || filter.value === null)) clean[key] = { resultId: filter.resultId, field: filter.field, value: filter.value, ...(filter.interactionGroup ? { interactionGroup: filter.interactionGroup } : {}), ...(filter.interactionScope ? { interactionScope: filter.interactionScope } : {}), ...(filter.valueType ? { valueType: filter.valueType } : {}) };
		}
	}
	return clean;
}

/** Rehydrate only controls declared in this spec; stale state never leaks across revisions. */
export function resolvePresentationState(spec: { root: string; elements: Record<string, { type: string; props: Record<string, unknown> }> }, state: PresentationState, results: BoundResult[], interactionScope?: string): PresentationState {
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
			const key = resultFilterStateKey(String(props.name), typeof props.interactionGroup === 'string' ? props.interactionGroup : undefined);
			const value = clean[key] as FilterSelection | undefined;
			if (value && value.resultId === props.resultId && value.field === props.field && value.interactionGroup === props.interactionGroup && value.interactionScope === undefined) {
				const result = requireResult(results, value.resultId);
				if (value.value === null || result.rows.some((_, row) => exactValue(result, row, value.field) === value.value)) resolved[key] = value;
			} else if (value && value.interactionGroup === props.interactionGroup && value.field === props.field && value.interactionScope === interactionScope && interactionScope) {
				resolved[key] = value;
			} else resolved[key] = { resultId: String(props.resultId), field: String(props.field), value: null, ...(typeof props.interactionGroup === 'string' ? { interactionGroup: props.interactionGroup } : {}) };
		}
	}
	return resolved;
}

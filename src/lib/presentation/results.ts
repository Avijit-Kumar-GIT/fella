import type { BoundResult, PresentationState } from './types';
import { PRESENTATION_LIMITS as limits } from './types';
import type { ElementNode } from '@openuidev/lang-core';

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

/** Convert result-backed local shaping into a display-only table/chart source. */
export function derivedRowsResult(base: BoundResult, rows: unknown[]): BoundResult {
	if (!Array.isArray(rows) || rows.length > limits.rowsPerResult) throw new Error('A derived display exceeds the saved-row limit.');
	const records = rows.map((row, index) => {
		if (!row || typeof row !== 'object' || Array.isArray(row)) throw new Error(`Derived row ${index + 1} is not a row object.`);
		const record = row as Record<string, unknown>;
		if (Object.keys(record).some((key) => ['__proto__', 'constructor', 'prototype'].includes(key))) throw new Error('Derived rows contain an unsafe field.');
		return record;
	});
	const columns = [...new Set(records.flatMap((row) => Object.keys(row)))].slice(0, 100);
	if (!columns.length) columns.push(...base.columns);
	const values = records.map((row) => columns.map((column) => {
		const value = row[column];
		if (value === null || typeof value === 'string' || typeof value === 'boolean' || typeof value === 'number' && Number.isFinite(value)) return value;
		throw new Error('Derived rows may contain only scalar display values.');
	}));
	if (values.length * columns.length > limits.cells) throw new Error('A derived display exceeds the presentation cell limit.');
	return {
		id: base.id,
		columns,
		rows: values,
		exact_rows: values.map((row) => row.map((value) => value === null ? 'null' : String(value))),
		row_count: base.complete ? values.length : Math.max(base.row_count, values.length),
		complete: base.complete
	};
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
		if (/^\$[A-Za-z_][A-Za-z0-9_]{0,63}$/.test(key) && (value === null || typeof value === 'string' && value.length <= 256 || typeof value === 'boolean' || typeof value === 'number' && Number.isFinite(value) && Math.abs(value) <= 1e12)) clean[key] = value;
		if (key.startsWith('filter:') && value && typeof value === 'object') {
			const filter = value as FilterSelection;
			if (typeof filter.resultId === 'string' && filter.resultId.length <= 240 && typeof filter.field === 'string' && filter.field.length <= 240 && (typeof filter.value === 'string' && filter.value.length <= limits.text || filter.value === null)) clean[key] = { resultId: filter.resultId, field: filter.field, value: filter.value };
		}
	}
	return clean;
}

/** Hydrated state is presentation-specific. Stale/hidden filters cannot affect another program. */
export function resolvePresentationState(root: ElementNode, state: PresentationState, results: BoundResult[], stateDefaults: Record<string, unknown> = {}): PresentationState {
	const clean = sanitizeState(state);
	const resolved: PresentationState = Object.create(null);
	for (const [name, defaultValue] of Object.entries(stateDefaults)) {
		const candidate = clean[name];
		resolved[name] = candidate !== undefined && (typeof candidate === typeof defaultValue || candidate === null && defaultValue === null) ? candidate : defaultValue;
	}
	function visit(node: ElementNode) {
		if (node.typeName === 'Tabs') {
			const key = `tab:${String(node.props.name)}`;
			const value = clean[key];
			if (typeof value === 'number' && value < (node.props.labels as string[]).length) resolved[key] = value;
		} else if (node.typeName === 'ResultFilter') {
			const key = `filter:${String(node.props.name)}`;
			const value = clean[key] as FilterSelection | undefined;
			if (value && value.resultId === node.props.resultId && value.field === node.props.field) {
				const result = requireResult(results, value.resultId);
				if (value.value === null || result.rows.some((_, row) => exactValue(result, row, value.field) === value.value)) resolved[key] = value;
			}
		}
		if (Array.isArray(node.props.children)) (node.props.children as ElementNode[]).forEach(visit);
	}
	visit(root);
	return resolved;
}

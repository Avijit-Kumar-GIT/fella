import { BUILTINS, BUILTIN_NAMES } from '@openuidev/lang-core';

/**
 * Pure, bounded display functions for Fella's OpenUI runtime.
 *
 * These functions only receive values produced from persisted analysis results
 * or declared UI state. They never issue queries, mutate workspace data, or
 * call the model. This deliberately covers analytical shaping rather than the
 * unrelated game/media helpers in Open Intelligent UI.
 */
export type PresentationFunction = (...args: unknown[]) => unknown;

type Row = Record<string, unknown>;
const MAX_ITEMS = 10_000;
const MAX_GROUPS = 500;
const unsafeKeys = new Set(['__proto__', 'prototype', 'constructor']);

function list(value: unknown): unknown[] {
	return Array.isArray(value) ? value.slice(0, MAX_ITEMS) : [];
}
function field(row: unknown, key: unknown): unknown {
	if (unsafeKeys.has(String(key)) || !row || typeof row !== 'object') return null;
	return Object.hasOwn(row, String(key)) ? (row as Row)[String(key)] ?? null : null;
}
function numeric(value: unknown): number | null {
	if (typeof value === 'number' && Number.isFinite(value)) return value;
	if (typeof value === 'string' && value.trim()) {
		const parsed = Number(value);
		return Number.isFinite(parsed) ? parsed : null;
	}
	return null;
}
function values(input: unknown, key?: unknown): number[] {
	return list(input).flatMap((item) => {
		const value = key === undefined ? item : field(item, key);
		const parsed = numeric(value);
		return parsed === null ? [] : [parsed];
	});
}
function aggregate(rows: unknown[], key: unknown, operation: unknown): number {
	const op = String(operation ?? (key === undefined ? 'count' : 'sum')).toLowerCase();
	if (op === 'count') return rows.length;
	const nums = values(rows, key);
	if (!nums.length) return 0;
	switch (op) {
		case 'avg': case 'average': case 'mean': return nums.reduce((a, b) => a + b, 0) / nums.length;
		case 'min': return Math.min(...nums);
		case 'max': return Math.max(...nums);
		case 'sum': default: return nums.reduce((a, b) => a + b, 0);
	}
}
function grouped(rowsInput: unknown, key: unknown): Map<string | number | boolean, unknown[]> {
	const result = new Map<string | number | boolean, unknown[]>();
	for (const row of list(rowsInput)) {
		const value = field(row, key);
		if (value === null || !['string', 'number', 'boolean'].includes(typeof value)) continue;
		let group = result.get(value as string | number | boolean);
		if (!group) { if (result.size >= MAX_GROUPS) break; result.set(value as string | number | boolean, group = []); }
		group.push(row);
	}
	return result;
}
function compare(a: unknown, b: unknown): number {
	const x = numeric(a), y = numeric(b);
	if (x !== null && y !== null) return x - y;
	const left = String(a ?? ''), right = String(b ?? '');
	return left < right ? -1 : left > right ? 1 : 0;
}
function round(value: unknown, decimals: unknown = 0): number {
	const number = numeric(value) ?? 0;
	const places = Math.max(0, Math.min(8, Math.trunc(numeric(decimals) ?? 0)));
	const factor = 10 ** places;
	return Math.round((number + Number.EPSILON) * factor) / factor;
}

export const PRESENTATION_FUNCTIONS: Record<string, PresentationFunction> = {
	GroupBy: (rows, key, measure, operation) => [...grouped(rows, key)].map(([group, members]) => ({ key: group, value: aggregate(members, measure, operation) })),
	Pivot: (rows, rowKey, columnKey, measure, operation) => {
		const columns = [...grouped(rows, columnKey).keys()].map(String);
		return [...grouped(rows, rowKey)].map(([key, members]) => {
			const output: Row = { key };
			for (const column of columns) output[column] = 0;
			for (const [column, items] of grouped(members, columnKey)) output[String(column)] = aggregate(items, measure, operation);
			return output;
		});
	},
	Top: (rows, key, count, direction) => {
		const sign = direction === 'asc' ? 1 : -1;
		return list(rows).map((row, index) => ({ row, index, value: numeric(field(row, key)) }))
			.sort((a, b) => a.value === null || b.value === null ? (a.value === b.value ? a.index - b.index : a.value === null ? 1 : -1) : (a.value - b.value) * sign || a.index - b.index)
			.slice(0, Math.max(0, Math.min(MAX_ITEMS, Math.trunc(numeric(count) ?? 0)))).map((entry) => entry.row);
	},
	Between: (rows, key, min, max) => list(rows).filter((row) => {
		const value = field(row, key);
		return value !== null && (min == null || compare(value, min) >= 0) && (max == null || compare(value, max) <= 0);
	}),
	Percent: (part, total) => {
		const numerator = numeric(part) ?? 0, denominator = numeric(total) ?? 0;
		return denominator === 0 ? 0 : (numerator / denominator) * 100;
	},
	Clamp: (value, min, max) => Math.min(Math.max(numeric(value) ?? 0, Math.min(numeric(min) ?? 0, numeric(max) ?? 0)), Math.max(numeric(min) ?? 0, numeric(max) ?? 0)),
	Format: (value, style, decimals) => {
		const number = numeric(value);
		if (number === null) return String(value ?? '');
		const places = Math.max(0, Math.min(8, Math.trunc(numeric(decimals) ?? (style === 'currency' ? 2 : 1))));
		const currency = String(style ?? '').toUpperCase();
		if (/^[A-Z]{3}$/.test(currency)) return new Intl.NumberFormat(undefined, { style: 'currency', currency, minimumFractionDigits: places, maximumFractionDigits: places }).format(number);
		if (style === 'percent') return `${new Intl.NumberFormat(undefined, { maximumFractionDigits: places }).format(number)}%`;
		if (style === 'compact') return new Intl.NumberFormat(undefined, { notation: 'compact', maximumFractionDigits: places }).format(number);
		return new Intl.NumberFormat(undefined, { maximumFractionDigits: places }).format(number);
	},
	Join: (input, separator = ', ') => list(input).map((item) => item == null ? '' : String(item)).join(String(separator)).slice(0, 16_000),
	Default: (value, fallback) => value === null || value === undefined || value === '' ? fallback ?? null : value,
	Plural: (count, singular, plural) => numeric(count) === 1 ? String(singular ?? '') : String(plural ?? `${String(singular ?? '')}s`),
	Unique: (input) => [...new Set(list(input).filter((item) => ['string', 'number', 'boolean'].includes(typeof item)))].slice(0, MAX_ITEMS),
	Range: (start, end, step = 1) => {
		const from = Math.trunc(numeric(start) ?? 0), to = Math.trunc(numeric(end) ?? 0), by = Math.trunc(numeric(step) ?? 1);
		if (!by || Math.sign(to - from) !== Math.sign(by) && from !== to) return [];
		const count = Math.min(MAX_ITEMS, Math.max(0, Math.ceil((to - from) / by)));
		return Array.from({ length: count }, (_, index) => from + index * by);
	},
	Change: (rows, key) => {
		const input = list(rows);
		return input.map((row, index) => {
			const current = numeric(field(row, key));
			const previous = index > 0 ? numeric(field(input[index - 1], key)) : null;
			return { ...(row && typeof row === 'object' ? row as Row : { value: row }), change: current === null || previous === null ? null : current - previous, changePercent: current === null || previous === null || previous === 0 ? null : ((current - previous) / Math.abs(previous)) * 100 };
		});
	},
	Cumulative: (rows, key) => {
		let running = 0;
		return list(rows).map((row) => {
			const current = numeric(field(row, key));
			if (current !== null) running += current;
			return { ...(row && typeof row === 'object' ? row as Row : { value: row }), cumulative: running };
		});
	},
	MovingAvg: (rows, key, windowSize = 3) => {
		const size = Math.max(1, Math.min(200, Math.trunc(numeric(windowSize) ?? 3)));
		const input = list(rows);
		return input.map((row, index) => {
			const sample = values(input.slice(Math.max(0, index - size + 1), index + 1), key);
			return { ...(row && typeof row === 'object' ? row as Row : { value: row }), movingAverage: sample.length ? sample.reduce((a, b) => a + b, 0) / sample.length : null };
		});
	},
	Histogram: (input, binCount = 10) => {
		const data = values(input);
		if (!data.length) return [];
		const bins = Math.max(1, Math.min(100, Math.trunc(numeric(binCount) ?? 10)));
		const min = Math.min(...data), max = Math.max(...data), width = (max - min) / bins || 1;
		const counts = Array.from({ length: bins }, () => 0);
		for (const value of data) counts[Math.min(bins - 1, Math.floor((value - min) / width))]++;
		return counts.map((count, index) => ({ key: `${round(min + index * width, 2)}–${round(index === bins - 1 ? max : min + (index + 1) * width, 2)}`, value: count }));
	},
	Stats: (input) => {
		const data = values(input).sort((a, b) => a - b);
		if (!data.length) return { count: 0, sum: 0, average: null, min: null, max: null, median: null };
		const middle = Math.floor(data.length / 2);
		return { count: data.length, sum: data.reduce((a, b) => a + b, 0), average: data.reduce((a, b) => a + b, 0) / data.length, min: data[0], max: data[data.length - 1], median: data.length % 2 ? data[middle] : (data[middle - 1] + data[middle]) / 2 };
	},
};

export const PRESENTATION_FUNCTION_DOCS: Record<string, string> = {
	Rows: 'Return row objects from one supplied saved result by ID. This is the only row source for local display functions.',
	GroupBy: 'Group saved rows by a dimension and aggregate a numeric field (sum by default), returning key/value rows.',
	Pivot: 'Pivot saved rows into key rows with one value column per category.',
	Top: 'Return the n rows with the highest (or lowest) numeric value in a field.',
	Between: 'Keep rows with a field in an inclusive range; works for numeric and sortable date text values.',
	Percent: 'Compute part divided by total times 100 for a display expression.',
	Clamp: 'Keep a numeric display value between two bounds.',
	Format: 'Format a number as a currency code, percent, compact value, or number.',
	Join: 'Join a list of display values into a short string.',
	Default: 'Use a fallback when a display value is null or empty.',
	Plural: 'Choose singular or plural text based on a count.',
	Unique: 'Return distinct scalar values in first-seen order.',
	Range: 'Generate a bounded sequence of integers for display labels.',
	Change: 'Add per-row absolute and percentage change compared with the previous row.',
	Cumulative: 'Add a running total column to ordered saved rows.',
	MovingAvg: 'Add a moving-average column over an ordered numeric field.',
	Histogram: 'Bin a numeric list into count rows for a distribution display.',
	Stats: 'Summarize a numeric list for display with count, sum, average, minimum, maximum and median.'
};

export const SAFE_PRESENTATION_FUNCTIONS = new Set(Object.keys(PRESENTATION_FUNCTIONS));

/**
 * OpenUI Lang 0.4 exposes its builtin registry as the parser's extension point,
 * but does not yet expose library-scoped custom functions. Register only pure,
 * bounded functions for this local presentation runtime. Rows is deliberately
 * a trap here: runtime.ts must bind it to a supplied result before evaluation.
 */
let functionsInstalled = false;
export function installPresentationFunctions(): void {
	if (functionsInstalled) return;
	functionsInstalled = true;
	const custom: Record<string, PresentationFunction> = {
		...PRESENTATION_FUNCTIONS,
		Rows: () => { throw new Error('Rows() must be bound to a supplied saved result.'); }
	};
	for (const [name, fn] of Object.entries(custom)) {
		if (BUILTINS[name]) continue;
		BUILTINS[name] = {
			name,
			signature: `${name}(...)`,
			description: PRESENTATION_FUNCTION_DOCS[name] ?? 'A pure local display function over supplied values.',
			fn
		};
		BUILTIN_NAMES.add(name);
	}
}

installPresentationFunctions();

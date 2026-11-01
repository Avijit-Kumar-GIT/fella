import type { BoundResult } from './types';
import { acceptContract } from './contract';
type Obj = Record<string, unknown>;
const object = (value: unknown): value is Obj => !!value && typeof value === 'object' && !Array.isArray(value);

/** Validate through the same acceptance contract used by the native engine. */
export function validateChartSpec(spec: unknown, result: BoundResult): void {
 const input={root:'doc',elements:{doc:{type:'Document',props:{},children:['chart']},chart:{type:'Chart',props:{resultId:result.id,spec},children:[]}}};
 const checked=acceptContract(input,[result]);
 if(!checked.valid)throw new Error(checked.issues.map(issue=>issue.message).join('; '));
}

/** Escape field paths by mapping saved columns to internal identifiers before compilation. */
export function bindChartSpec(spec: Record<string, unknown>, result: BoundResult, rows = result.rows.map((_, index) => index)): Record<string, unknown> {
	validateChartSpec(spec, result);
	function rewrite(value: unknown, key = ''): unknown {
		if (key === 'field' && typeof value === 'string') return `fella_col_${result.columns.indexOf(value)}`;
		if (Array.isArray(value)) return value.map((item) => rewrite(item, key));
		if (!object(value)) return value;
		const copy: Obj = {};
		for (const [k, v] of Object.entries(value)) {
			if (k === '$schema') continue;
			if (k === 'repeat' && (Array.isArray(v) || (object(v) && !('repeat' in v)))) {
				const mapFields = (items: unknown) => (items as string[]).map((f) => `fella_col_${result.columns.indexOf(f)}`);
				copy[k] = Array.isArray(v) ? mapFields(v) : Object.fromEntries(Object.entries(v as Obj).map(([group, items]) => [group, mapFields(items)]));
			} else copy[k] = rewrite(v, k);
		}
		if (typeof value.field === 'string' && !('title' in value)) copy.title = value.field;
		return copy;
	}
	const bound = rewrite(spec) as Obj;
	bound.data = { values: rows.map((row) => Object.fromEntries(result.columns.map((_, col) => [`fella_col_${col}`, result.rows[row][col]]))) };
	return bound;
}

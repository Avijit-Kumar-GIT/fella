import type { BoundResult } from './types';
import { PRESENTATION_LIMITS as limits } from './types';

type Obj = Record<string, unknown>;
const object = (value: unknown): value is Obj => !!value && typeof value === 'object' && !Array.isArray(value);
const marks = new Set(['arc', 'area', 'bar', 'boxplot', 'circle', 'errorband', 'errorbar', 'line', 'point', 'rect', 'rule', 'square', 'text', 'tick', 'trail']);
const channels = new Set('x y x2 y2 xOffset yOffset theta theta2 radius radius2 color fill stroke opacity fillOpacity strokeOpacity strokeWidth size shape angle text tooltip detail order row column facet longitude latitude longitude2 latitude2'.split(' '));
const styleChannels = new Set('color fill stroke opacity fillOpacity strokeOpacity strokeWidth size shape angle'.split(' '));
const forbidden = /^(?:__proto__|prototype|constructor|data|datasets|url|href|image|images|signals?|expr|expression|calculate|filter|transform|params?|bind|on|usermeta|formatType|labelExpr|encode|handler|event|selection|lookup|config)$/i;
const markKeys = new Set('type color fill stroke opacity fillOpacity strokeOpacity strokeWidth strokeDash size shape angle orient interpolate tension point line clip filled cornerRadius cornerRadiusTopLeft cornerRadiusTopRight cornerRadiusBottomLeft cornerRadiusBottomRight thickness binSpacing continuousBandSize discreteBandSize strokeCap strokeJoin font fontSize fontStyle fontWeight align baseline dx dy limit ellipsis'.split(' '));
const encodingKeys = new Set('field type title aggregate bin timeUnit sort scale axis legend stack impute bandPosition spacing header value'.split(' '));
const auxiliaryKeys = new Set(('type title domain domainMin domainMax domainMid zero nice reverse clamp padding paddingInner paddingOuter range scheme base exponent constant round ' +
	'orient format tickCount values labelAngle labelLimit labelPadding labelOverlap labelColor labelFontSize titleColor titleFontSize grid gridColor domainColor tickColor tickSize offset ' +
	'columns direction gradientLength symbolType symbolSize symbolStrokeWidth symbolOpacity labelAlign titleAnchor titlePadding labelFontWeight titleFontWeight ' +
	'maxbins step extent anchor divide steps binned ' +
	'field op order header labels labelOrient titleOrient labelExpr ' +
	'method value groupby key frame start stop').split(' '));

/** Deliberately declarative Vega-Lite: no model-supplied expressions, data loaders or code.
 * This is a safety boundary, not the legacy chart-kind schema. Composition/encoding remain open.
 * Compiler-produced Vega expressions are evaluated by vega-interpreter, never Function(). */
export function validateChartSpec(spec: unknown, result: BoundResult): void {
	if (!object(spec)) throw new Error('Chart specification must be a Vega-Lite object.');
	let nodes = 0;
	let views = 0;
	let repeatedPanels = 1;
	let pixels = 0;
	const fields = new Set(result.columns);
	function bounded(value: unknown, depth = 0): void {
		if (++nodes > limits.chartNodes || depth > limits.depth) throw new Error('Chart exceeds the complexity budget.');
		if (typeof value === 'number' && (!Number.isFinite(value) || Math.abs(value) > 1e15)) throw new Error('Chart numeric setting is outside the supported range.');
		if (typeof value === 'string' && (value.length > 4000 || /(?:url\s*\(|(?:https?|file|javascript|data|vbscript):|<\/?(?:script|svg|iframe)\b)/i.test(value))) throw new Error('Chart contains an external or executable resource.');
		if (Array.isArray(value)) {
			if (value.length > 256) throw new Error('Chart array exceeds its budget.');
			value.forEach((item) => bounded(item, depth + 1));
		} else if (object(value)) {
			for (const [key, item] of Object.entries(value)) {
				if (forbidden.test(key)) throw new Error(`Chart property “${key}” is not permitted.`);
				bounded(item, depth + 1);
			}
		} else if (value !== null && !['number', 'boolean', 'string'].includes(typeof value)) throw new Error('Chart settings must be JSON values.');
	}
	bounded(spec);
	function known(value: Obj, allowed: Set<string>, context: string): void {
		for (const key of Object.keys(value)) if (!allowed.has(key)) throw new Error(`Unsupported ${context} setting “${key}”.`);
	}
	function field(value: unknown): void {
		if (typeof value !== 'string' || !fields.has(value)) throw new Error(`Chart field “${String(value)}” is not present in saved result ${result.id}.`);
	}
	function auxiliary(value: unknown, depth = 0): void {
		if (depth > 12) throw new Error('Chart style nesting is too deep.');
		if (Array.isArray(value)) { value.forEach((v) => auxiliary(v, depth + 1)); return; }
		if (!object(value)) return;
		known(value, auxiliaryKeys, 'style');
		for (const [key, item] of Object.entries(value)) {
			if (key === 'field') field(item);
			auxiliary(item, depth + 1);
		}
	}
	function encoding(value: unknown): void {
		if (!object(value)) throw new Error('Chart encoding must be an object.');
		known(value, channels, 'channel');
		for (const [channel, settings] of Object.entries(value)) {
			if (settings === null || settings === false) continue;
			for (const definition of Array.isArray(settings) ? settings : [settings]) {
				if (!object(definition)) throw new Error(`Chart channel ${channel} needs a field definition.`);
				known(definition, encodingKeys, 'encoding');
				if ('value' in definition && !styleChannels.has(channel)) throw new Error('Analytical chart values must reference saved fields, not model-authored constants.');
				if ('field' in definition) {
					if (object(definition.field) && ['row', 'column', 'repeat', 'layer'].includes(String(definition.field.repeat)) && Object.keys(definition.field).length === 1) { /* resolves from validated repeat fields */ }
					else field(definition.field);
				} else if (definition.aggregate !== 'count' && !('value' in definition)) throw new Error(`Chart channel ${channel} has no bound field.`);
				if (definition.type !== undefined && !['quantitative', 'temporal', 'ordinal', 'nominal', 'geojson'].includes(String(definition.type))) throw new Error('Unsupported field type.');
				if (definition.aggregate !== undefined && !['count', 'sum', 'mean', 'average', 'median', 'min', 'max', 'distinct', 'valid', 'missing', 'q1', 'q3', 'stdev', 'stdevp', 'variance', 'variancep'].includes(String(definition.aggregate))) throw new Error('Unsupported declarative aggregation.');
				if ('impute' in definition) throw new Error('Missing-value imputation requires a recorded analytical computation.');
				for (const key of ['scale', 'axis', 'legend', 'bin', 'sort', 'header']) if (definition[key] !== undefined) auxiliary(definition[key]);
			}
		}
	}
	const viewKeys = new Set('mark encoding layer facet spec repeat concat hconcat vconcat resolve title description width height spacing columns bounds center align autosize background padding'.split(' '));
	function view(value: unknown): void {
		if (!object(value)) throw new Error('Every chart view must be an object.');
		if (++views > limits.chartLayers) throw new Error('Too many composed chart views.');
		known(value, viewKeys, 'chart');
		pixels += (typeof value.width === 'number' ? value.width : 300) * (typeof value.height === 'number' ? value.height : 240);
		for (const size of ['width', 'height', 'spacing', 'padding']) {
			if (value[size] !== undefined && value[size] !== 'container' && (typeof value[size] !== 'number' || (value[size] as number) < 0 || (value[size] as number) > 1600)) throw new Error(`Chart ${size} exceeds the display budget.`);
		}
		if (value.columns !== undefined && (!Number.isInteger(value.columns) || Number(value.columns) < 1 || Number(value.columns) > 6)) throw new Error('Chart columns must be between one and six.');
		if (value.mark !== undefined) {
			const mark = object(value.mark) ? value.mark : { type: value.mark };
			known(mark, markKeys, 'mark');
			if (!marks.has(String(mark.type))) throw new Error('Unsupported or unsafe chart mark.');
			for (const [key, val] of Object.entries(mark)) {
				if (object(val) && !['point', 'line'].includes(key)) throw new Error('Mark properties must be static styles.');
				if (object(val)) known(val, markKeys, 'nested mark');
				if (typeof val === 'number' && Math.abs(val) > 1600) throw new Error('Mark style exceeds the rendering budget.');
			}
		}
		if (value.encoding !== undefined) encoding(value.encoding);
		for (const key of ['layer', 'concat', 'hconcat', 'vconcat']) {
			if (value[key] !== undefined) {
				if (!Array.isArray(value[key]) || !(value[key] as unknown[]).length) throw new Error('Chart composition needs child views.');
				(value[key] as unknown[]).forEach(view);
			}
		}
		if (value.facet !== undefined) {
			const facet = value.facet;
			if (!object(facet)) throw new Error('Facet must reference saved fields.');
			if ('field' in facet) encoding({ facet }); else encoding(facet);
			const facetFields = 'field' in facet ? [facet.field] : Object.values(facet).map((entry) => object(entry) ? entry.field : null);
			const counts = facetFields.map((name) => new Set(result.rows.map((row) => row[result.columns.indexOf(String(name))])).size);
			if (counts.reduce((a, b) => a * b, 1) > limits.facets) throw new Error('Too many facets. Choose a smaller saved result.');
			repeatedPanels *= Math.max(1, counts.reduce((a, b) => a * b, 1));
		}
		if (value.repeat !== undefined) {
			const repeat = value.repeat;
			if (!Array.isArray(repeat) && !object(repeat)) throw new Error('Repeat must name saved fields.');
			if (object(repeat)) known(repeat, new Set(['row', 'column', 'layer']), 'repeat');
			const groups = Array.isArray(repeat) ? [repeat] : Object.values(repeat as Obj);
			let panels = 1;
			for (const group of groups) { if (!Array.isArray(group) || !group.length) throw new Error('Repeat must name saved fields.'); group.forEach(field); panels *= group.length; }
			if (panels > limits.facets) throw new Error('Too many repeated views.');
			repeatedPanels *= panels;
		}
		if (value.spec !== undefined) view(value.spec);
		if (value.resolve !== undefined) {
			if (!object(value.resolve)) throw new Error('Resolve must be an object.');
			known(value.resolve, new Set(['scale', 'axis', 'legend']), 'resolve');
			for (const group of Object.values(value.resolve)) {
				if (!object(group)) throw new Error('Resolve needs channel settings.');
				known(group, channels, 'resolve channel');
				if (Object.values(group).some((v) => v !== 'shared' && v !== 'independent')) throw new Error('Invalid resolve setting.');
			}
		}
		if (value.title !== undefined && typeof value.title !== 'string') throw new Error('Chart titles must be plain text.');
		if (value.autosize !== undefined && !['pad', 'fit', 'fit-x', 'fit-y', 'none'].includes(String(value.autosize))) throw new Error('Unsupported autosize setting.');
		if (!['mark', 'layer', 'concat', 'hconcat', 'vconcat', 'spec'].some((key) => key in value)) throw new Error('Chart has no mark or composed views.');
	}
	view(spec);
	if (pixels * repeatedPanels > 8_000_000 || result.rows.length * views * repeatedPanels > 500_000) throw new Error('Composed chart exceeds the total rendering budget.');
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

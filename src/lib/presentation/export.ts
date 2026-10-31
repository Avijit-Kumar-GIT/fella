import type { PresentationSpec, BoundResult, PresentationExport, PresentationState } from './types';
import { exactValue, filteredRows, requireResult, resolvePresentationState, validateResults } from './results';
import { validatePresentation } from './validate';
import { formatPresentationValue, presentationColumnLabel, type PresentationFormatOptions } from './format';

export interface PresentationExportOptions {
	state?: PresentationState;
	chartSnapshots?: Record<string, string>;
	interactionScope?: string;
}

function escape(value: unknown): string {
	return String(value ?? '').replace(/\\/g, '\\\\').replace(/\|/g, '\\|').replace(/[\r\n]+/g, ' ').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

function scope(result: BoundResult): string {
	return result.complete ? `Saved result ${result.id} contains ${result.rows.length} rows.` : `Incomplete saved result ${result.id}: ${result.rows.length} of ${result.row_count} rows. This is not the full dataset.`;
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return !!value && typeof value === 'object' && !Array.isArray(value);
}

function resultRows(result: BoundResult, indices: number[], fields: string[], formats: Record<string, PresentationFormatOptions> = {}): string {
	const columns = fields.filter((field) => result.columns.includes(field));
	if (!columns.length) return '_No saved fields were selected._';
	const header = `| ${columns.map((field) => escape(presentationColumnLabel(field, formats[field]))).join(' | ')} |`;
	const separator = `| ${columns.map(() => '---').join(' | ')} |`;
	const body = indices.map((row) => `| ${columns.map((field) => {
		const exact = exactValue(result, row, field);
		const value = exact === 'null' ? null : exact;
		return escape(formats[field] ? formatPresentationValue(value, formats[field]).display : value);
	}).join(' | ')} |`);
	return [header, separator, ...body].join('\n');
}

function textFigure(spec: Record<string, unknown>, result: BoundResult, indices: number[]): string | null {
	const enc = spec.encoding as Record<string, Record<string, unknown>> | undefined;
	const x = enc?.x; const y = enc?.y;
	const mark = isRecord(spec.mark) ? spec.mark.type : spec.mark;
	if (!x || !y || !['bar', 'line', 'point'].includes(String(mark)) || typeof x.field !== 'string' || typeof y.field !== 'string' || !result.columns.includes(x.field) || !result.columns.includes(y.field) || indices.length > 36 || !indices.length) return null;
	const yIndex = result.columns.indexOf(y.field);
	const points = indices.map((index) => Number(result.rows[index][yIndex]));
	if (points.some((value) => !Number.isFinite(value))) return null;
	const max = Math.max(...points.map(Math.abs), 1);
	return ['```text', ...indices.map((index, i) => {
		const label = exactValue(result, index, x.field as string);
		const value = exactValue(result, index, y.field as string);
		const bar = '█'.repeat(Math.round(Math.abs(points[i]) / max * 24));
		return `${label}: ${points[i] < 0 ? '−' : ''}${bar} ${value}`;
	}), '```', '', 'Text figure is approximate; exact saved values follow.'].join('\n');
}

/** Deterministic spec-to-Markdown conversion; never reruns analysis or asks a model to recreate values. */
export function exportPresentation(spec: PresentationSpec, results: BoundResult[], options: PresentationExportOptions = {}): PresentationExport {
	const validation = validatePresentation(spec, results);
	const warnings: string[] = [];
	const referenced = new Set<string>();
	try { validateResults(results); } catch (error) {
		return { markdown: `Presentation unavailable: ${error instanceof Error ? error.message : 'invalid saved results'}`, warnings: ['Saved result bindings could not be exported.'], resultIds: [] };
	}
	if (!validation.valid || !validation.spec) {
		const fallback = results.map((result) => `## Saved result: ${escape(result.id)}\n\n${scope(result)}\n\n${resultRows(result, result.rows.map((_, i) => i), result.columns)}`).join('\n\n');
		return { markdown: `The saved presentation could not be validated. Exact saved results are included below.\n\n${fallback}`, warnings: validation.issues.map((issue) => issue.message), resultIds: [] };
	}
	const state = resolvePresentationState(validation.spec, options.state ?? {}, results, options.interactionScope);
	const elements = validation.spec.elements as Record<string, { type: string; props: Record<string, unknown>; children: string[] }>;
	function render(key: string): string {
		const node = elements[key]; const { type, props, children } = node;
		const childText = () => children.map(render).filter(Boolean).join('\n\n');
		if (type === 'Document') return childText();
		if (type === 'Title') return `${'#'.repeat(Math.min(4, Number(props.level ?? 2) + 1))} ${escape(props.content)}`;
		if (type === 'Section') return `### ${escape(props.title)}\n\n${childText()}`;
		if (type === 'Card') return `${props.title ? `#### ${escape(props.title)}\n\n` : ''}${props.detail ? `${escape(props.detail)}\n\n` : ''}${childText()}`;
		if (type === 'Text') return escape(props.content);
		if (type === 'Caption') return `_${escape(props.content)}_`;
		if (type === 'Badge') return `**${escape(props.content)}**`;
		if (type === 'Callout') return `> ${props.title ? `**${escape(props.title)}**\n> ` : ''}${escape(props.content)}`;
		if (type === 'Divider') return props.label ? `---\n\n_${escape(props.label)}_` : '---';
		if (type === 'Spacer') return '';
		if (type === 'Columns' || type === 'Grid' || type === 'Row' || type === 'TabPanel') return childText();
		if (type === 'Tabs') {
			const index = Number(state[`tab:${String(props.name)}`] ?? 0);
			const selected = children[index];
			return selected ? `**${escape((props.labels as string[])[index])}**\n\n${render(selected)}` : '';
		}
		if (type === 'Metric' || type === 'Progress') {
			const result = requireResult(results, String(props.resultId)); referenced.add(result.id);
			const field = String(props.field); const exact = exactValue(result, Number(props.row), field);
			const value = exact === 'null' ? '—' : formatPresentationValue(exact, { ...(type === 'Metric' && props.format ? { format: props.format as PresentationFormatOptions['format'] } : {}), ...(props.unit ? { unit: String(props.unit) } : {}) }).display;
			return `**${escape(props.title)}:** ${escape(value)}${props.detail ? `\n\n${escape(props.detail)}` : ''}`;
		}
		if (type === 'Table') {
			const result = requireResult(results, String(props.resultId)); referenced.add(result.id);
			const indices = filteredRows(result, state);
			const fields = (props.fields as string[] | undefined) ?? result.columns;
			const formats = (props.columnFormats as Record<string, PresentationFormatOptions> | undefined) ?? {};
			return `${props.title ? `#### ${escape(props.title)}\n\n` : ''}${scope(result)}\n\n${resultRows(result, indices, fields, formats)}`;
		}
		if (type === 'Chart') {
			const result = requireResult(results, String(props.resultId)); referenced.add(result.id);
			const indices = filteredRows(result, state);
			const snapshot = options.chartSnapshots?.[key];
			const chart = textFigure(props.spec as Record<string, unknown>, result, indices);
			if (snapshot && /^(?:[A-Za-z0-9_-]+\/)*[A-Za-z0-9_.-]+\.(?:png|svg)$/i.test(snapshot) && !snapshot.split('/').includes('..')) return `${props.title ? `#### ${escape(props.title)}\n\n` : ''}![${escape(props.title ?? 'Chart')}](${`./${snapshot}`})\n\n${scope(result)}`;
			if (chart) return `${props.title ? `#### ${escape(props.title)}\n\n` : ''}${chart}\n\n${scope(result)}\n\n${resultRows(result, indices, result.columns)}`;
			warnings.push(`Chart “${String(props.title ?? key)}” has no faithful text representation; its exact saved rows are included.`);
			return `${props.title ? `#### ${escape(props.title)}\n\n` : ''}Chart layout is retained in the interactive artifact but cannot be reproduced as Markdown.\n\n${scope(result)}\n\n${resultRows(result, indices, result.columns)}`;
		}
		if (type === 'ResultFilter') {
			const result = requireResult(results, String(props.resultId)); referenced.add(result.id);
			const value = state[`filter:${String(props.name)}`] as { value: string | null } | undefined;
			return `_${escape(props.title ?? props.field)}: ${escape(value?.value ?? 'All saved values')}_`;
		}
		if (type === 'Select' || type === 'SegmentedControl' || type === 'Checkbox') {
			warnings.push(`${type} is interactive in Fella; the selected display state is exported as text.`);
			return `_${escape(props.name ?? props.label)}: ${escape(state[`control:${String(props.stateKey)}`] ?? props.defaultValue ?? props.defaultChecked ?? '')}_`;
		}
		if (type === 'FollowUp' || type === 'Button') {
			warnings.push(`${type} requires an explicit click in Fella and is not executed by report export.`);
			return `_${escape(props.label)} (available in Fella)_`;
		}
		return '';
	}
	let markdown = render(validation.spec.root);
	for (const id of validation.resultIds) referenced.add(id);
	const appendices = [...referenced].map((id) => {
		const result = results.find((item) => item.id === id);
		return result ? `## Exact saved source values: ${escape(id)}\n\n${scope(result)}\n\n${resultRows(result, result.rows.map((_, i) => i), result.columns)}` : '';
	});
	if (appendices.length) markdown += `\n\n${appendices.join('\n\n')}`;
	return { markdown: markdown.trim(), warnings: [...new Set(warnings)], resultIds: [...referenced] };
}

export function exportResultTable(result: BoundResult, fields = result.columns, rows = result.rows.map((_, index) => index)): string {
	return `${scope(result)}\n\n${resultRows(result, rows, fields)}`;
}

import type { ElementNode } from '@openuidev/svelte-lang';
import type { BoundResult, PresentationExport, PresentationState } from './types';
import { exactValue, filteredRows, requireResult, resolvePresentationState, validateResults, type FilterSelection } from './results';
import { validatePresentation } from './validate';

export interface PresentationExportOptions {
	state?: PresentationState;
	/** Host-generated local relative paths, keyed by statementId or chart title. No fetching. */
	chartSnapshots?: Record<string, string>;
}

function escape(value: string): string {
	return value.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/([\\`*_{}\[\]|])/g, '\\$1').replace(/\r/g, '&#13;').replace(/\n/g, '&#10;');
}

function extent(result: BoundResult): string {
	return result.complete ? `Saved result: ${escape(result.id)} · ${result.rows.length} rows.` : `Incomplete saved result: ${escape(result.id)} · ${result.rows.length} of ${result.row_count} rows. Do not treat this as the full dataset.`;
}

export function exportResultTable(result: BoundResult, fields = result.columns, rows = result.rows.map((_, index) => index)): string {
	const header = `| ${fields.map(escape).join(' | ')} |`;
	const separator = `| ${fields.map(() => '---').join(' | ')} |`;
	return [header, separator, ...rows.map((row) => `| ${fields.map((field) => escape(exactValue(result, row, field))).join(' | ')} |`), '', extent(result)].join('\n');
}

function safeSnapshot(path: string | undefined): path is string {
	return !!path && /^(?:[A-Za-z0-9_-]+\/)*[A-Za-z0-9_.-]+\.(?:png|svg)$/i.test(path) && !path.split('/').includes('..');
}

/** Small deterministic text figure when representable; the exact table always accompanies it. */
function textFigure(spec: Record<string, unknown>, result: BoundResult, rows: number[]): string | null {
	const encoding = spec.encoding as Record<string, Record<string, unknown>> | undefined;
	const x = encoding?.x;
	const y = encoding?.y;
	if (!x || !y || !['bar', 'line', 'point'].includes(String(typeof spec.mark === 'object' ? (spec.mark as Record<string, unknown>).type : spec.mark)) || x.aggregate || y.aggregate || x.bin || y.bin || x.timeUnit || y.timeUnit || typeof x.field !== 'string' || typeof y.field !== 'string' || x.type !== 'nominal' || y.type !== 'quantitative' || rows.length > 24 || rows.length === 0 || ['layer', 'facet', 'repeat', 'concat', 'hconcat', 'vconcat'].some((key) => key in spec)) return null;
	const col = result.columns.indexOf(y.field);
	const values = rows.map((row) => result.rows[row][col]);
	if (values.some((value) => typeof value !== 'number' || !Number.isFinite(value))) return null;
	const max = Math.max(...(values as number[]).map(Math.abs), 1);
	const lines = rows.map((row, index) => {
		const label = exactValue(result, row, x.field as string).replace(/[\r\n\t]/g, ' ').replace(/`/g, "'");
		const bars = '█'.repeat(Math.round(Math.abs(values[index] as number) / max * 24));
		const value = exactValue(result, row, y.field as string).replace(/`/g, "'");
		return `${label}: ${(values[index] as number) < 0 ? '−' : ''}${bars}  ${value}`;
	});
	return `\`\`\`text\n${lines.join('\n')}\n\`\`\`\n\nText figure: bar lengths are approximate; the following table preserves exact saved values.`;
}

/** Deterministic traversal of the actual OpenUI parse tree. No model calls or analytical queries. */
export function exportPresentation(program: string, results: BoundResult[], options: PresentationExportOptions = {}): PresentationExport {
	const validation = validatePresentation(program, results);
	const warnings: string[] = [];
	const state = validation.root ? resolvePresentationState(validation.root, options.state ?? {}, results) : {};
	if (!validation.valid || !validation.root) {
		warnings.push(...validation.issues.map((issue) => issue.message));
		let tables = '';
		try { validateResults(results); tables = results.map((result) => `## Saved result: ${escape(result.id)}\n\n${exportResultTable(result)}`).join('\n\n'); }
		catch { warnings.push('Invalid result bindings cannot be exported.'); }
		return { markdown: `Presentation unavailable. The saved analysis has not changed.\n\n${tables}`.trim(), warnings, resultIds: validation.resultIds };
	}
	const selections = Object.entries(state).filter(([key]) => key.startsWith('filter:')).map(([key, value]) => {
		const filter = value as FilterSelection;
		return `${escape(key.slice(7))}: ${filter.value === null ? 'all saved values' : escape(filter.value)}`;
	});
	function walk(node: ElementNode, level = 2): string {
		const props = node.props;
		const children = () => (props.children as ElementNode[]).map((child) => walk(child, level + (node.typeName === 'Section' ? 1 : 0))).join('\n\n');
		switch (node.typeName) {
			case 'Document': case 'Columns': return children();
			case 'Section': return `${'#'.repeat(Math.min(6, level))} ${escape(String(props.title))}\n\n${children()}`;
			case 'Text': return escape(String(props.content));
			case 'Metric': {
				const result = requireResult(results, String(props.resultId));
				return `**${escape(String(props.title))}:** ${escape(exactValue(result, Number(props.row), String(props.field)))}${props.unit ? ` ${escape(String(props.unit))}` : ''}\n\nSaved value: ${escape(result.id)}, ${escape(String(props.field))}, row ${Number(props.row) + 1}.`;
			}
			case 'Table': {
				const result = requireResult(results, String(props.resultId));
				const rows = filteredRows(result, state);
				return `${rows.length !== result.rows.length ? `Snapshot: ${rows.length} of ${result.rows.length} saved rows match the displayed filters.\n\n` : ''}${exportResultTable(result, props.fields as string[] | undefined, rows)}`;
			}
			case 'Chart': {
				const result = requireResult(results, String(props.resultId));
				const spec = props.spec as Record<string, unknown>;
				const title = String(props.title ?? 'Visualization');
				const rows = filteredRows(result, state);
				const snapshot = options.chartSnapshots?.[node.statementId ?? title];
				const text = textFigure(spec, result, rows);
				let figure = text;
				if (safeSnapshot(snapshot)) figure = `![${escape(title)}](./${snapshot})`;
				else if (snapshot) warnings.push(`Snapshot for ${title} was not a safe local image path.`);
				if (!figure) { warnings.push(`${title}: this composed visualization is represented by its declarative specification and exact saved values; no image snapshot was supplied.`); figure = 'This visualization has no faithful portable text figure. Its exact saved values and declarative specification follow.'; }
				// JSON fences remain safe even for arbitrary textual titles inside the spec.
				const json = JSON.stringify(spec, null, 2).replace(/`/g, '\\u0060');
				return `### ${escape(title)}\n\n${figure}\n\n${rows.length !== result.rows.length ? `Filtered snapshot: ${rows.length} of ${result.rows.length} saved rows.\n\n` : ''}${exportResultTable(result, result.columns, rows)}\n\nDisplay specification (Vega-Lite):\n\n\`\`\`json\n${json}\n\`\`\``;
			}
			case 'Tabs': {
				const selected = Number(state[`tab:${String(props.name)}`] ?? 0);
				return (props.children as ElementNode[]).map((child, index) => `### ${escape(String((props.labels as string[])[index]))}${index === selected ? ' (selected view)' : ' (additional view)'}\n\n${walk(child, level + 1)}`).join('\n\n');
			}
			case 'ResultFilter': return `Display filter: ${escape(String(props.title ?? props.field))}. ${escape(String(props.resultId))} saved rows only; no source query is executed.`;
			case 'FollowUp': return `Optional follow-up: ${escape(String(props.label))} — ${escape(String(props.message))}. Not executed in this report.`;
			default: warnings.push(`Unsupported component ${node.typeName}.`); return `Unsupported presentation component: ${escape(node.typeName)}.`;
		}
	}
	const markdown = [selections.length ? `Display snapshot\n\n${selections.join('\n\n')}` : '', walk(validation.root)].filter(Boolean).join('\n\n');
	return { markdown, warnings, resultIds: validation.resultIds };
}

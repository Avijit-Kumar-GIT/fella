import { isReactiveAssign } from '@openuidev/lang-core';
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

function isRecord(value: unknown): value is Record<string, unknown> {
	return !!value && typeof value === 'object' && !Array.isArray(value);
}

function extent(result: BoundResult): string {
	return result.complete ? `Saved result: ${escape(result.id)} · ${result.rows.length} rows.` : `Incomplete saved result: ${escape(result.id)} · ${result.rows.length} of ${result.row_count} rows. Do not treat this as the full dataset.`;
}

function jsonValue(value: unknown): string {
	if (value === undefined) return 'undefined';
	if (typeof value === 'string') return value;
	if (typeof value === 'number' && Number.isFinite(value)) return String(value);
	if (typeof value === 'boolean' || value === null) return String(value);
	try { return JSON.stringify(value) ?? String(value); }
	catch { return '[value could not be serialized]'; }
}

function safeSnapshot(path: string | undefined): path is string {
	return !!path && /^(?:[A-Za-z0-9_-]+\/)*[A-Za-z0-9_.-]+\.(?:png|svg)$/i.test(path) && !path.split('/').includes('..');
}

function objectRows(value: unknown): Record<string, unknown>[] | null {
	if (!Array.isArray(value) || value.some((row) => !isRecord(row))) return null;
	return value as Record<string, unknown>[];
}

interface DerivedTableExport {
	markdown: string;
	containsDerivedValues: boolean;
}

/** Materialized expression values are snapshots, not provenance. Exact source rows are appended separately. */
function exportDerivedTable(result: BoundResult, fields: string[], rows: Record<string, unknown>[], title?: string): DerivedTableExport {
	const heading = title ? `### ${escape(title)}\n\n` : '';
	const header = `| ${fields.map((field) => `${escape(field)} †`).join(' | ')} |`;
	const separator = `| ${fields.map(() => '---').join(' | ')} |`;
	const body = rows.map((row) => `| ${fields.map((field) => escape(jsonValue(row[field]))).join(' | ')} |`);
	const scope = `Display snapshot: ${rows.length} locally derived row${rows.length === 1 ? '' : 's'} from saved result ${escape(result.id)} (${result.rows.length} loaded source rows).`;
	const caveat = '† Materialized display value; row lineage and exact numeric formatting are not retained in this snapshot. The exact saved source values are included below.';
	return {
		markdown: `${heading}${scope}\n\n${header}\n${separator}\n${body.join('\n')}\n\n${caveat}\n\n${extent(result)}`,
		containsDerivedValues: true
	};
}

/** Small deterministic text figure when representable; the exact table always accompanies it. */
function textFigure(spec: Record<string, unknown>, result: BoundResult, rows: number[], derivedRows?: Record<string, unknown>[]): string | null {
	const encoding = spec.encoding as Record<string, Record<string, unknown>> | undefined;
	const x = encoding?.x;
	const y = encoding?.y;
	const mark = typeof spec.mark === 'object' ? (spec.mark as Record<string, unknown>).type : spec.mark;
	if (!x || !y || !['bar', 'line', 'point'].includes(String(mark)) || x.aggregate || y.aggregate || x.bin || y.bin || x.timeUnit || y.timeUnit || typeof x.field !== 'string' || typeof y.field !== 'string' || x.type !== 'nominal' || y.type !== 'quantitative' || (derivedRows?.length ?? rows.length) > 24 || (derivedRows?.length ?? rows.length) === 0 || ['layer', 'facet', 'repeat', 'concat', 'hconcat', 'vconcat'].some((key) => key in spec)) return null;
	const values = (derivedRows ?? rows.map((index) => Object.fromEntries(result.columns.map((column, col) => [column, result.rows[index][col]]))))
		.map((row) => row[y.field as string]);
	if (values.some((value) => typeof value !== 'number' || !Number.isFinite(value))) return null;
	const max = Math.max(...(values as number[]).map(Math.abs), 1);
	const lines = (derivedRows ?? rows.map((index) => Object.fromEntries(result.columns.map((column, col) => [column, result.rows[index][col]]))))
		.map((row, index) => {
			const label = (derivedRows ? jsonValue(row[x.field as string]) : exactValue(result, rows[index], x.field as string)).replace(/[\r\n\t]/g, ' ').replace(/`/g, "'");
			const bars = '█'.repeat(Math.round(Math.abs(values[index] as number) / max * 24));
			const value = (derivedRows ? jsonValue(row[y.field as string]) : exactValue(result, rows[index], y.field as string)).replace(/`/g, "'");
			return `${label}: ${(values[index] as number) < 0 ? '−' : ''}${bars}  ${value}`;
		});
	return `\`\`\`text\n${lines.join('\n')}\n\`\`\`\n\nText figure: bar lengths are approximate; the following table preserves exact saved values where available.`;
}

function stateValue(state: PresentationState, prop: unknown): { found: boolean; value: unknown; target?: string } {
	if (isReactiveAssign(prop)) {
		const target = prop.target;
		const candidates = [target, target.startsWith('$') ? target.slice(1) : `$${target}`];
		for (const key of candidates) if (Object.hasOwn(state, key)) return { found: true, value: state[key], target };
		return { found: false, value: undefined, target };
	}
	return prop === undefined ? { found: false, value: undefined } : { found: true, value: prop };
}

function jsonFence(value: unknown): string {
	let serialized: string;
	try { serialized = JSON.stringify(value, null, 2) ?? 'null'; }
	catch { serialized = '[action details could not be serialized]'; }
	return `\`\`\`json\n${serialized.replace(/`/g, '\\u0060')}\n\`\`\``;
}

/** Deterministic traversal of the actual OpenUI parse tree. No model calls or analytical queries. */
export function exportPresentation(program: string, results: BoundResult[], options: PresentationExportOptions = {}): PresentationExport {
	const validation = validatePresentation(program, results);
	const warnings: string[] = [];
	const warned = new Set<string>();
	const warn = (message: string) => { if (!warned.has(message)) { warned.add(message); warnings.push(message); } };
	const rawState = options.state ?? {};
	const state = validation.root ? resolvePresentationState(validation.root, rawState, results) : {};
	const exactAppendixIds = new Set<string>();
	const exactAppendixNotes = new Map<string, BoundResult>();
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
	function queueExactSource(result: BoundResult, reason: string): void {
		exactAppendixIds.add(result.id);
		exactAppendixNotes.set(result.id, result);
		warn(`${reason} is display-derived; its exact saved source values are included in an appendix.`);
	}
	function queueReferencedExactSources(reason: string): void {
		const referenced = validation.resultIds.map((id) => results.find((result) => result.id === id)).filter((result): result is BoundResult => !!result);
		if (!referenced.length) {
			warn(`${reason} is a display snapshot, but the validated presentation did not identify its source result for an exact-value appendix.`);
			return;
		}
		for (const result of referenced) queueExactSource(result, reason);
	}
	function children(node: ElementNode, level: number): string {
		const nodes = Array.isArray(node.props.children) ? node.props.children as ElementNode[] : [];
		return nodes.map((child) => walk(child, level + (node.typeName === 'Section' ? 1 : 0))).filter(Boolean).join('\n\n');
	}
	function layout(node: ElementNode, level: number, description: string): string {
		const content = children(node, level);
		warn(`${node.typeName} layout was linearized in Markdown; item order and content are preserved.`);
		return `${description}\n\n${content}`;
	}
	function controlSnapshot(name: string, selected: { found: boolean; value: unknown; target?: string }, available?: string[]): string {
		warn(`${name} was exported as a static Markdown snapshot; its local interaction is unavailable in Markdown.`);
		const selectedLine = selected.found
			? `Selected value: **${escape(jsonValue(selected.value))}**${selected.target ? ` (local state \`${escape(selected.target)}\`)` : ''}.`
			: `Selected value: not saved${selected.target ? ` for local state \`${escape(selected.target)}\`` : ''}.`;
		const optionsLine = available?.length ? `\n\nAvailable options: ${available.map((item) => `\`${escape(item)}\``).join(', ')}.` : '';
		return `${selectedLine}${optionsLine}\n\n_Static snapshot; this control cannot be changed in Markdown._`;
	}
	function walk(node: ElementNode, level = 2): string {
		const props = node.props;
		switch (node.typeName) {
			case 'Document': return children(node, level);
			case 'Section': {
				const title = String(props.title ?? 'Section');
				return `${'#'.repeat(Math.min(6, level))} ${escape(title)}\n\n${children(node, level)}`;
			}
			case 'Card': {
				const heading = props.title ? `### ${escape(String(props.title))}\n\n` : '';
				const detail = props.detail ? `_${escape(String(props.detail))}_\n\n` : '';
				const tone = String(props.tone ?? 'plain');
				if (tone !== 'plain') warn(`Card tone “${tone}” is recorded as text because Markdown does not preserve Fella surface styling.`);
				const toneNote = tone !== 'plain' ? `_Card surface tone: ${escape(tone)}._\n\n` : '';
				return `${heading}${detail}${toneNote}${children(node, level)}`;
			}
			case 'Columns': return layout(node, level, `Layout snapshot: ${Number(props.count ?? 0) || 'responsive'} columns; content follows in source order.`);
			case 'Row': return layout(node, level, `Layout snapshot: horizontal row (gap ${String(props.gap ?? 'default')}; align ${String(props.align ?? 'default')}); content follows in source order.`);
			case 'Grid': return layout(node, level, `Layout snapshot: ${String(props.columns ?? 'responsive')} column grid; content follows in source order.`);
			case 'Divider': return props.label ? `---\n\n**${escape(String(props.label))}**` : '---';
			case 'Spacer': {
				const size = String(props.size ?? 'md');
				warn(`Spacer (${size}) has no visual equivalent in Markdown; its intended spacing is recorded as a source comment.`);
				return `<!-- Fella spacer: ${size} -->`;
			}
			case 'Title': {
				const requested = Number(props.level ?? 2);
				return `${'#'.repeat(Math.min(6, Math.max(1, requested)))} ${escape(String(props.content))}`;
			}
			case 'Text': return escape(String(props.content));
			case 'Caption': return `_${escape(String(props.content))}_`;
			case 'Badge': {
				const tone = String(props.tone ?? 'neutral');
				return `**[${escape(tone)} · ${escape(String(props.content))}]**`;
			}
			case 'Callout': {
				const tone = String(props.tone ?? 'neutral');
				const title = props.title ? `**${escape(String(props.title))}** — ` : '';
				return `> ${title}${escape(String(props.content))}\n>\n> _${escape(tone)} callout_`;
			}
			case 'Metric': {
				const title = String(props.title ?? 'Metric');
				if (typeof props.resultId === 'string' && typeof props.field === 'string' && typeof props.row === 'number') {
					const result = requireResult(results, props.resultId);
					const value = exactValue(result, props.row, props.field);
					return `**${escape(title)}:** ${escape(value)}${props.unit ? ` ${escape(String(props.unit))}` : ''}\n\nSaved value: ${escape(result.id)}, ${escape(props.field)}, row ${props.row + 1}.${props.format ? ` Requested display format: ${escape(String(props.format))}; exact saved value shown.` : ''}${props.detail ? `\n\n${escape(String(props.detail))}` : ''}`;
				}
				const selected = stateValue(rawState, props.value);
				const value = selected.found ? jsonValue(selected.value) : 'not available in the saved snapshot';
				if (typeof props.resultId === 'string') queueExactSource(requireResult(results, props.resultId), `Metric “${title}”`);
				else queueReferencedExactSources(`Metric “${title}”`);
				return `**${escape(title)}:** ${escape(value)}${props.unit ? ` ${escape(String(props.unit))}` : ''}\n\n_Display-derived snapshot; this value is not an exact saved result cell.${props.format ? ` Requested display format: ${escape(String(props.format))}.` : ''}_${selected.target ? ` Local state binding: \`${escape(selected.target)}\`.` : ''}${props.detail ? `\n\n${escape(String(props.detail))}` : ''}`;
			}
			case 'Select': case 'SegmentedControl': {
				const title = String(props.name ?? node.typeName);
				const selected = stateValue(rawState, props.value);
				const options = Array.isArray(props.options) ? (props.options as { value?: unknown; label?: unknown }[]).map((option) => `${String(option.label ?? option.value ?? '')} = ${String(option.value ?? '')}`) : [];
				return `**${escape(title)}**\n\n${controlSnapshot(title, selected, options)}`;
			}
			case 'Checkbox': {
				const title = String(props.label ?? 'Checkbox');
				const selected = stateValue(rawState, props.checked);
				return `**${escape(title)}**\n\n${controlSnapshot(title, selected)}`;
			}
			case 'Button': {
				const label = String(props.label ?? 'Button');
				const action = props.action;
				warn(`Button “${label}” was exported with its action description; the action was not executed.`);
				return `**${escape(label)}**\n\n_Action not executed in Markdown._${action === undefined ? '' : `\n\nAction definition (for reference only):\n\n${jsonFence(action)}`}`;
			}
			case 'Table': {
				const result = requireResult(results, String(props.resultId));
				const title = typeof props.title === 'string' ? `### ${escape(props.title)}\n\n` : '';
				if (props.rows !== undefined) {
					const derivedRows = objectRows(props.rows);
					if (!derivedRows) {
						warn(`Table${props.title ? ` “${String(props.title)}”` : ''} has a local row expression that was not materialized for Markdown; the exact saved source result is exported instead.`);
						return `${title}Local row expression unavailable in this Markdown snapshot.\n\n${exportResultTable(result, props.fields as string[] | undefined)}`;
					}
					const fields = (props.fields as string[] | undefined) ?? [...new Set(derivedRows.flatMap((row) => Object.keys(row)))];
					if (!fields.length) fields.push(...result.columns);
					const exported = exportDerivedTable(result, fields, derivedRows);
					if (exported.containsDerivedValues) queueExactSource(result, `Table${props.title ? ` “${String(props.title)}”` : ''}`);
					return `${title}${exported.markdown}`;
				}
				const rows = filteredRows(result, state);
				const scope = rows.length !== result.rows.length ? `Snapshot: ${rows.length} of ${result.rows.length} loaded saved rows match the displayed filters.\n\n` : '';
				return `${title}${scope}${exportResultTable(result, props.fields as string[] | undefined, rows)}`;
			}
			case 'Chart': {
				const result = requireResult(results, String(props.resultId));
				const spec = props.spec as Record<string, unknown>;
				const title = String(props.title ?? 'Visualization');
				const derivedRows = props.rows === undefined ? null : objectRows(props.rows);
				if (props.rows !== undefined && !derivedRows) warn(`${title} has a local row expression that was not materialized for Markdown; the exact saved source result is exported instead.`);
				const rows = derivedRows ? [] : filteredRows(result, state);
				const snapshot = options.chartSnapshots?.[node.statementId ?? title];
				let figure = derivedRows ? textFigure(spec, result, [], derivedRows) : textFigure(spec, result, rows);
				if (safeSnapshot(snapshot)) figure = `![${escape(title)}](./${snapshot})`;
				else if (snapshot) warn(`Snapshot for ${title} was not a safe local image path.`);
				if (!figure) { warn(`${title}: this composed visualization is represented by its declarative specification and exact saved values; no image snapshot was supplied.`); figure = 'This visualization has no faithful portable text figure. Its exact saved values and declarative specification follow.'; }
				const derivedFields = derivedRows?.length ? [...new Set(derivedRows.flatMap((row) => Object.keys(row)))] : result.columns;
				const dataTable = derivedRows
					? exportDerivedTable(result, derivedFields, derivedRows)
					: { markdown: exportResultTable(result, result.columns, rows), containsDerivedValues: false };
				if (derivedRows && dataTable.containsDerivedValues) queueExactSource(result, title);
				const json = JSON.stringify(spec, null, 2).replace(/`/g, '\\u0060');
				const selectedCount = derivedRows ? derivedRows.length : rows.length;
				const displayedExtent = selectedCount !== result.rows.length ? `Displayed snapshot: ${selectedCount} of ${result.rows.length} saved rows (${selectedCount} row${selectedCount === 1 ? '' : 's'} from the loaded result).\n\n` : '';
				return `### ${escape(title)}\n\n${figure}\n\n${displayedExtent}${dataTable.markdown}\n\nDisplay specification (Vega-Lite):\n\n\`\`\`json\n${json}\n\`\`\``;
			}
			case 'Progress': {
				const title = String(props.title ?? 'Progress');
				const selected = stateValue(rawState, props.value);
				const value = selected.found ? selected.value : undefined;
				const numeric = typeof value === 'number' && Number.isFinite(value) ? value : null;
				const visual = numeric === null ? 0 : Math.round(Math.max(0, Math.min(100, numeric)));
				queueReferencedExactSources(`Progress “${title}”`);
				return `**${escape(title)}:** ${escape(jsonValue(value))} (indicator displays ${visual}%; clamped to 0–100 and rounded).\n\n_Display-derived snapshot, not a separately verified saved result._${props.detail ? `\n\n${escape(String(props.detail))}` : ''}`;
			}
			case 'Tabs': {
				const labels = props.labels as string[];
				const selected = Number(state[`tab:${String(props.name)}`] ?? 0);
				warn(`Tabs “${String(props.name)}” were exported as sections; the selected tab is marked and tab switching is unavailable in Markdown.`);
				return (props.children as ElementNode[]).map((child, index) => `### ${escape(String(labels[index]))}${index === selected ? ' (selected view; static snapshot)' : ' (additional view)'}\n\n${walk(child, level + 1)}`).join('\n\n');
			}
			case 'ResultFilter': {
				const name = String(props.name);
				const key = `filter:${name}`;
				const selected = (state[key] as FilterSelection | undefined) ?? { resultId: String(props.resultId), field: String(props.field), value: null };
				const result = requireResult(results, String(props.resultId));
				const index = result.columns.indexOf(String(props.field));
				const available = [...new Set(result.rows.map((_, row) => exactValue(result, row, String(props.field))))];
				const scope = selected.value === null ? 'all saved values' : `\`${escape(selected.value)}\``;
				return `**${escape(String(props.title ?? props.field))}**\n\nSelected snapshot: ${scope}.\n\nAvailable saved values: ${available.map((value) => `\`${escape(value)}\``).join(', ')}.\n\n_Static snapshot; this filter cannot be changed in Markdown. It filters saved rows only._${index < 0 ? `\n\nWarning: field ${escape(String(props.field))} is unavailable in the saved result.` : ''}`;
			}
			case 'FollowUp': {
				warn(`Follow-up “${String(props.label)}” is included for reference only and was not sent.`);
				return `Optional follow-up: **${escape(String(props.label))}** — ${escape(String(props.message))}. Not executed in this report.`;
			}
			default:
				warn(`Unsupported component ${node.typeName} was not rendered; its presence is recorded below.`);
				return `Unsupported presentation component: ${escape(node.typeName)}.`;
		}
	}
	const body = walk(validation.root);
	const exactAppendices = [...exactAppendixIds].map((id) => {
		const result = exactAppendixNotes.get(id);
		return result ? `## Exact saved source values: ${escape(id)}\n\n${exportResultTable(result)}` : '';
	}).filter(Boolean);
	const markdown = [selections.length ? `Display snapshot\n\n${selections.join('\n\n')}` : '', body, exactAppendices.length ? `## Exact source values for display-derived content\n\n${exactAppendices.join('\n\n')}` : ''].filter(Boolean).join('\n\n');
	return { markdown, warnings, resultIds: validation.resultIds };
}

export function exportResultTable(result: BoundResult, fields = result.columns, rows = result.rows.map((_, index) => index)): string {
	const header = `| ${fields.map(escape).join(' | ')} |`;
	const separator = `| ${fields.map(() => '---').join(' | ')} |`;
	return [header, separator, ...rows.map((row) => `| ${fields.map((field) => escape(exactValue(result, row, field))).join(' | ')} |`), '', extent(result)].join('\n');
}

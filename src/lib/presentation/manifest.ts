import { z } from 'zod/v4';
import { componentDefinitions } from './schemas';
import { PRESENTATION_VERSION, type BoundResult } from './types';

export const PRESENTATION_EXAMPLE = 'root = Document([Section("Revenue", [Metric("revenue", "revenue", 0, "Q3 revenue", "USD"), Chart("revenue", {"mark":"bar","encoding":{"x":{"field":"quarter","type":"nominal"},"y":{"field":"revenue","type":"quantitative"}}}, "Quarterly revenue"), Table("revenue")])])';

export const PRESENTATION_RULES = [
	'Output compatible OpenUI Lang statements only, including root = Document([...]). Calls use positional arguments in schema property order. Component references are allowed.',
	'Use only the registered components. No expressions, $variables, Query, Mutation, Each, action plans, HTML, JavaScript, SVG markup, URLs, data or datasets.',
	'Metric, Table, Chart and ResultFilter resultId must name a supplied saved result; field names must be present in that result. Do not invent or copy analytical values into props or narrative.',
	'Chart spec is a JSON Vega-Lite object without $schema/data/datasets/config/transform/params/signals/expr/URLs. Use encoding fields, marks, layer, facet, repeat, concat, axes, legends and static styles. Expressions and literal positional/text values are prohibited. Field annotations must come from saved results.',
	'Declarative encoding aggregate/bin/timeUnit describe a saved-row display, not a new verified analytical computation. Do not present summaries of incomplete rows as full-dataset answers. No imputation.',
	'Layout should be readable and calm, not a grid of redundant cards. Use Section, Columns (1–3), Tabs only when useful. State names must be unique. Tabs labels and children must have equal lengths.',
	'ResultFilter changes only saved rows, never executes tools. Metric always points to its specified saved row. FollowUp requests a continuation only after a user click; it does not run code. Filter and Action are reserved OpenUI built-ins, not component names.',
	'Text is plain explanation. Preserve caveats, missing values and incomplete result scope. Never imply presentation validation proves analytical correctness.'
];

/** Pure, Node-importable manifest: no Svelte, renderer, Vega or browser imports. */
export function presentationSchema() {
	return {
		$defs: Object.fromEntries(Object.entries(componentDefinitions).map(([name, definition]) => [name, z.toJSONSchema(definition.props)])),
		properties: Object.fromEntries(Object.keys(componentDefinitions).map((name) => [name, { $ref: `#/$defs/${name}` }]))
	};
}

function typeLabel(value: unknown): string {
	const schema = value as Record<string, unknown>;
	if (Array.isArray(schema.enum)) return schema.enum.map((v) => JSON.stringify(v)).join(' | ');
	if (schema.type === 'array') return `${typeLabel(schema.items)}[]`;
	if (schema.type === 'object') return 'object';
	return String(schema.type ?? 'component');
}

/** Generated from exactly the same ordered Zod definitions used by the native registry. */
export function presentationPrompt(results: Pick<BoundResult, 'id' | 'columns' | 'row_count' | 'complete'>[] = []): string {
	const schema = presentationSchema();
	const signatures = Object.entries(componentDefinitions).map(([name, definition]) => {
		const def = schema.$defs[name] as Record<string, unknown>;
		const required = (def.required ?? []) as string[];
		const props = def.properties as Record<string, unknown>;
		return `${name}(${Object.entries(props).map(([key, prop]) => `${key}${required.includes(key) ? '' : '?'}: ${typeLabel(prop)}`).join(', ')}) — ${definition.description}`;
	});
	return [`Fella analytical presentation library ${PRESENTATION_VERSION}.`, ...PRESENTATION_RULES, '', ...signatures, '', 'Example (use actual supplied result IDs and fields, not these example names):', PRESENTATION_EXAMPLE, '', 'Available saved result bindings (metadata only; not replacement analytical data):', JSON.stringify(results)].join('\n');
}

export function presentationManifest() {
	return { version: PRESENTATION_VERSION, schema: presentationSchema(), prompt: presentationPrompt(), rules: PRESENTATION_RULES, example: PRESENTATION_EXAMPLE };
}
